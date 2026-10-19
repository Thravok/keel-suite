//! Real-time mixer graph matching Wave Link 3 FEATURE_LEDGER §C.
//!
//! Sample rate 48 kHz, 24-bit path (processed as f32), stereo.

use keel_audio_fx::{process_chain, EffectInstance, EffectKind};
use keel_store::{keel_audio_dir, load_json, save_json};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

pub const SAMPLE_RATE: u32 = 48_000;
pub const MAX_SOFTWARE_CHANNELS: usize = 8;
pub const MAX_WAVE_INPUTS: usize = 4;
pub const MAX_MIXES: usize = 5;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("store: {0}")]
    Store(#[from] keel_store::StoreError),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, EngineError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    Device,
    AppRunning,
    AppNotRunning,
    Virtual,
    Empty,
    Wave,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSource {
    pub id: Uuid,
    pub name: String,
    pub kind: InputKind,
    #[serde(default)]
    pub device_uid: Option<String>,
    #[serde(default)]
    pub app_bundle_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: Uuid,
    pub name: String,
    pub volume: f32,
    pub mute: bool,
    #[serde(default)]
    pub inputs: Vec<Uuid>,
    #[serde(default)]
    pub effects: Vec<EffectInstance>,
    /// Direct monitoring — bypass software FX for self-monitor path.
    #[serde(default)]
    pub direct_monitoring: bool,
    #[serde(default)]
    pub is_wave: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mix {
    pub id: Uuid,
    pub name: String,
    pub volume: f32,
    pub mute: bool,
    #[serde(default)]
    pub icon: Option<String>,
    /// channel_id → contribution (0..1) when routed.
    #[serde(default)]
    pub routing: HashMap<Uuid, f32>,
    #[serde(default)]
    pub output_device_ids: Vec<String>,
    #[serde(default)]
    pub maximizer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MixerState {
    pub inputs: Vec<InputSource>,
    pub channels: Vec<Channel>,
    pub mixes: Vec<Mix>,
    pub main_output_device_id: Option<String>,
    pub main_output_id: Option<String>,
    pub lock_default_input: bool,
    pub lock_default_output: bool,
    #[serde(default)]
    pub au_plugin_paths: Vec<String>,
    pub theme: String,
    pub start_at_login: bool,
}

impl Default for MixerState {
    fn default() -> Self {
        let personal = Uuid::new_v4();
        let chat = Uuid::new_v4();
        let stream = Uuid::new_v4();
        Self {
            inputs: Vec::new(),
            channels: Vec::new(),
            mixes: vec![
                Mix {
                    id: personal,
                    name: "Personal".into(),
                    volume: 1.0,
                    mute: false,
                    icon: None,
                    routing: HashMap::new(),
                    output_device_ids: Vec::new(),
                    maximizer: false,
                },
                Mix {
                    id: chat,
                    name: "Chat".into(),
                    volume: 1.0,
                    mute: false,
                    icon: None,
                    routing: HashMap::new(),
                    output_device_ids: Vec::new(),
                    maximizer: false,
                },
                Mix {
                    id: stream,
                    name: "Stream".into(),
                    volume: 1.0,
                    mute: false,
                    icon: None,
                    routing: HashMap::new(),
                    output_device_ids: Vec::new(),
                    maximizer: false,
                },
            ],
            main_output_device_id: None,
            main_output_id: Some("generic".into()),
            lock_default_input: false,
            lock_default_output: false,
            au_plugin_paths: Vec::new(),
            theme: "system".into(),
            start_at_login: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MeterLevels {
    pub channels: HashMap<Uuid, f32>,
    pub mixes: HashMap<Uuid, f32>,
}

pub struct AudioEngine {
    inner: RwLock<MixerState>,
    meters: RwLock<MeterLevels>,
    path: PathBuf,
}

impl AudioEngine {
    pub fn open_default() -> Result<Arc<Self>> {
        let dir = keel_audio_dir()?;
        let path = dir.join("mixer.json");
        let state = load_json::<MixerState>(&path)?.unwrap_or_default();
        Ok(Arc::new(Self {
            inner: RwLock::new(state),
            meters: RwLock::new(MeterLevels::default()),
            path,
        }))
    }

    pub fn state(&self) -> MixerState {
        self.inner.read().clone()
    }

    pub fn meters(&self) -> MeterLevels {
        self.meters.read().clone()
    }

    pub fn save(&self) -> Result<()> {
        save_json(&self.path, &*self.inner.read())?;
        Ok(())
    }

    pub fn set_main_output(&self, device_id: String, output_id: String) {
        let mut s = self.inner.write();
        s.main_output_device_id = Some(device_id);
        s.main_output_id = Some(output_id);
    }

    pub fn add_channel(&self, name: impl Into<String>, is_wave: bool) -> Result<Uuid> {
        let mut s = self.inner.write();
        let soft = s.channels.iter().filter(|c| !c.is_wave).count();
        let wave = s.channels.iter().filter(|c| c.is_wave).count();
        if is_wave && wave >= MAX_WAVE_INPUTS {
            return Err(EngineError::Msg("max 4 Wave inputs".into()));
        }
        if !is_wave && soft >= MAX_SOFTWARE_CHANNELS {
            return Err(EngineError::Msg("max 8 software channels".into()));
        }
        let id = Uuid::new_v4();
        s.channels.push(Channel {
            id,
            name: name.into(),
            volume: 1.0,
            mute: false,
            inputs: Vec::new(),
            effects: Vec::new(),
            direct_monitoring: false,
            is_wave,
        });
        Ok(id)
    }

    pub fn add_mix(&self, name: impl Into<String>) -> Result<Uuid> {
        let mut s = self.inner.write();
        if s.mixes.len() >= MAX_MIXES {
            return Err(EngineError::Msg("max 5 mixes".into()));
        }
        let id = Uuid::new_v4();
        s.mixes.push(Mix {
            id,
            name: name.into(),
            volume: 1.0,
            mute: false,
            icon: None,
            routing: HashMap::new(),
            output_device_ids: Vec::new(),
            maximizer: false,
        });
        Ok(id)
    }

    pub fn set_channel_volume(&self, id: Uuid, volume: f32) {
        if let Some(c) = self.inner.write().channels.iter_mut().find(|c| c.id == id) {
            c.volume = volume.clamp(0.0, 1.5);
        }
    }

    pub fn set_channel_mute(&self, id: Uuid, mute: bool) {
        if let Some(c) = self.inner.write().channels.iter_mut().find(|c| c.id == id) {
            c.mute = mute;
        }
    }

    pub fn set_mix_volume(&self, id: Uuid, volume: f32) {
        if let Some(m) = self.inner.write().mixes.iter_mut().find(|m| m.id == id) {
            m.volume = volume.clamp(0.0, 1.5);
        }
    }

    pub fn set_mix_mute(&self, id: Uuid, mute: bool) {
        if let Some(m) = self.inner.write().mixes.iter_mut().find(|m| m.id == id) {
            m.mute = mute;
        }
    }

    pub fn set_routing(&self, mix_id: Uuid, channel_id: Uuid, level: Option<f32>) {
        let mut s = self.inner.write();
        if let Some(m) = s.mixes.iter_mut().find(|m| m.id == mix_id) {
            match level {
                Some(l) => {
                    m.routing.insert(channel_id, l.clamp(0.0, 1.0));
                }
                None => {
                    m.routing.remove(&channel_id);
                }
            }
        }
    }

    pub fn add_effect(&self, channel_id: Uuid, kind: EffectKind) -> Option<Uuid> {
        let mut s = self.inner.write();
        let c = s.channels.iter_mut().find(|c| c.id == channel_id)?;
        let fx = EffectInstance::new(kind);
        let id = fx.id;
        c.effects.push(fx);
        Some(id)
    }

    pub fn remove_effect(&self, channel_id: Uuid, effect_id: Uuid) {
        if let Some(c) = self
            .inner
            .write()
            .channels
            .iter_mut()
            .find(|c| c.id == channel_id)
        {
            c.effects.retain(|e| e.id != effect_id);
        }
    }

    pub fn set_effect_bypass(&self, channel_id: Uuid, effect_id: Uuid, bypass: bool) {
        if let Some(c) = self
            .inner
            .write()
            .channels
            .iter_mut()
            .find(|c| c.id == channel_id)
        {
            if let Some(e) = c.effects.iter_mut().find(|e| e.id == effect_id) {
                e.bypass = bypass;
            }
        }
    }

    /// Process one channel buffer, apply FX (unless direct monitor path), accumulate into mixes.
    pub fn process_block(
        &self,
        channel_id: Uuid,
        input: &[f32],
        mix_bufs: &mut HashMap<Uuid, Vec<f32>>,
        for_direct_monitor: bool,
    ) {
        let state = self.inner.read();
        let Some(ch) = state.channels.iter().find(|c| c.id == channel_id) else {
            return;
        };
        if ch.mute {
            return;
        }
        let mut buf = input.to_vec();
        if !(for_direct_monitor && ch.direct_monitoring) {
            process_chain(&ch.effects, &mut buf);
        }
        let vol = ch.volume;
        for s in buf.iter_mut() {
            *s *= vol;
        }
        let peak = buf.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        self.meters.write().channels.insert(channel_id, peak);

        for mix in &state.mixes {
            if mix.mute {
                continue;
            }
            let Some(&route) = mix.routing.get(&channel_id) else {
                continue;
            };
            let out = mix_bufs
                .entry(mix.id)
                .or_insert_with(|| vec![0.0; buf.len()]);
            if out.len() < buf.len() {
                out.resize(buf.len(), 0.0);
            }
            let g = route * mix.volume;
            for (o, i) in out.iter_mut().zip(buf.iter()) {
                *o += *i * g;
            }
        }
    }

    pub fn finalize_mixes(&self, mix_bufs: &mut HashMap<Uuid, Vec<f32>>) {
        let state = self.inner.read();
        for mix in &state.mixes {
            let Some(buf) = mix_bufs.get_mut(&mix.id) else {
                continue;
            };
            if mix.maximizer {
                let fx = EffectInstance::new(EffectKind::Maximizer);
                process_chain(std::slice::from_ref(&fx), buf);
            }
            let peak = buf.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
            self.meters.write().mixes.insert(mix.id, peak);
        }
    }

    pub fn routing_issues(&self) -> Vec<String> {
        let s = self.inner.read();
        let mut issues = Vec::new();
        for c in &s.channels {
            if c.mute {
                issues.push(format!("channelMuted:{}", c.name));
            }
            if c.volume == 0.0 {
                issues.push(format!("channelVolumeZero:{}", c.name));
            }
        }
        for m in &s.mixes {
            if m.mute {
                issues.push(format!("mixMuted:{}", m.name));
            }
            if m.volume == 0.0 {
                issues.push(format!("mixVolumeZero:{}", m.name));
            }
            if m.output_device_ids.is_empty() && m.name != "Personal" {
                issues.push(format!("mixNoOutput:{}", m.name));
            }
        }
        issues
    }
}
