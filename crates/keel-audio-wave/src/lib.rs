//! Wave device discovery + control.
//!
//! Wave:3 / DX expose **USB Audio** (+ vendor interfaces), not always HID.
//! Discovery uses Core Audio names (macOS) plus HID when present, and known
//! Elgato PIDs. Control writes go through WaveAPI SessionAPI (vendor USB)
//! first, with HID as a fallback for older units.

pub mod session_api;

use parking_lot::RwLock;
use rusb::UsbContext;
use serde::{Deserialize, Serialize};
use session_api::{apply_device_state, ParamValue, SessionDevice};
use std::collections::HashSet;
use std::sync::Arc;
use thiserror::Error;
use tracing::{info, warn};

/// Corsair/Elgato USB vendor id (same as Stream Deck).
pub const ELGATO_VID: u16 = 0x0FD9;

/// Known Wave family product IDs (decimal from IOKit / REA).
pub const WAVE3_MK2_PID: u16 = 0x00CA; // 202 — Elgato Wave:3 MK.2
pub const WAVE3_PID: u16 = 0x0041;
pub const WAVE1_PID: u16 = 0x0040;
pub const WAVE_DX_PID: u16 = 0x0085;
pub const WAVE_XLR_PID: u16 = 0x006B;

#[derive(Debug, Error)]
pub enum WaveError {
    #[error("hid: {0}")]
    Hid(String),
    #[error("device not found")]
    NotFound,
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, WaveError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveFamily {
    Wave1,
    Wave3,
    WaveDx,
    WaveXlr,
    WaveXlrMk2,
    WaveXlrPro,
    XlrDock,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    /// Seen via Core Audio / USB Audio class.
    CoreAudio,
    /// HID control interface present.
    Hid,
    /// Both.
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveDeviceInfo {
    pub id: String,
    pub name: String,
    pub family: WaveFamily,
    pub serial: Option<String>,
    pub firmware: Option<String>,
    pub path: String,
    pub transport: Transport,
    /// Core Audio / cpal device name when known.
    #[serde(default)]
    pub audio_device_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveDeviceState {
    pub info: WaveDeviceInfo,
    pub input_gain_db: f32,
    pub gain_lock: bool,
    pub mute: bool,
    pub clipguard: bool,
    pub headphone_volume: f32,
    pub mic_pc_mix: f32,
    pub direct_monitoring: bool,
    pub lowcut: bool,
    pub lowcut_enhanced: bool,
    pub phantom_48v: bool,
    pub low_impedance: bool,
    pub led_direction_flipped: bool,
    pub led_color_general: [u8; 3],
    pub led_color_mute: [u8; 3],
    pub led_color_clipguard: [u8; 3],
    pub lighting_effect: String,
}

impl WaveDeviceState {
    pub fn new(info: WaveDeviceInfo) -> Self {
        Self {
            info,
            input_gain_db: 20.0,
            gain_lock: false,
            mute: false,
            clipguard: true,
            headphone_volume: 0.5,
            mic_pc_mix: 0.5,
            direct_monitoring: true,
            lowcut: false,
            lowcut_enhanced: false,
            phantom_48v: false,
            low_impedance: false,
            led_direction_flipped: false,
            led_color_general: [0, 180, 255],
            led_color_mute: [255, 40, 40],
            led_color_clipguard: [255, 180, 0],
            lighting_effect: "static".into(),
        }
    }
}

/// Known WaveAPI-style control paths (from REA `waveapi-paths.txt`).
pub mod paths {
    pub use crate::session_api::path::*;
    pub const LINE_VOLUME: &str = "/line/volume";
    pub const LINE_MUTE: &str = "/line/mute";
    /// USB host playback levels (Wave:3 mic↔PC headphone blend).
    pub const USB_HOST_PLAYBACK_0: &str = "/usb_host_playback/level/0";
    pub const USB_HOST_PLAYBACK_1: &str = "/usb_host_playback/level/1";
}

pub struct WaveManager {
    devices: RwLock<Vec<WaveDeviceState>>,
}

impl WaveManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            devices: RwLock::new(Vec::new()),
        })
    }

    pub fn refresh(&self) -> Result<()> {
        let mut by_key: Vec<WaveDeviceState> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // 1) Core Audio — Wave:3 MK.2 shows up here, not as HID.
        for info in discover_coreaudio_wave() {
            let key = normalize_key(&info.name, info.serial.as_deref());
            if seen.insert(key) {
                by_key.push(WaveDeviceState::new(info));
            }
        }

        // 2) HID — some Wave / XLR units expose a control interface.
        match hidapi::HidApi::new() {
            Ok(api) => {
                for dev in api.device_list() {
                    if dev.vendor_id() != ELGATO_VID {
                        continue;
                    }
                    let product = dev.product_string().unwrap_or("").to_string();
                    let product_lc = product.to_lowercase();
                    let pid = dev.product_id();
                    if !is_wave_product(&product_lc, pid) {
                        continue;
                    }
                    let serial = dev.serial_number().map(|s| s.to_string());
                    let key = normalize_key(&product, serial.as_deref());
                    if let Some(existing) = by_key
                        .iter_mut()
                        .find(|d| normalize_key(&d.info.name, d.info.serial.as_deref()) == key)
                    {
                        existing.info.transport = Transport::Both;
                        existing.info.path = dev.path().to_string_lossy().into();
                        continue;
                    }
                    if !seen.insert(key) {
                        continue;
                    }
                    let family = classify_family(&product_lc, pid);
                    let id = format!(
                        "{:04x}:{:04x}:{}",
                        dev.vendor_id(),
                        pid,
                        serial.as_deref().unwrap_or("unknown")
                    );
                    let info = WaveDeviceInfo {
                        id,
                        name: if product.is_empty() {
                            format!("Wave Device (0x{pid:04x})")
                        } else {
                            product
                        },
                        family,
                        serial,
                        firmware: None,
                        path: dev.path().to_string_lossy().into(),
                        transport: Transport::Hid,
                        audio_device_name: None,
                    };
                    by_key.push(WaveDeviceState::new(info));
                }
            }
            Err(e) => {
                warn!("hidapi init failed: {e}");
            }
        }

        enrich_usb_serials(&mut by_key);

        if by_key.is_empty() {
            info!("no Wave devices present (Core Audio + HID)");
        } else {
            info!(
                "found {} Wave device(s): {}",
                by_key.len(),
                by_key
                    .iter()
                    .map(|d| format!("{} [{:?}]", d.info.name, d.info.transport))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        *self.devices.write() = by_key;
        Ok(())
    }

    pub fn list(&self) -> Vec<WaveDeviceState> {
        self.devices.read().clone()
    }

    pub fn get_mut_apply<F: FnOnce(&mut WaveDeviceState)>(&self, id: &str, f: F) -> Result<()> {
        let mut list = self.devices.write();
        let dev = list
            .iter_mut()
            .find(|d| d.info.id == id)
            .ok_or(WaveError::NotFound)?;
        f(dev);
        let _ = write_control(dev);
        Ok(())
    }

    pub fn set_gain(&self, id: &str, gain_db: f32) -> Result<()> {
        self.get_mut_apply(id, |d| {
            if !d.gain_lock {
                d.input_gain_db = gain_db.clamp(0.0, 60.0);
            }
        })
    }

    pub fn set_gain_lock(&self, id: &str, lock: bool) -> Result<()> {
        self.get_mut_apply(id, |d| d.gain_lock = lock)
    }

    pub fn set_mute(&self, id: &str, mute: bool) -> Result<()> {
        self.get_mut_apply(id, |d| d.mute = mute)
    }

    pub fn set_clipguard(&self, id: &str, on: bool) -> Result<()> {
        self.get_mut_apply(id, |d| d.clipguard = on)
    }

    pub fn set_headphone_volume(&self, id: &str, vol: f32) -> Result<()> {
        self.get_mut_apply(id, |d| d.headphone_volume = vol.clamp(0.0, 1.0))
    }

    pub fn set_mic_pc_mix(&self, id: &str, mix: f32) -> Result<()> {
        self.get_mut_apply(id, |d| d.mic_pc_mix = mix.clamp(0.0, 1.0))
    }

    /// Auto Gain Wizard — measures peak into target ~-12 dBFS.
    pub fn auto_gain_wizard(&self, id: &str, measured_peak: f32) -> Result<f32> {
        let target = 10f32.powf(-12.0 / 20.0);
        let mut new_gain = 20.0;
        self.get_mut_apply(id, |d| {
            if measured_peak > 1e-6 {
                let delta_db = 20.0 * (target / measured_peak).log10();
                d.input_gain_db = (d.input_gain_db + delta_db).clamp(0.0, 60.0);
            }
            new_gain = d.input_gain_db;
        })?;
        Ok(new_gain)
    }
}

fn normalize_key(name: &str, serial: Option<&str>) -> String {
    let n = name.to_lowercase().replace(' ', "");
    format!("{n}|{}", serial.unwrap_or(""))
}

fn is_wave_product(product_lc: &str, pid: u16) -> bool {
    if matches!(
        pid,
        WAVE3_MK2_PID | WAVE3_PID | WAVE1_PID | WAVE_DX_PID | WAVE_XLR_PID
    ) {
        return true;
    }
    if product_lc.contains("stream deck") || product_lc.contains("streamdeck") {
        return false;
    }
    product_lc.contains("wave") || product_lc.contains("xlr")
}

fn classify_family(product: &str, pid: u16) -> WaveFamily {
    if product.contains("xlr pro") {
        WaveFamily::WaveXlrPro
    } else if product.contains("xlr") && product.contains("mk") {
        WaveFamily::WaveXlrMk2
    } else if product.contains("xlr") || pid == WAVE_XLR_PID {
        WaveFamily::WaveXlr
    } else if product.contains("dock") {
        WaveFamily::XlrDock
    } else if product.contains("dx") || product.contains("neo") || pid == WAVE_DX_PID {
        WaveFamily::WaveDx
    } else if product.contains("wave:3")
        || product.contains("wave 3")
        || product.contains("wave3")
        || pid == WAVE3_PID
        || pid == WAVE3_MK2_PID
    {
        WaveFamily::Wave3
    } else if product.contains("wave:1")
        || product.contains("wave 1")
        || product.contains("wave1")
        || pid == WAVE1_PID
    {
        WaveFamily::Wave1
    } else {
        WaveFamily::Unknown
    }
}

fn discover_coreaudio_wave() -> Vec<WaveDeviceInfo> {
    #[cfg(target_os = "macos")]
    {
        discover_coreaudio_wave_macos()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

#[cfg(target_os = "macos")]
fn discover_coreaudio_wave_macos() -> Vec<WaveDeviceInfo> {
    // Wave:3 MK.2 is USB Audio — visible to Core Audio / cpal, not hidapi.
    discover_coreaudio_wave_via_cpal()
}

#[cfg(target_os = "macos")]
fn discover_coreaudio_wave_via_cpal() -> Vec<WaveDeviceInfo> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let mut out = Vec::new();
    let host = cpal::default_host();
    let mut names: HashSet<String> = HashSet::new();

    let mut consider = |name: String| {
        let name_lc = name.to_lowercase();
        if name_lc.contains("stream deck") {
            return;
        }
        if !(name_lc.contains("wave") || name_lc.contains("xlr")) {
            return;
        }
        if !names.insert(name.clone()) {
            return;
        }
        let family = classify_family(&name_lc, 0);
        out.push(WaveDeviceInfo {
            id: format!("coreaudio:{}", name.replace(' ', "_")),
            name: name.clone(),
            family,
            serial: None,
            firmware: None,
            path: format!("coreaudio://{name}"),
            transport: Transport::CoreAudio,
            audio_device_name: Some(name),
        });
    };

    if let Ok(inputs) = host.input_devices() {
        for d in inputs {
            if let Ok(n) = d.name() {
                consider(n);
            }
        }
    }
    if let Ok(outputs) = host.output_devices() {
        for d in outputs {
            if let Ok(n) = d.name() {
                consider(n);
            }
        }
    }
    out
}

fn write_control(dev: &WaveDeviceState) -> Result<()> {
    // Prefer WaveAPI SessionAPI over vendor USB (Wave:3 MK.2 has no HID).
    match apply_device_state(
        dev.info.serial.as_deref(),
        dev.input_gain_db,
        dev.gain_lock,
        dev.mute,
        dev.clipguard,
        dev.headphone_volume,
        dev.direct_monitoring,
    ) {
        Ok(()) => {
            if let Err(e) = write_mic_pc_mix(dev) {
                warn!("mic/pc mix SessionAPI: {e}");
            }
            return Ok(());
        }
        Err(e) => {
            warn!("SessionAPI write failed ({e}); trying HID fallback");
        }
    }

    if matches!(dev.info.transport, Transport::CoreAudio) {
        return Ok(());
    }
    let api = hidapi::HidApi::new().map_err(|e| WaveError::Hid(e.to_string()))?;
    let c_path = std::ffi::CString::new(dev.info.path.as_str())
        .map_err(|e| WaveError::Msg(e.to_string()))?;
    let Ok(device) = api.open_path(&c_path) else {
        return Ok(());
    };
    let mut report = [0u8; 64];
    report[0] = 0x01;
    report[1] = if dev.mute { 1 } else { 0 };
    report[2] = if dev.clipguard { 1 } else { 0 };
    report[3] = if dev.gain_lock { 1 } else { 0 };
    report[4] = (dev.input_gain_db.clamp(0.0, 60.0) as u8).min(60);
    report[5] = (dev.headphone_volume * 100.0) as u8;
    report[6] = (dev.mic_pc_mix * 100.0) as u8;
    report[7] = if dev.phantom_48v { 1 } else { 0 };
    report[8] = if dev.low_impedance { 1 } else { 0 };
    let _ = device.write(&report);
    Ok(())
}

fn write_mic_pc_mix(dev: &WaveDeviceState) -> session_api::Result<()> {
    let session = match dev.info.serial.as_deref() {
        Some(s) if !s.is_empty() => {
            SessionDevice::open_by_serial(s).or_else(|_| SessionDevice::open_first())
        }
        _ => SessionDevice::open_first(),
    }?;
    let mic = dev.mic_pc_mix.clamp(0.0, 1.0);
    let pc = 1.0 - mic;
    // Wave:3 headphone blend — usb_host_playback levels (REA strings).
    session.set_param(paths::USB_HOST_PLAYBACK_0, ParamValue::Float(mic))?;
    session.set_param(paths::USB_HOST_PLAYBACK_1, ParamValue::Float(pc))?;
    Ok(())
}

/// Enrich Core Audio discoveries with USB serials from rusb when available.
pub fn enrich_usb_serials(devices: &mut [WaveDeviceState]) {
    let Ok(ctx) = rusb::Context::new() else {
        return;
    };
    let Ok(list) = ctx.devices() else {
        return;
    };
    let mut serials: Vec<(u16, String)> = Vec::new();
    for device in list.iter() {
        let Ok(desc) = device.device_descriptor() else {
            continue;
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
        let Ok(handle) = device.open() else {
            continue;
        };
        if let Ok(s) = handle.read_serial_number_string_ascii(&desc) {
            serials.push((pid, s));
        }
    }
    for (i, dev) in devices.iter_mut().enumerate() {
        if dev.info.serial.is_some() {
            continue;
        }
        if let Some((_, s)) = serials.get(i).or_else(|| serials.first()) {
            dev.info.serial = Some(s.clone());
            if !dev.info.id.contains(s) {
                dev.info.id = format!("usb:{s}");
            }
        }
    }
}
