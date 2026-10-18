//! Profile, page, key, and action data model.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Supported virtual / hardware layouts for v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutKind {
    Mini,
    Original,
    Xl,
}

impl LayoutKind {
    pub fn rows(self) -> u8 {
        match self {
            Self::Mini => 2,
            Self::Original => 3,
            Self::Xl => 4,
        }
    }

    pub fn cols(self) -> u8 {
        match self {
            Self::Mini => 3,
            Self::Original => 5,
            Self::Xl => 8,
        }
    }

    pub fn key_count(self) -> usize {
        (self.rows() as usize) * (self.cols() as usize)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mini => "Mini (3×2)",
            Self::Original => "Original / Mk2 (5×3)",
            Self::Xl => "XL (8×4)",
        }
    }
}

/// Built-in + Stream Deck SDK plugin actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionSpec {
    Empty,
    Hotkey {
        /// Modifier flags: cmd/meta, ctrl, alt/option, shift
        cmd: bool,
        ctrl: bool,
        alt: bool,
        shift: bool,
        /// Key name understood by enigo, e.g. "c", "Space", "F5", "Return"
        key: String,
    },
    OpenUrl {
        url: String,
    },
    OpenApp {
        /// Absolute path to app/binary, or macOS .app bundle path
        path: String,
    },
    RunCommand {
        program: String,
        args: Vec<String>,
    },
    Media {
        command: MediaCommand,
    },
    Text {
        text: String,
    },
    Brightness {
        percent: u8,
    },
    /// Navigate to another page in the same profile
    GoPage {
        page_id: String,
    },
    /// Go to the previous page in the stack (folder back)
    GoBack,
    /// Fire an action UUID in an imported Stream Deck SDK plugin
    PluginAction {
        /// Folder name e.g. `com.example.foo.sdPlugin`
        plugin_id: String,
        action_uuid: String,
        #[serde(default)]
        settings: serde_json::Value,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaCommand {
    PlayPause,
    Next,
    Previous,
    VolumeUp,
    VolumeDown,
    Mute,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyButton {
    pub id: String,
    pub title: String,
    /// CSS / hex color for the virtual key face
    pub color: String,
    /// Optional data-URL or absolute path to an image (PNG/JPEG)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    pub action: ActionSpec,
}

impl KeyButton {
    pub fn empty() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title: String::new(),
            color: "#2a2e35".into(),
            image: None,
            action: ActionSpec::Empty,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    pub name: String,
    /// Flat row-major key list; length must match layout key_count
    pub keys: Vec<KeyButton>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub layout: LayoutKind,
    pub pages: Vec<Page>,
    pub current_page_id: String,
    /// Navigation stack for folder/back
    #[serde(default)]
    pub page_stack: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub active_profile_id: Option<String>,
    pub brightness: u8,
    pub start_hidden: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            active_profile_id: None,
            brightness: 70,
            start_hidden: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub kind: String,
    pub serial: String,
    pub rows: u8,
    pub cols: u8,
    pub key_count: u8,
    pub encoder_count: u8,
}
