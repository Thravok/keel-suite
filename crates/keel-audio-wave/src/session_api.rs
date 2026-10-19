//! WaveAPI SessionAPI framing over vendor USB (IOKit / LibUSB path).
//!
//! Wire format recovered from WaveAPI.framework strings + Ghidra notes
//! (`scratch/ghidra-waveapi/`): path-oriented Param get/set with typed
//! variants (bool / float). MK.2 devices use `MK2VendorUSBBackendStrategy`
//! (vendor class interface), not HID.
//!
//! Framing (Keel v1 — matches observed Message API versioning):
//! ```text
//! [0..3]   magic      u32 LE  = 0x4C575453  ("LWTS" Lewitt Session)
//! [4]      version    u8      = 1
//! [5]      op         u8      = 1 set / 2 get
//! [6]      value_ty   u8      = 1 bool / 2 float / 3 i32
//! [7]      path_len   u8
//! [8..]    path       UTF-8 (no NUL)
//! [8+n..]  value      bool:1 / f32 LE / i32 LE
//! ```
//! Sent on the first vendor interface (class 255 or 254) via interrupt OUT
//! when present, else control transfer `bmRequestType=0x21`, `bRequest=0x09`.

use rusb::{Context, DeviceHandle, Direction, TransferType, UsbContext};
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, warn};

use crate::{ELGATO_VID, WAVE1_PID, WAVE3_MK2_PID, WAVE3_PID, WAVE_DX_PID, WAVE_XLR_PID};

pub const SESSION_MAGIC: u32 = 0x4C57_5453; // "LWTS"
pub const SESSION_VERSION: u8 = 1;
pub const OP_SET: u8 = 1;
pub const OP_GET: u8 = 2;
pub const TY_BOOL: u8 = 1;
pub const TY_FLOAT: u8 = 2;
pub const TY_I32: u8 = 3;

const TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("usb: {0}")]
    Usb(String),
    #[error("no Wave vendor interface")]
    NoVendorInterface,
    #[error("path too long")]
    PathTooLong,
}

pub type Result<T> = std::result::Result<T, SessionError>;

/// SessionAPI control paths used by Wave:3 / DX (from REA waveapi-paths.txt).
pub mod path {
    pub const GAIN: &str = "/gain";
    pub const GAIN_LOCK: &str = "/gain_lock";
    pub const MIC_GAIN: &str = "/mic/gain";
    pub const MIC_MUTE: &str = "/mic/mute";
    pub const MIC_GAIN_LOCK: &str = "/mic/gain_lock";
    pub const INPUT_MUTE: &str = "/input_mute";
    pub const CLIPGUARD: &str = "/clipguard_enable";
    pub const HEADPHONE_VOLUME: &str = "/headphone_volume";
    pub const HEADPHONE_MUTE: &str = "/headphone_mute";
    pub const DIRECT_MONITOR: &str = "/direct_monitor";
    pub const LOWCUT: &str = "/lowcut_enable";
}

#[derive(Debug, Clone, Copy)]
pub enum ParamValue {
    Bool(bool),
    Float(f32),
    I32(i32),
}

impl ParamValue {
    fn ty(self) -> u8 {
        match self {
            Self::Bool(_) => TY_BOOL,
            Self::Float(_) => TY_FLOAT,
            Self::I32(_) => TY_I32,
        }
    }

    fn append_bytes(self, buf: &mut Vec<u8>) {
        match self {
            Self::Bool(b) => buf.push(if b { 1 } else { 0 }),
            Self::Float(f) => buf.extend_from_slice(&f.to_le_bytes()),
            Self::I32(i) => buf.extend_from_slice(&i.to_le_bytes()),
        }
    }
}

pub fn encode_set(path: &str, value: ParamValue) -> Result<Vec<u8>> {
    if path.len() > 200 {
        return Err(SessionError::PathTooLong);
    }
    let mut buf = Vec::with_capacity(8 + path.len() + 4);
    buf.extend_from_slice(&SESSION_MAGIC.to_le_bytes());
    buf.push(SESSION_VERSION);
    buf.push(OP_SET);
    buf.push(value.ty());
    buf.push(path.len() as u8);
    buf.extend_from_slice(path.as_bytes());
    value.append_bytes(&mut buf);
    Ok(buf)
}

/// Open the first matching Elgato Wave device and claim the vendor interface.
pub struct SessionDevice {
    handle: DeviceHandle<Context>,
    interface: u8,
    out_ep: Option<u8>,
}

impl SessionDevice {
    pub fn open_first() -> Result<Self> {
        let ctx = Context::new().map_err(|e| SessionError::Usb(e.to_string()))?;
        for device in ctx
            .devices()
            .map_err(|e| SessionError::Usb(e.to_string()))?
            .iter()
        {
            let desc = match device.device_descriptor() {
                Ok(d) => d,
                Err(_) => continue,
            };
            if desc.vendor_id() != ELGATO_VID {
                continue;
            }
            let pid = desc.product_id();
            if !matches!(
                pid,
                WAVE3_MK2_PID | WAVE3_PID | WAVE1_PID | WAVE_DX_PID | WAVE_XLR_PID
            ) {
                continue;
            }
            let handle = device
                .open()
                .map_err(|e| SessionError::Usb(format!("open: {e}")))?;
            return claim_vendor(handle);
        }
        Err(SessionError::NoVendorInterface)
    }

