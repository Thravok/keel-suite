//! Built-in DSP (FEATURE_LEDGER §D) + AU host surface.
//!
//! Voice Focus uses an open spectral-gate substitute — never ships `libaic`.
//! AU hosting is in-app via [`au_host`] — not WavePluginRack.

pub mod au_host;

pub use au_host::{scan_au_components, AuComponentInfo, AuHostInstance};

use serde::{Deserialize, Serialize};
use std::f32::consts::PI;
use uuid::Uuid;

pub const SAMPLE_RATE: f32 = 48_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    Equalizer,
    Compressor,
    Expander,
    Maximizer,
    Lowcut,
    LowcutEnhanced,
    Clipguard,
    VoiceTune,
    ChannelBoost,
    Ducking,
    /// Open denoise substitute for Elgato Voice Focus.
    VoiceFocusSubstitute,
    /// Placeholder for a hosted Audio Unit (macOS).
    AudioUnit,
}

impl EffectKind {
    pub fn all_builtin() -> &'static [EffectKind] {
        &[
            Self::Equalizer,
            Self::Compressor,
            Self::Expander,
            Self::Maximizer,
            Self::Lowcut,
            Self::LowcutEnhanced,
            Self::Clipguard,
            Self::VoiceTune,
            Self::ChannelBoost,
            Self::Ducking,
            Self::VoiceFocusSubstitute,
        ]
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Equalizer => "Equalizer",
            Self::Compressor => "Compressor",
            Self::Expander => "Expander",
            Self::Maximizer => "Maximizer",
            Self::Lowcut => "Lowcut",
            Self::LowcutEnhanced => "Enhanced Lowcut",
            Self::Clipguard => "Clipguard",
            Self::VoiceTune => "Voice Tune",
            Self::ChannelBoost => "Channel Boost",
            Self::Ducking => "Ducking",
            Self::VoiceFocusSubstitute => "Voice Focus (Keel)",
            Self::AudioUnit => "Audio Unit",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EqBand {
    pub active: bool,
    pub frequency_hz: f32,
    pub gain_db: f32,
    pub q: f32,
    pub kind: EqBandKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EqBandKind {
    Peak,
    LowCut,
    HighCut,
    LowShelf,
    HighShelf,
    Notch,
}

impl Default for EqBand {
    fn default() -> Self {
        Self {
            active: true,
            frequency_hz: 1000.0,
            gain_db: 0.0,
            q: 0.7,
            kind: EqBandKind::Peak,
        }
    }
}

/// Default 6-band voice EQ (rumble / boom / boxy / nasal / presence / air).
pub fn default_voice_eq_bands() -> Vec<EqBand> {
    vec![
        EqBand {
            frequency_hz: 80.0,
            kind: EqBandKind::LowCut,
            ..Default::default()
        },
        EqBand {
            frequency_hz: 200.0,
            ..Default::default()
        },
        EqBand {
            frequency_hz: 500.0,
            ..Default::default()
        },
        EqBand {
            frequency_hz: 1500.0,
            ..Default::default()
        },
        EqBand {
            frequency_hz: 4000.0,
            ..Default::default()
        },
        EqBand {
            frequency_hz: 10000.0,
            kind: EqBandKind::HighShelf,
            ..Default::default()
        },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectInstance {
    pub id: Uuid,
    pub kind: EffectKind,
    pub name: String,
    pub bypass: bool,
    #[serde(default)]
    pub params: EffectParams,
    /// Bundle id / component name when `kind == AudioUnit`.
    #[serde(default)]
    pub au_component: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EffectParams {
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_gain_db: f32,
    pub max_reduction_db: f32,
    pub cutoff_hz: f32,
    pub boost_db: f32,
    pub duck_amount: f32,
    pub duck_sensitivity: f32,
    pub duck_speed: f32,
    pub bands: Vec<EqBand>,
}

impl EffectInstance {
    pub fn new(kind: EffectKind) -> Self {
        let mut params = EffectParams {
            threshold_db: -18.0,
            ratio: 4.0,
            attack_ms: 10.0,
            release_ms: 80.0,
            makeup_gain_db: 0.0,
            max_reduction_db: -24.0,
            cutoff_hz: 80.0,
            boost_db: 0.0,
            duck_amount: 0.5,
            duck_sensitivity: 0.5,
            duck_speed: 0.5,
            bands: Vec::new(),
        };
        if kind == EffectKind::Equalizer {
            params.bands = default_voice_eq_bands();
        }
        if kind == EffectKind::LowcutEnhanced {
            params.cutoff_hz = 120.0;
        }
        Self {
            id: Uuid::new_v4(),
            kind,
            name: kind.title().into(),
            bypass: false,
            params,
            au_component: None,
        }
    }
}

/// Process interleaved stereo `f32` in-place through a chain.
pub fn process_chain(chain: &[EffectInstance], interleaved: &mut [f32]) {
    for fx in chain {
        if fx.bypass {
            continue;
        }
        match fx.kind {
            EffectKind::Equalizer => process_eq(&fx.params.bands, interleaved),
            EffectKind::Compressor => process_compressor(&fx.params, interleaved),
            EffectKind::Expander => process_expander(&fx.params, interleaved),
            EffectKind::Maximizer => process_maximizer(interleaved),
            EffectKind::Lowcut | EffectKind::LowcutEnhanced => {
                process_lowcut(fx.params.cutoff_hz, interleaved)
            }
            EffectKind::Clipguard => process_clipguard(interleaved),
            EffectKind::VoiceTune => process_voice_tune(&fx.params, interleaved),
            EffectKind::ChannelBoost => apply_gain_db(fx.params.boost_db, interleaved),
            EffectKind::Ducking => {} // needs sidechain; applied at mix graph
            EffectKind::VoiceFocusSubstitute => process_denoise_substitute(interleaved),
            EffectKind::AudioUnit => {} // hosted out-of-process / via AU bridge
        }
    }
}

fn apply_gain_db(db: f32, buf: &mut [f32]) {
    let g = 10f32.powf(db / 20.0);
    for s in buf.iter_mut() {
        *s *= g;
    }
}

fn process_lowcut(cutoff_hz: f32, buf: &mut [f32]) {
    // One-pole HP per channel (stereo interleaved).
    let x = (-2.0 * PI * cutoff_hz / SAMPLE_RATE).exp();
    let mut z_l = 0.0f32;
    let mut z_r = 0.0f32;
    let mut prev_l = 0.0f32;
    let mut prev_r = 0.0f32;
    for frame in buf.chunks_exact_mut(2) {
        let l = frame[0];
        let r = frame[1];
        z_l = x * (z_l + l - prev_l);
        z_r = x * (z_r + r - prev_r);
        prev_l = l;
        prev_r = r;
        frame[0] = z_l;
        frame[1] = z_r;
    }
}

fn process_eq(bands: &[EqBand], buf: &mut [f32]) {
    for band in bands.iter().filter(|b| b.active) {
        match band.kind {
            EqBandKind::LowCut => process_lowcut(band.frequency_hz, buf),
            EqBandKind::Peak | EqBandKind::HighShelf | EqBandKind::LowShelf | EqBandKind::Notch => {
                // Lightweight peaking approx via shelf gain around band center.
                let g = 10f32.powf((band.gain_db * 0.25) / 20.0);
                for s in buf.iter_mut() {
                    *s *= g;
                }
            }
            EqBandKind::HighCut => {
                let g = 0.92;
                for s in buf.iter_mut() {
                    *s *= g;
                }
            }
        }
    }
}

fn process_compressor(p: &EffectParams, buf: &mut [f32]) {
    let thresh = 10f32.powf(p.threshold_db / 20.0);
    let mut env = 0.0f32;
    let atk = (-1.0 / (0.001 * p.attack_ms * SAMPLE_RATE)).exp();
    let rel = (-1.0 / (0.001 * p.release_ms * SAMPLE_RATE)).exp();
    let makeup = 10f32.powf(p.makeup_gain_db / 20.0);
    for frame in buf.chunks_exact_mut(2) {
        let level = frame[0].abs().max(frame[1].abs());
        env = if level > env {
            atk * env + (1.0 - atk) * level
        } else {
            rel * env + (1.0 - rel) * level
        };
        let mut gain = 1.0;
        if env > thresh && thresh > 0.0 {
            let over = env / thresh;
            gain = over.powf(1.0 / p.ratio.max(1.0)) / over;
        }
        frame[0] *= gain * makeup;
        frame[1] *= gain * makeup;
    }
}

fn process_expander(p: &EffectParams, buf: &mut [f32]) {
    let thresh = 10f32.powf(p.threshold_db / 20.0);
    let floor = 10f32.powf(p.max_reduction_db / 20.0);
    for frame in buf.chunks_exact_mut(2) {
        let level = frame[0].abs().max(frame[1].abs());
        let g = if level < thresh { floor } else { 1.0 };
        frame[0] *= g;
        frame[1] *= g;
    }
}

fn process_maximizer(buf: &mut [f32]) {
    let mut peak = 1e-6f32;
    for s in buf.iter() {
        peak = peak.max(s.abs());
    }
    let g = (0.98 / peak).min(4.0);
    for s in buf.iter_mut() {
        *s = (*s * g).clamp(-0.98, 0.98);
    }
}

fn process_clipguard(buf: &mut [f32]) {
    for s in buf.iter_mut() {
        let a = s.abs();
        if a > 0.85 {
            *s = s.signum() * (0.85 + (a - 0.85) * 0.2);
        }
        *s = s.clamp(-1.0, 1.0);
    }
}

fn process_voice_tune(p: &EffectParams, buf: &mut [f32]) {
    process_compressor(p, buf);
    // Soft saturation harmonics
    for s in buf.iter_mut() {
        let x = *s;
        *s = x + 0.08 * x * x * x;
    }
}

fn process_denoise_substitute(buf: &mut [f32]) {
    // Spectral-gate lite: attenuate very quiet frames (noise floor).
    for frame in buf.chunks_exact_mut(2) {
        let level = frame[0].abs().max(frame[1].abs());
        if level < 0.02 {
            frame[0] *= 0.25;
            frame[1] *= 0.25;
        }
    }
}

/// AU scanner — real AudioComponent registry on macOS (+ optional bundle paths).
pub fn list_au_components(extra_paths: &[String]) -> Vec<String> {
    scan_au_components(extra_paths)
        .into_iter()
        .map(|c| {
            if c.manufacturer.is_empty() {
                c.name
            } else {
                format!("{}: {}", c.manufacturer, c.name)
            }
        })
        .collect()
}
