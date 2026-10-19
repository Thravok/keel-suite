//! Auto-install / upgrade the Keel Core Audio HAL driver (macOS).
//!
//! Installing under `/Library/Audio/Plug-Ins/HAL/` needs admin rights. We prompt
//! once via `osascript` (`with administrator privileges`) — same UX as many
//! local audio tools. No background daemon, no telemetry.
//!
//! Versioning: compare installed `CFBundleVersion` (integer build) to
//! [`BUNDLED_DRIVER_BUILD`]. Bump both this constant and the driver's
//! `Info.plist` whenever the HAL binary changes.

use crate::{driver_bundle_path, HalError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

/// Must match `CFBundleVersion` in `packaging/macos/KeelAudioVirtual.driver/Contents/Info.plist`.
pub const BUNDLED_DRIVER_BUILD: u32 = 2;
/// Must match `CFBundleShortVersionString` in the same Info.plist.
pub const BUNDLED_DRIVER_VERSION: &str = "0.1.0";

pub const SYSTEM_DRIVER_DIR: &str = "/Library/Audio/Plug-Ins/HAL/KeelAudioVirtual.driver";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverStatus {
    /// No bundle at the system HAL path.
    Missing,
    /// Installed build matches (or exceeds) the bundled build.
    Current,
    /// Installed build is older than what this keel-audio ships.
    Outdated,
    /// Bundle exists but Info.plist could not be read.
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverReport {
    pub status: DriverStatus,
    pub installed_build: Option<u32>,
    pub installed_version: Option<String>,
    pub bundled_build: u32,
    pub bundled_version: String,
    pub system_path: String,
    pub source_path: Option<String>,
}

impl DriverReport {
    pub fn needs_install(&self) -> bool {
        matches!(self.status, DriverStatus::Missing | DriverStatus::Outdated)
    }
}

/// Inspect system driver vs bundled build.
pub fn probe_driver() -> DriverReport {
    let system = PathBuf::from(SYSTEM_DRIVER_DIR);
    let source = resolve_bundled_driver_dir();
    if !system.exists() {
        return DriverReport {
            status: DriverStatus::Missing,
            installed_build: None,
            installed_version: None,
            bundled_build: BUNDLED_DRIVER_BUILD,
            bundled_version: BUNDLED_DRIVER_VERSION.into(),
            system_path: SYSTEM_DRIVER_DIR.into(),
            source_path: source.map(|p| p.display().to_string()),
        };
    }
    match read_plist_versions(&system.join("Contents/Info.plist")) {
        Some((build, ver)) => {
            let status = if build >= BUNDLED_DRIVER_BUILD {
                DriverStatus::Current
            } else {
                DriverStatus::Outdated
            };
            DriverReport {
                status,
                installed_build: Some(build),
                installed_version: Some(ver),
                bundled_build: BUNDLED_DRIVER_BUILD,
                bundled_version: BUNDLED_DRIVER_VERSION.into(),
                system_path: SYSTEM_DRIVER_DIR.into(),
                source_path: source.map(|p| p.display().to_string()),
            }
        }
        None => DriverReport {
            status: DriverStatus::Unknown,
            installed_build: None,
            installed_version: None,
            bundled_build: BUNDLED_DRIVER_BUILD,
            bundled_version: BUNDLED_DRIVER_VERSION.into(),
            system_path: SYSTEM_DRIVER_DIR.into(),
            source_path: source.map(|p| p.display().to_string()),
        },
    }
}

/// Ensure the system driver is present and at least `BUNDLED_DRIVER_BUILD`.
///
/// When an update is needed, shows the macOS admin password sheet (unless
/// `KEEL_SKIP_HAL_INSTALL=1` or `skip_prompt` is true — then returns an error).
pub fn ensure_driver_installed(skip_prompt: bool) -> Result<DriverReport> {
    ensure_driver_installed_inner(skip_prompt, false)
}

/// Same as [`ensure_driver_installed`], but always re-copies even when current.
pub fn reinstall_driver(skip_prompt: bool) -> Result<DriverReport> {
    ensure_driver_installed_inner(skip_prompt, true)
}

fn ensure_driver_installed_inner(skip_prompt: bool, force: bool) -> Result<DriverReport> {
    let report = probe_driver();
    if !force && !report.needs_install() {
        info!(
            "HAL driver OK (installed build {:?}, bundled {})",
            report.installed_build, BUNDLED_DRIVER_BUILD
        );
        return Ok(report);
    }

    if skip_prompt || std::env::var_os("KEEL_SKIP_HAL_INSTALL").is_some() {
        return Err(HalError::NotInstalled);
    }

    let src = resolve_bundled_driver_dir().ok_or_else(|| {
        HalError::Msg(
            "bundled KeelAudioVirtual.driver not found (set KEEL_HAL_BUNDLE or run from repo)"
                .into(),
        )
    })?;

    ensure_driver_binary_built(&src)?;

    let action = if force {
        "reinstall"
    } else {
        match report.status {
            DriverStatus::Missing => "install",
            DriverStatus::Outdated => "update",
            _ => "install",
        }
    };
    info!(
        "HAL driver needs {action} (installed={:?} → bundled={}); prompting for admin…",
        report.installed_build, BUNDLED_DRIVER_BUILD
    );

    privileged_install(&src)?;

    let after = probe_driver();
    if !Path::new(SYSTEM_DRIVER_DIR).exists() {
        return Err(HalError::Msg(
            "driver install finished but bundle still missing".into(),
        ));
    }
    info!(
        "HAL driver {action} complete (build {:?})",
        after.installed_build
    );
    Ok(after)
}

fn ensure_driver_binary_built(driver_dir: &Path) -> Result<()> {
    let bin = driver_dir.join("Contents/MacOS/KeelAudioVirtual");
    if bin.exists() {
        return Ok(());
    }
    let native = resolve_native_source().ok_or_else(|| {
        HalError::Msg("HAL binary missing and native source not found to compile".into())
    })?;
    let mac_dir = driver_dir.join("Contents/MacOS");
    fs::create_dir_all(&mac_dir).map_err(|e| HalError::Msg(e.to_string()))?;
    info!("compiling HAL plug-in → {}", bin.display());
    let status = Command::new("clang")
        .args([
            "-bundle",
            "-o",
            bin.to_str().unwrap_or(""),
            native.to_str().unwrap_or(""),
            "-framework",
            "CoreAudio",
            "-framework",
            "CoreFoundation",
            "-fPIC",
        ])
        .status()
        .map_err(|e| HalError::Msg(format!("clang: {e}")))?;
    if !status.success() {
        return Err(HalError::Msg(
            "clang failed to build KeelAudioVirtual".into(),
        ));
    }
    Ok(())
}

fn privileged_install(src: &Path) -> Result<()> {
    // Escape for embedding inside AppleScript double-quoted string.
    let src_s = src
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let dst = SYSTEM_DRIVER_DIR;
    // Single shell pipeline run as root after the password dialog.
    let shell = format!(
        "rm -rf '{dst}' && mkdir -p '/Library/Audio/Plug-Ins/HAL' && \
         cp -R '{src_s}' '{dst}' && chown -R root:wheel '{dst}' && \
         chmod -R 755 '{dst}' && killall coreaudiod || true"
    );
    // AppleScript: do shell script "…" with administrator privileges
    let escaped_shell = shell.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("do shell script \"{escaped_shell}\" with administrator privileges");

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| HalError::Msg(format!("osascript: {e}")))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let err = err.trim();
        if err.contains("User canceled") || err.contains("-128") {
            return Err(HalError::Msg(
                "HAL install canceled (admin prompt dismissed)".into(),
            ));
        }
        return Err(HalError::Msg(format!(
            "HAL install failed: {}",
            if err.is_empty() { "unknown error" } else { err }
        )));
    }
    Ok(())
}