    pub fn open_by_serial(serial: &str) -> Result<Self> {
        let ctx = Context::new().map_err(|e| SessionError::Usb(e.to_string()))?;
        for device in ctx
            .devices()
            .map_err(|e| SessionError::Usb(e.to_string()))?
            .iter()
        {
            let desc = match device.device_descriptor() {
                Ok(d) => d,
                Err(_) => continue,
            };
            if desc.vendor_id() != ELGATO_VID {
                continue;
            }
            let Ok(handle) = device.open() else { continue };
            let Ok(s) = handle.read_serial_number_string_ascii(&desc) else {
                continue;
            };
            if s != serial {
                continue;
            }
            return claim_vendor(handle);
        }
        Err(SessionError::NoVendorInterface)
    }

    pub fn set_param(&self, path: &str, value: ParamValue) -> Result<()> {
        let packet = encode_set(path, value)?;
        self.send(&packet)
    }

    fn send(&self, packet: &[u8]) -> Result<()> {
        if let Some(ep) = self.out_ep {
            match self.handle.write_interrupt(ep, packet, TIMEOUT) {
                Ok(n) => {
                    debug!("session interrupt OUT {n} bytes ep=0x{ep:02x}");
                    return Ok(());
                }
                Err(e) => {
                    warn!("interrupt OUT failed ({e}); trying control transfer");
                }
            }
        }
        // Class interface OUT, SET_REPORT-style (HID-like vendor).
        let r = self.handle.write_control(
            rusb::request_type(
                Direction::Out,
                rusb::RequestType::Class,
                rusb::Recipient::Interface,
            ),
            0x09,
            0x0200,
            u16::from(self.interface),
            packet,
            TIMEOUT,
        );
        match r {
            Ok(n) => {
                debug!("session control OUT {n} bytes iface={}", self.interface);
                Ok(())
            }
            Err(e) => Err(SessionError::Usb(format!("control: {e}"))),
        }
    }
}

fn claim_vendor(handle: DeviceHandle<Context>) -> Result<SessionDevice> {
    let device = handle.device();
    let config = device
        .active_config_descriptor()
        .map_err(|e| SessionError::Usb(e.to_string()))?;

    let mut vendor_iface: Option<u8> = None;
    let mut out_ep: Option<u8> = None;

    for interface in config.interfaces() {
        for desc in interface.descriptors() {
            let class = desc.class_code();
            // Vendor (0xFF) or Elgato proprietary (0xFE) — not Audio (0x01) / HID (0x03).
            if class != 0xFF && class != 0xFE {
                continue;
            }
            let num = desc.interface_number();
            if handle.kernel_driver_active(num).unwrap_or(false) {
                let _ = handle.detach_kernel_driver(num);
            }
            if handle.claim_interface(num).is_err() {
                continue;
            }
            vendor_iface = Some(num);
            for ep in desc.endpoint_descriptors() {
                if ep.direction() == Direction::Out
                    && matches!(
                        ep.transfer_type(),
                        TransferType::Interrupt | TransferType::Bulk
                    )
                {
                    out_ep = Some(ep.address());
                    break;
                }
            }
            break;
        }
        if vendor_iface.is_some() {
            break;
        }
    }

    let interface = vendor_iface.ok_or(SessionError::NoVendorInterface)?;
    Ok(SessionDevice {
        handle,
        interface,
        out_ep,
    })
}

/// Best-effort: open device and apply a full soft state snapshot.
pub fn apply_device_state(
    serial: Option<&str>,
    gain_db: f32,
    gain_lock: bool,
    mute: bool,
    clipguard: bool,
    headphone_volume_norm: f32,
    direct_monitor: bool,
) -> Result<()> {
    let session = match serial {
        Some(s) if !s.is_empty() => {
            SessionDevice::open_by_serial(s).or_else(|_| SessionDevice::open_first())
        }
        _ => SessionDevice::open_first(),
    }?;

    // Wave:3 uses top-level /gain; some firmware also mirrors /mic/gain.
    session.set_param(path::GAIN, ParamValue::Float(gain_db))?;
    let _ = session.set_param(path::MIC_GAIN, ParamValue::Float(gain_db));
    session.set_param(path::GAIN_LOCK, ParamValue::Bool(gain_lock))?;
    session.set_param(path::MIC_MUTE, ParamValue::Bool(mute))?;
    session.set_param(path::INPUT_MUTE, ParamValue::Bool(mute))?;
    session.set_param(path::CLIPGUARD, ParamValue::Bool(clipguard))?;
    // Path is documented as dB; map 0..1 UI → ~-60..0 dB.
    let hp_db = if headphone_volume_norm <= 0.0 {
        -60.0
    } else {
        20.0 * headphone_volume_norm.log10()
    };
    session.set_param(path::HEADPHONE_VOLUME, ParamValue::Float(hp_db))?;
    session.set_param(path::DIRECT_MONITOR, ParamValue::Bool(direct_monitor))?;
    Ok(())
}
