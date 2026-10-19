//! Profile, page, key, dial, and action data model.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Distinct keypad / pedal geometries Keel supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutKind {
    Mini,
    Original,
    Xl,
    Plus,
    PlusXl,
    Neo,
    Pedal,
}

impl LayoutKind {
    pub const ALL: [LayoutKind; 7] = [
        LayoutKind::Mini,
        LayoutKind::Original,
        LayoutKind::Xl,
        LayoutKind::Plus,
        LayoutKind::PlusXl,
        LayoutKind::Neo,
        LayoutKind::Pedal,
    ];

    pub fn rows(self) -> u8 {
        match self {
            Self::Mini => 2,
            Self::Original => 3,
            Self::Xl | Self::PlusXl => 4,
            Self::Plus | Self::Neo => 2,
            Self::Pedal => 1,
        }
    }

    pub fn cols(self) -> u8 {
        match self {
            Self::Mini => 3,
            Self::Original => 5,
            Self::Xl => 8,
            Self::PlusXl => 9,
            Self::Plus | Self::Neo => 4,
            Self::Pedal => 3,
        }
    }

    pub fn key_count(self) -> usize {
        (self.rows() as usize) * (self.cols() as usize)
    }

    pub fn encoder_count(self) -> usize {
        match self {
            Self::Plus => 4,
            Self::PlusXl => 6,
            _ => 0,
        }
    }

    pub fn touch_point_count(self) -> usize {
        match self {
            Self::Neo => 2,
            _ => 0,
        }
    }

    pub fn has_lcd(self) -> bool {
        matches!(self, Self::Plus | Self::PlusXl | Self::Neo)
    }

    pub fn is_visual(self) -> bool {
        !matches!(self, Self::Pedal)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mini => "Mini (3×2)",
            Self::Original => "Original / Mk2 (5×3)",
            Self::Xl => "XL (8×4)",
            Self::Plus => "Plus (4×2 + 4 dials)",
            Self::PlusXl => "Plus XL (9×4 + 6 dials)",
            Self::Neo => "Neo (4×2 + touch)",
            Self::Pedal => "Pedal (3)",
        }
    }
}

/// Built-in + Stream Deck SDK plugin actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionSpec {
    Empty,
    Hotkey {
        cmd: bool,
        ctrl: bool,
        alt: bool,
        shift: bool,
        key: String,
    },
    OpenUrl {
        url: String,
    },
    OpenApp {
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
    GoPage {
        page_id: String,
    },
    GoBack,
    PluginAction {
        plugin_id: String,
        action_uuid: String,
        #[serde(default)]
        settings: serde_json::Value,
    },
    /// Control keel-audio over local JSON-RPC (mute/volume/mix/main out).
    AudioRpc {
        #[serde(default = "default_audio_port")]
        port: u16,
        method: String,
        #[serde(default)]
        params: serde_json::Value,
    },
    /// Control keel-light over local JSON-RPC (power/brightness/temperature).
    LightRpc {
        #[serde(default = "default_light_port")]
        port: u16,
        method: String,
        #[serde(default)]
        params: serde_json::Value,
    },
}

fn default_audio_port() -> u16 {
    keel_rpc::DEFAULT_PORT
}

fn default_light_port() -> u16 {
    keel_rpc::LIGHT_PORT
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
    pub color: String,
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

/// Stream Deck Plus / Plus XL dial (encoder) binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderBinding {
    pub id: String,
    pub title: String,
    pub press: ActionSpec,
    pub twist_cw: ActionSpec,
    pub twist_ccw: ActionSpec,
}

impl EncoderBinding {
    pub fn default_at(index: usize) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title: format!("Dial {}", index + 1),
            press: ActionSpec::Media {
                command: MediaCommand::PlayPause,
            },
            twist_cw: ActionSpec::Media {
                command: MediaCommand::VolumeUp,
            },
            twist_ccw: ActionSpec::Media {
                command: MediaCommand::VolumeDown,
            },
        }
    }
}

/// Stream Deck Neo touch-point binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TouchPointBinding {
    pub id: String,
    pub title: String,
    pub action: ActionSpec,
}

impl TouchPointBinding {
    pub fn default_at(index: usize) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title: format!("Touch {}", index + 1),
            action: ActionSpec::Empty,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    pub name: String,
    pub keys: Vec<KeyButton>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub layout: LayoutKind,
    pub pages: Vec<Page>,
    pub current_page_id: String,
    #[serde(default)]
    pub page_stack: Vec<String>,
    #[serde(default)]
    pub encoders: Vec<EncoderBinding>,
    #[serde(default)]
    pub touch_points: Vec<TouchPointBinding>,
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
    pub layout: LayoutKind,
    pub rows: u8,
    pub cols: u8,
    pub key_count: u8,
    pub encoder_count: u8,
    pub touch_point_count: u8,
    pub has_lcd: bool,
    pub is_visual: bool,
}