/// Locate the driver bundle to copy from (dev tree or next to the binary).
pub fn resolve_bundled_driver_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("KEEL_HAL_BUNDLE") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }

    // Compile-time workspace path (dev / cargo run).
    let from_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packaging/macos/KeelAudioVirtual.driver");
    if let Ok(canon) = from_manifest.canonicalize() {
        if canon.exists() {
            return Some(canon);
        }
    }

    // Walk up from the running executable looking for packaging/…
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        for _ in 0..8 {
            let Some(d) = dir else { break };
            let candidate = d.join("packaging/macos/KeelAudioVirtual.driver");
            if candidate.exists() {
                return Some(candidate);
            }
            let beside = d.join("KeelAudioVirtual.driver");
            if beside.exists() {
                return Some(beside);
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }

    let cwd = PathBuf::from("packaging/macos/KeelAudioVirtual.driver");
    if cwd.exists() {
        return Some(cwd);
    }
    None
}

fn resolve_native_source() -> Option<PathBuf> {
    let from_manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("native/keel_hal_plugin.c");
    if from_manifest.exists() {
        return Some(from_manifest);
    }
    None
}

fn read_plist_versions(path: &Path) -> Option<(u32, String)> {
    let raw = fs::read_to_string(path).ok()?;
    let build = plist_string_value(&raw, "CFBundleVersion")?
        .parse::<u32>()
        .ok()?;
    let ver =
        plist_string_value(&raw, "CFBundleShortVersionString").unwrap_or_else(|| build.to_string());
    Some((build, ver))
}

fn plist_string_value(xml: &str, key: &str) -> Option<String> {
    let needle = format!("<key>{key}</key>");
    let idx = xml.find(&needle)?;
    let after = &xml[idx + needle.len()..];
    let start = after.find("<string>")? + "<string>".len();
    let end = after[start..].find("</string>")? + start;
    Some(after[start..end].trim().to_string())
}

/// Refresh [`crate::HalClient`] installed flag after an install attempt.
pub fn is_system_driver_present() -> bool {
    driver_bundle_path().map(|p| p.exists()).unwrap_or(false)
}

pub fn install_or_warn() {
    match ensure_driver_installed(false) {
        Ok(r) => {
            if r.needs_install() {
                warn!("HAL still needs attention: {:?}", r.status);
            }
        }
        Err(HalError::NotInstalled) => {
            warn!("{}", crate::install_instructions());
        }
        Err(e) => warn!("HAL auto-install: {e}"),
    }
}
