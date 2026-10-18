//! Layout helpers and default demo profile.

use crate::model::{ActionSpec, KeyButton, LayoutKind, MediaCommand, Page, Profile};
use uuid::Uuid;

pub fn ensure_page_size(page: &mut Page, layout: LayoutKind) {
    let n = layout.key_count();
    while page.keys.len() < n {
        page.keys.push(KeyButton::empty());
    }
    if page.keys.len() > n {
        page.keys.truncate(n);
    }
}

pub fn index_to_row_col(index: usize, layout: LayoutKind) -> (u8, u8) {
    let cols = layout.cols() as usize;
    let row = (index / cols) as u8;
    let col = (index % cols) as u8;
    (row, col)
}

pub fn row_col_to_index(row: u8, col: u8, layout: LayoutKind) -> usize {
    (row as usize) * (layout.cols() as usize) + (col as usize)
}

pub fn default_profile() -> Profile {
    let layout = LayoutKind::Original;
    let page_id = Uuid::new_v4().to_string();
    let mut keys: Vec<KeyButton> = (0..layout.key_count())
        .map(|_| KeyButton::empty())
        .collect();

    keys[0] = KeyButton {
        id: Uuid::new_v4().to_string(),
        title: "Play".into(),
        color: "#3d4a3a".into(),
        image: None,
        action: ActionSpec::Media {
            command: MediaCommand::PlayPause,
        },
    };
    keys[1] = KeyButton {
        id: Uuid::new_v4().to_string(),
        title: "Mute".into(),
        color: "#4a3a3a".into(),
        image: None,
        action: ActionSpec::Media {
            command: MediaCommand::Mute,
        },
    };
    keys[2] = KeyButton {
        id: Uuid::new_v4().to_string(),
        title: "Vol+".into(),
        color: "#c47a2c".into(),
        image: None,
        action: ActionSpec::Media {
            command: MediaCommand::VolumeUp,
        },
    };
    // Hotkey example: Cmd+C — user can edit; on non-mac Cmd maps to Meta
    keys[3] = KeyButton {
        id: Uuid::new_v4().to_string(),
        title: "Copy".into(),
        color: "#3a4450".into(),
        image: None,
        action: ActionSpec::Hotkey {
            cmd: true,
            ctrl: false,
            alt: false,
            shift: false,
            key: "c".into(),
        },
    };
    keys[4] = KeyButton {
        id: Uuid::new_v4().to_string(),
        title: "Type".into(),
        color: "#3a4050".into(),
        image: None,
        action: ActionSpec::Text {
            text: "Hello from Keel".into(),
        },
    };

    Profile {
        id: Uuid::new_v4().to_string(),
        name: "Default".into(),
        layout,
        current_page_id: page_id.clone(),
        page_stack: vec![],
        pages: vec![Page {
            id: page_id,
            name: "Home".into(),
            keys,
        }],
    }
}
