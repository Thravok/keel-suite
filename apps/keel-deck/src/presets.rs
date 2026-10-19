//! One-click key presets (media / deck) — title + color + action + icon.

use crate::model::{ActionSpec, KeyButton, MediaCommand};
use egui_phosphor::regular;

/// Built-in key template applied in one click from the key editor.
#[derive(Clone, Copy)]
pub struct KeyPreset {
    pub id: &'static str,
    pub title: &'static str,
    pub color: &'static str,
    pub icon: &'static str,
    pub action: ActionSpecStatic,
}

/// `ActionSpec` without owned Strings — for static preset tables.
#[derive(Clone, Copy)]
pub enum ActionSpecStatic {
    Media(MediaCommand),
    Brightness(u8),
    GoBack,
}

impl ActionSpecStatic {
    pub fn to_action(self) -> ActionSpec {
        match self {
            Self::Media(command) => ActionSpec::Media { command },
            Self::Brightness(percent) => ActionSpec::Brightness { percent },
            Self::GoBack => ActionSpec::GoBack,
        }
    }
}

impl KeyPreset {
    pub fn apply_to(self, button: &mut KeyButton) {
        button.title = self.title.to_string();
        button.color = self.color.to_string();
        button.action = self.action.to_action();
    }
}

/// All quick-assign presets shown in the key editor.
pub const PRESETS: &[KeyPreset] = &[
    KeyPreset {
        id: "play_pause",
        title: "Play",
        color: "#2d4a3e",
        icon: regular::PLAY_PAUSE,
        action: ActionSpecStatic::Media(MediaCommand::PlayPause),
    },
    KeyPreset {
        id: "previous",
        title: "Prev",
        color: "#2d3a4a",
        icon: regular::SKIP_BACK,
        action: ActionSpecStatic::Media(MediaCommand::Previous),
    },
    KeyPreset {
        id: "next",
        title: "Next",
        color: "#2d3a4a",
        icon: regular::SKIP_FORWARD,
        action: ActionSpecStatic::Media(MediaCommand::Next),
    },
    KeyPreset {
        id: "volume_down",
        title: "Vol−",
        color: "#3a3548",
        icon: regular::SPEAKER_LOW,
        action: ActionSpecStatic::Media(MediaCommand::VolumeDown),
    },
    KeyPreset {
        id: "volume_up",
        title: "Vol+",
        color: "#3a3548",
        icon: regular::SPEAKER_HIGH,
        action: ActionSpecStatic::Media(MediaCommand::VolumeUp),
    },
    KeyPreset {
        id: "mute",
        title: "Mute",
        color: "#4a3038",
        icon: regular::SPEAKER_SLASH,
        action: ActionSpecStatic::Media(MediaCommand::Mute),
    },
    KeyPreset {
        id: "go_back",
        title: "Back",
        color: "#5a4030",
        icon: regular::ARROW_LEFT,
        action: ActionSpecStatic::GoBack,
    },
    KeyPreset {
        id: "brightness_50",
        title: "50%",
        color: "#4a4530",
        icon: regular::SUN_DIM,
        action: ActionSpecStatic::Brightness(50),
    },
    KeyPreset {
        id: "brightness_100",
        title: "100%",
        color: "#4a4530",
        icon: regular::SUN,
        action: ActionSpecStatic::Brightness(100),
    },
];

/// Phosphor glyph for a key action (virtual deck / editor).
pub fn icon_for_action(action: &ActionSpec) -> Option<&'static str> {
    match action {
        ActionSpec::Media { command } => Some(match command {
            MediaCommand::PlayPause => regular::PLAY_PAUSE,
            MediaCommand::Next => regular::SKIP_FORWARD,
            MediaCommand::Previous => regular::SKIP_BACK,
            MediaCommand::VolumeUp => regular::SPEAKER_HIGH,
            MediaCommand::VolumeDown => regular::SPEAKER_LOW,
            MediaCommand::Mute => regular::SPEAKER_SLASH,
        }),
        ActionSpec::GoBack => Some(regular::ARROW_LEFT),
        ActionSpec::Brightness { percent } => {
            if *percent >= 75 {
                Some(regular::SUN)
            } else {
                Some(regular::SUN_DIM)
            }
        }
        ActionSpec::GoPage { .. } => Some(regular::FOLDER),
        ActionSpec::Hotkey { .. } => Some(regular::KEYBOARD),
        ActionSpec::OpenUrl { .. } => Some(regular::LINK),
        ActionSpec::OpenApp { .. } => Some(regular::APP_WINDOW),
        ActionSpec::RunCommand { .. } => Some(regular::TERMINAL),
        ActionSpec::Text { .. } => Some(regular::TEXT_T),
        ActionSpec::PluginAction { .. } => Some(regular::PUZZLE_PIECE),
        ActionSpec::AudioRpc { .. } => Some(regular::SPEAKER_HIGH),
        ActionSpec::LightRpc { .. } => Some(regular::LIGHTBULB),
        ActionSpec::Empty => None,
    }
}
