//! Userspace IPC client for the Keel Core Audio HAL virtual driver.
//!
//! Shared-memory ring layout mirrors a BlackHole-style HAL ↔ app bridge.
//! Driver sources live in `packaging/macos/KeelAudioVirtual.driver` and
//! `crates/keel-audio-hal/native/`.
//!
//! Auto-install / upgrade: see [`install`].

pub mod install;
pub mod shm;

pub use install::{
    ensure_driver_installed, probe_driver, reinstall_driver, DriverReport, DriverStatus,
    BUNDLED_DRIVER_BUILD, BUNDLED_DRIVER_VERSION, SYSTEM_DRIVER_DIR,
};
pub use shm::ShmRing;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use tracing::warn;

pub const SHM_NAME: &str = "keel_audio_hal_v1";
pub const MAGIC: u32 = 0x4B4C4841; // "KLHA"
pub const MAX_DEVICES: usize = 16;
pub const RING_FRAMES: usize = 48000; // 1s @ 48k mono plane; stereo = 2 planes
pub const CHANNELS: usize = 2;

#[derive(Debug, Error)]
pub enum HalError {
    #[error("driver not installed")]
    NotInstalled,
    #[error("shm: {0}")]
    Shm(String),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, HalError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualDeviceDesc {
    pub id: u32,
    pub name: String,
    pub is_input: bool,
    pub is_output: bool,
}

/// Default virtual device set (FEATURE_LEDGER §B).
pub fn default_virtual_devices() -> Vec<VirtualDeviceDesc> {
    let mixes = [
        ("Keel Mix Personal", false, true),
        ("Keel Mix Chat", false, true),
        ("Keel Mix Stream", false, true),
        ("Keel Mix 4", false, true),
        ("Keel Mix 5", false, true),
        ("Keel MicFX", false, true),
    ];
    let inputs = [
        "Keel Game",
        "Keel Music",
        "Keel Browser",
        "Keel Voice Chat",
        "Keel SFX",
        "Keel System",
        "Keel Aux 1",
        "Keel Aux 2",
    ];
    let mut out = Vec::new();
    let mut id = 1u32;
    for (name, _i, o) in mixes {
        out.push(VirtualDeviceDesc {
            id,
            name: name.into(),
            is_input: false,
            is_output: o,
        });
        id += 1;
    }
    for name in inputs {
        out.push(VirtualDeviceDesc {
            id,
            name: name.into(),
            is_input: true,
            is_output: true, // apps write into these
        });
        id += 1;
    }
    out
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HalHeader {
    pub magic: u32,
    pub version: u32,
    pub sample_rate: u32,
    pub device_count: u32,
    pub write_idx: u32,
    pub read_idx: u32,
}

/// Client that maps shared memory when the HAL driver is loaded.
pub struct HalClient {
    pub devices: Vec<VirtualDeviceDesc>,
    pub report: DriverReport,
    ring: Option<ShmRing>,
}

impl HalClient {
    pub fn connect() -> Result<Self> {
        let devices = default_virtual_devices();
        let ring = match ShmRing::open_or_create(devices.len() as u32, 48_000) {
            Ok(r) => {
                for (i, d) in devices.iter().enumerate() {
                    r.init_device_slot(i, d.id, &d.name, d.is_input, d.is_output);
                }
                Some(r)
            }
            Err(e) => {
                warn!("HAL shm open failed: {e}");
                None
            }
        };
        Ok(Self {
            devices,
            report: probe_driver(),
            ring,
        })
    }

    /// Probe → install/upgrade if needed → re-probe.
    pub fn connect_and_ensure(skip_prompt: bool) -> Result<Self> {
        let _ = ensure_driver_installed(skip_prompt);
        Self::connect()
    }

    pub fn refresh_report(&mut self) {
        self.report = probe_driver();
    }

    pub fn is_installed(&self) -> bool {
        PathBuf::from(SYSTEM_DRIVER_DIR).exists()
    }

    pub fn needs_update(&self) -> bool {
        self.report.needs_install()
    }

    pub fn ensure_or_advise(&self) -> Result<()> {
        if self.is_installed() && !self.needs_update() {
            return Ok(());
        }
        Err(HalError::NotInstalled)
    }

    fn device_index(&self, device_id: u32) -> Option<usize> {
        self.devices.iter().position(|d| d.id == device_id)
    }

    /// Push interleaved stereo frames to a virtual output device ring.
    pub fn write_output(&self, device_id: u32, interleaved: &[f32]) -> Result<()> {
        let Some(ring) = self.ring.as_ref() else {
            return Err(HalError::Shm("not mapped".into()));
        };
        let idx = self
            .device_index(device_id)
            .ok_or_else(|| HalError::Msg(format!("unknown device {device_id}")))?;
        let _ = ring.write_interleaved(idx, interleaved);
        Ok(())
    }

    /// Pull interleaved frames from a virtual input device (apps → keel).
    pub fn read_input(&self, device_id: u32, out: &mut [f32]) -> Result<usize> {
        let Some(ring) = self.ring.as_ref() else {
            return Err(HalError::Shm("not mapped".into()));
        };
        let idx = self
            .device_index(device_id)
            .ok_or_else(|| HalError::Msg(format!("unknown device {device_id}")))?;
        Ok(ring.read_interleaved(idx, out))
    }
}

pub fn driver_bundle_path() -> Option<PathBuf> {
    Some(PathBuf::from(SYSTEM_DRIVER_DIR))
}

pub fn install_instructions() -> &'static str {
    "Install or update the Keel Audio virtual driver (admin password required):\n\
     • Automatic: keel-audio prompts on launch when missing/outdated\n\
     • Manual: packaging/macos/install-hal.sh\n\
     • Skip prompt: KEEL_SKIP_HAL_INSTALL=1 or --skip-hal-install"
}
