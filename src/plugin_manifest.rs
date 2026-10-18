//! Stream Deck SDK plugin manifest (plaintext JSON only).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("{0}")]
    Msg(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginActionManifest {
    #[serde(rename = "UUID")]
    pub uuid: String,
    #[serde(rename = "Name")]
    pub name: Option<String>,
    #[serde(rename = "Tooltip")]
    pub tooltip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    #[serde(rename = "UUID")]
    pub uuid: Option<String>,
    #[serde(rename = "SDKVersion")]
    pub sdk_version: Option<u32>,
    #[serde(rename = "Name")]
    pub name: Option<String>,
    #[serde(rename = "Author")]
    pub author: Option<String>,
    #[serde(rename = "Version")]
    pub version: Option<String>,
    #[serde(rename = "CodePath")]
    pub code_path: Option<String>,
    #[serde(rename = "CodePathMac")]
    pub code_path_mac: Option<String>,
    #[serde(rename = "CodePathWin")]
    pub code_path_win: Option<String>,
    #[serde(rename = "PropertyInspectorPath")]
    pub property_inspector_path: Option<String>,
    #[serde(rename = "PrivateAPI")]
    pub private_api: Option<bool>,
    #[serde(rename = "Actions", default)]
    pub actions: Vec<PluginActionManifest>,
}

impl PluginManifest {
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        let bytes = fs::read(path)?;
        if bytes.starts_with(b"ELGATO") {
            return Err(ManifestError::Msg(
                "manifest.json is ELGATO-sealed (installed Elgato format). Import the original plaintext .streamDeckPlugin / .sdPlugin package instead.".into(),
            ));
        }
        let text = String::from_utf8(bytes).map_err(|e| ManifestError::Msg(e.to_string()))?;
        let m: PluginManifest = serde_json::from_str(&text)?;
        if m.private_api == Some(true) {
            return Err(ManifestError::Msg(
                "PrivateAPI plugins are Stream Deck host builtins and cannot be imported".into(),
            ));
        }
        Ok(m)
    }

    pub fn code_path_for_host(&self) -> Option<&str> {
        #[cfg(target_os = "macos")]
        {
            self.code_path_mac
                .as_deref()
                .or(self.code_path.as_deref())
        }
        #[cfg(target_os = "windows")]
        {
            self.code_path_win
                .as_deref()
                .or(self.code_path.as_deref())
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            self.code_path.as_deref()
        }
    }

    pub fn is_html_code_path(&self) -> bool {
        self.code_path_for_host()
            .map(|p| p.ends_with(".html") || p.ends_with(".htm"))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn rejects_elgato_sealed_manifest() {
        let dir = std::env::temp_dir().join("keel-manifest-elgato-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("manifest.json");
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(b"ELGATO\x01\x00sealed").unwrap();
        let err = PluginManifest::load(&path).unwrap_err().to_string();
        assert!(err.contains("ELGATO-sealed"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn loads_plaintext_manifest() {
        let dir = std::env::temp_dir().join("keel-manifest-plain-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("manifest.json");
        fs::write(
            &path,
            r#"{"UUID":"com.test","SDKVersion":2,"Name":"T","CodePath":"bin","Actions":[{"UUID":"com.test.a","Name":"A"}]}"#,
        )
        .unwrap();
        let m = PluginManifest::load(&path).unwrap();
        assert_eq!(m.uuid.as_deref(), Some("com.test"));
        assert_eq!(m.actions.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
