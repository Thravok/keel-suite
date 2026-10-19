//! Layout helpers, Kind→LayoutKind mapping, and default demo profile.

use crate::model::{
    ActionSpec, EncoderBinding, KeyButton, LayoutKind, MediaCommand, Page, Profile,
    TouchPointBinding,
};
use elgato_streamdeck::info::Kind;
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

/// Resize encoder / touch-point vectors to match layout.
pub fn ensure_profile_extras(profile: &mut Profile) {
    let enc_n = profile.layout.encoder_count();
    while profile.encoders.len() < enc_n {
        let i = profile.encoders.len();
        profile.encoders.push(EncoderBinding::default_at(i));
    }
    if profile.encoders.len() > enc_n {
        profile.encoders.truncate(enc_n);
    }

    let touch_n = profile.layout.touch_point_count();
    while profile.touch_points.len() < touch_n {
        let i = profile.touch_points.len();
        profile.touch_points.push(TouchPointBinding::default_at(i));
    }
    if profile.touch_points.len() > touch_n {
        profile.touch_points.truncate(touch_n);
    }

    for page in &mut profile.pages {
        ensure_page_size(page, profile.layout);
    }
}

/// Map an Elgato HID `Kind` to Keel's layout bucket.
pub fn kind_to_layout(kind: Kind) -> LayoutKind {
    match kind {
        Kind::Mini | Kind::MiniMk2 | Kind::MiniDiscord | Kind::MiniMk2Module => LayoutKind::Mini,
        Kind::Original | Kind::OriginalV2 | Kind::Mk2 | Kind::Mk2Scissor | Kind::Mk2Module => {
            LayoutKind::Original
        }
        Kind::Xl | Kind::XlV2 | Kind::XlV2Module => LayoutKind::Xl,
        Kind::Plus => LayoutKind::Plus,
        Kind::PlusXl => LayoutKind::PlusXl,
        Kind::Neo => LayoutKind::Neo,
        Kind::Pedal => LayoutKind::Pedal,
    }
}

pub fn kind_display_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Original => "Stream Deck",
        Kind::OriginalV2 => "Stream Deck (rev2)",
        Kind::Mk2 => "Stream Deck Mk.2",
        Kind::Mk2Scissor => "Stream Deck Mk.2 Scissor",
        Kind::Mk2Module => "Stream Deck Mk.2 Module",
        Kind::Mini => "Stream Deck Mini",
        Kind::MiniMk2 => "Stream Deck Mini Mk.2",
        Kind::MiniDiscord => "Stream Deck Mini Discord",
        Kind::MiniMk2Module => "Stream Deck Mini Mk.2 Module",
        Kind::Xl => "Stream Deck XL",
        Kind::XlV2 => "Stream Deck XL (rev2)",
        Kind::XlV2Module => "Stream Deck XL Module",
        Kind::Plus => "Stream Deck Plus",
        Kind::PlusXl => "Stream Deck Plus XL",
        Kind::Neo => "Stream Deck Neo",
        Kind::Pedal => "Stream Deck Pedal",
    }
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

    let mut profile = Profile {
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
        encoders: vec![],
        touch_points: vec![],
    };
    ensure_profile_extras(&mut profile);
    profile
}
