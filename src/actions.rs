//! Built-in action runner (offline; no network except user-chosen open URL/app).

use crate::model::{ActionSpec, MediaCommand};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ActionError {
    #[error("{0}")]
    Msg(String),
}

pub struct ActionRunner;

impl ActionRunner {
    pub fn run(action: &ActionSpec) -> Result<ActionOutcome, ActionError> {
        match action {
            ActionSpec::Empty => Ok(ActionOutcome::None),
            ActionSpec::Hotkey {
                cmd,
                ctrl,
                alt,
                shift,
                key,
            } => {
                press_hotkey(*cmd, *ctrl, *alt, *shift, key)?;
                Ok(ActionOutcome::None)
            }
            ActionSpec::OpenUrl { url } => {
                open::that(url).map_err(|e| ActionError::Msg(e.to_string()))?;
                Ok(ActionOutcome::None)
            }
            ActionSpec::OpenApp { path } => {
                open::that(path).map_err(|e| ActionError::Msg(e.to_string()))?;
                Ok(ActionOutcome::None)
            }
            ActionSpec::RunCommand { program, args } => {
                if program.trim().is_empty() {
                    return Err(ActionError::Msg("command program is empty".into()));
                }
                // Explicit local spawn only — no shell wrapper.
                Command::new(program)
                    .args(args)
                    .spawn()
                    .map_err(|e| ActionError::Msg(format!("spawn {program}: {e}")))?;
                Ok(ActionOutcome::None)
            }
            ActionSpec::Media { command } => {
                media_key(*command)?;
                Ok(ActionOutcome::None)
            }
            ActionSpec::Text { text } => {
                type_text(text)?;
                Ok(ActionOutcome::None)
            }
            ActionSpec::Brightness { percent } => Ok(ActionOutcome::Brightness((*percent).min(100))),
            ActionSpec::GoPage { page_id } => Ok(ActionOutcome::GoPage(page_id.clone())),
            ActionSpec::GoBack => Ok(ActionOutcome::GoBack),
            ActionSpec::PluginAction {
                plugin_id,
                action_uuid,
                settings,
            } => Ok(ActionOutcome::Plugin {
                plugin_id: plugin_id.clone(),
                action_uuid: action_uuid.clone(),
                settings: settings.clone(),
            }),
        }
    }
}

#[derive(Debug)]
pub enum ActionOutcome {
    None,
    Brightness(u8),
    GoPage(String),
    GoBack,
    Plugin {
        plugin_id: String,
        action_uuid: String,
        settings: serde_json::Value,
    },
}

fn with_enigo<F>(f: F) -> Result<(), ActionError>
where
    F: FnOnce(&mut Enigo) -> Result<(), ActionError>,
{
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| ActionError::Msg(format!("enigo init: {e}")))?;
    f(&mut enigo)
}

fn parse_key(name: &str) -> Result<Key, ActionError> {
    let n = name.trim();
    if n.len() == 1 {
        let c = n.chars().next().unwrap();
        return Ok(Key::Unicode(c.to_ascii_lowercase()));
    }
    Ok(match n.to_ascii_lowercase().as_str() {
        "space" => Key::Space,
        "return" | "enter" => Key::Return,
        "tab" => Key::Tab,
        "escape" | "esc" => Key::Escape,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        other => {
            return Err(ActionError::Msg(format!("unknown key '{other}'")));
        }
    })
}

fn press_hotkey(
    cmd: bool,
    ctrl: bool,
    alt: bool,
    shift: bool,
    key: &str,
) -> Result<(), ActionError> {
    let key = parse_key(key)?;
    with_enigo(|enigo| {
        if cmd {
            enigo
                .key(Key::Meta, Direction::Press)
                .map_err(|e| ActionError::Msg(e.to_string()))?;
        }
        if ctrl {
            enigo
                .key(Key::Control, Direction::Press)
                .map_err(|e| ActionError::Msg(e.to_string()))?;
        }
        if alt {
            enigo
                .key(Key::Alt, Direction::Press)
                .map_err(|e| ActionError::Msg(e.to_string()))?;
        }
        if shift {
            enigo
                .key(Key::Shift, Direction::Press)
                .map_err(|e| ActionError::Msg(e.to_string()))?;
        }
        enigo
            .key(key, Direction::Click)
            .map_err(|e| ActionError::Msg(e.to_string()))?;
        if shift {
            let _ = enigo.key(Key::Shift, Direction::Release);
        }
        if alt {
            let _ = enigo.key(Key::Alt, Direction::Release);
        }
        if ctrl {
            let _ = enigo.key(Key::Control, Direction::Release);
        }
        if cmd {
            let _ = enigo.key(Key::Meta, Direction::Release);
        }
        Ok(())
    })
}

fn type_text(text: &str) -> Result<(), ActionError> {
    with_enigo(|enigo| {
        enigo
            .text(text)
            .map_err(|e| ActionError::Msg(e.to_string()))
    })
}

fn media_key(cmd: MediaCommand) -> Result<(), ActionError> {
    let key = match cmd {
        MediaCommand::PlayPause => Key::MediaPlayPause,
        MediaCommand::Next => Key::MediaNextTrack,
        MediaCommand::Previous => Key::MediaPrevTrack,
        MediaCommand::VolumeUp => Key::VolumeUp,
        MediaCommand::VolumeDown => Key::VolumeDown,
        MediaCommand::Mute => Key::VolumeMute,
    };
    with_enigo(|enigo| {
        enigo
            .key(key, Direction::Click)
            .map_err(|e| ActionError::Msg(e.to_string()))
    })
}
