//! Offline Application Support paths for Keel apps (no cloud).

use directories::ProjectDirs;
use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("could not resolve app data directory")]
    NoProjectDirs,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Resolve `~/Library/Application Support/app.Keel.<App>/...` (macOS) or platform equivalent.
pub fn app_data_dir(app: &str) -> Result<PathBuf> {
    let dirs = ProjectDirs::from("app", "Keel", app).ok_or(StoreError::NoProjectDirs)?;
    let path = dirs.data_dir().to_path_buf();
    fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn load_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&raw)?))
}

pub fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string_pretty(value)?;
    fs::write(path, raw)?;
    Ok(())
}

pub fn keel_audio_dir() -> Result<PathBuf> {
    app_data_dir("KeelAudio")
}

pub fn keel_deck_dir() -> Result<PathBuf> {
    app_data_dir("Keel")
}

pub fn keel_light_dir() -> Result<PathBuf> {
    app_data_dir("KeelLight")
}
