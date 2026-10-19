//! Orchestrates engine, HAL, Wave USB, and persistence.

use crate::audio_runtime::AudioRuntime;
use keel_audio_engine::AudioEngine;
use keel_audio_fx::{list_au_components, EffectKind};
use keel_audio_hal::{ensure_driver_installed, install_instructions, reinstall_driver, HalClient};
use keel_audio_wave::WaveManager;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{info, warn};

pub struct AudioService {
    pub engine: Arc<AudioEngine>,
    pub wave: Arc<WaveManager>,
    pub hal: Arc<Mutex<HalClient>>,
}

/// Bundle returned from startup — keep `runtime` on the main thread (cpal streams are !Send).
pub struct AudioServiceStart {
    pub service: Arc<AudioService>,
    pub runtime: Option<AudioRuntime>,
}

impl AudioService {
    pub fn start() -> Result<AudioServiceStart, String> {
        Self::start_with_options(false)
    }

    pub fn start_with_options(skip_hal_install: bool) -> Result<AudioServiceStart, String> {
        let engine = AudioEngine::open_default().map_err(|e| e.to_string())?;
        let wave = WaveManager::new();
        if let Err(e) = wave.refresh() {
            warn!("wave refresh: {e}");
        }
        // Ensure discovered Wave mics appear as mixer channels (is_wave=true).
        {
            let state = engine.state();
            for w in wave.list() {
                let already = state
                    .channels
                    .iter()
                    .any(|c| c.name.eq_ignore_ascii_case(&w.info.name));
                if !already {
                    match engine.add_channel(w.info.name.clone(), true) {
                        Ok(_) => info!("added Wave channel: {}", w.info.name),
                        Err(e) => warn!("could not add Wave channel: {e}"),
                    }
                }
            }
            let _ = engine.save();
        }

        let mut hal = HalClient::connect().map_err(|e| e.to_string())?;
        if hal.needs_update() {
            match ensure_driver_installed(skip_hal_install) {
                Ok(report) => {
                    info!(
                        "HAL driver status={:?} installed_build={:?}",
                        report.status, report.installed_build
                    );
                    hal.refresh_report();
                }
                Err(e) => {
                    warn!("HAL auto-install: {e}");
                    warn!("{}", install_instructions());
                }
            }
        } else {
            info!(
                "HAL driver current (build {:?})",
                hal.report.installed_build
            );
        }

        // Seed default channels if empty
        if engine.state().channels.is_empty() {
            let _ = engine.add_channel("Microphone", false);
            let _ = engine.add_channel("System", false);
            let _ = engine.add_channel("Music", false);
            let _ = engine.save();
        }

        let preferred_input = wave
            .list()
            .into_iter()
            .find(|d| {
                d.info.audio_device_name.is_some() || d.info.name.to_lowercase().contains("wave")
            })
            .map(|d| d.info.audio_device_name.unwrap_or(d.info.name));

        let hal = Arc::new(Mutex::new(hal));
        let runtime = match AudioRuntime::start(
            Arc::clone(&engine),
            Arc::clone(&hal),
            preferred_input.clone(),
        ) {
            Ok(rt) => {
                info!(
                    "capture runtime started (preferred_input={:?})",
                    preferred_input
                );
                Some(rt)
            }
            Err(e) => {
                warn!("capture runtime not started: {e}");
                None
            }
        };

        Ok(AudioServiceStart {
            service: Arc::new(Self { engine, wave, hal }),
            runtime,
        })
    }

    pub fn handle_rpc(&self, method: &str, params: &Value) -> Result<Value, (i32, String)> {
        use keel_rpc::RpcMethod;
        match RpcMethod::parse(method) {
            RpcMethod::GetOutputDevices => Ok(self.get_output_devices()),
            RpcMethod::SetOutputDevice => {
                let main = params
                    .get("mainOutput")
                    .ok_or((-32602, "missing mainOutput".into()))?;
                let device_id = main
                    .get("outputDeviceId")
                    .and_then(|v| v.as_str())
                    .ok_or((-32602, "missing outputDeviceId".into()))?
                    .to_string();
                let output_id = main
                    .get("outputId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("generic")
                    .to_string();
                self.engine.set_main_output(device_id, output_id);
                let _ = self.engine.save();
                Ok(
                    json!({ "mainOutput": self.engine.state().main_output_device_id.as_ref().map(|id| {
                        json!({
                            "outputDeviceId": id,
                            "outputId": self.engine.state().main_output_id,
                        })
                    })}),
                )
            }
            RpcMethod::GetMixerState => {
                Ok(serde_json::to_value(self.engine.state()).unwrap_or(json!({})))
            }
            RpcMethod::SetChannelVolume => {
                let id = parse_uuid(params, "channelId")?;
                let vol = params.get("volume").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                self.engine.set_channel_volume(id, vol);
                Ok(json!({ "ok": true }))
            }
            RpcMethod::SetChannelMute => {
                let id = parse_uuid(params, "channelId")?;
                let mute = params
                    .get("mute")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.engine.set_channel_mute(id, mute);
                Ok(json!({ "ok": true }))
            }
            RpcMethod::SetMixVolume => {
                let id = parse_uuid(params, "mixId")?;
                let vol = params.get("volume").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                self.engine.set_mix_volume(id, vol);
                Ok(json!({ "ok": true }))
            }
            RpcMethod::SetMixMute => {
                let id = parse_uuid(params, "mixId")?;
                let mute = params
                    .get("mute")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.engine.set_mix_mute(id, mute);
                Ok(json!({ "ok": true }))
            }
            RpcMethod::SetRouting => {
                let mix = parse_uuid(params, "mixId")?;
                let ch = parse_uuid(params, "channelId")?;
                let level = params
                    .get("level")
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32);
                self.engine.set_routing(mix, ch, level);
                Ok(json!({ "ok": true }))
            }
            RpcMethod::ListEffects => Ok(json!({
                "builtin": EffectKind::all_builtin().iter().map(|k| json!({
                    "id": format!("{k:?}").to_lowercase(),
                    "title": k.title(),
                })).collect::<Vec<_>>(),
                "au": list_au_components(&self.engine.state().au_plugin_paths),
            })),
            RpcMethod::AddEffect => {
                let ch = parse_uuid(params, "channelId")?;
                let kind_s = params
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("equalizer");
                let kind = parse_kind(kind_s);
                let id = self
                    .engine
                    .add_effect(ch, kind)
                    .ok_or((-32000, "channel not found".into()))?;
                Ok(json!({ "effectId": id }))
            }
            RpcMethod::RemoveEffect => {
                let ch = parse_uuid(params, "channelId")?;
                let fx = parse_uuid(params, "effectId")?;
                self.engine.remove_effect(ch, fx);
                Ok(json!({ "ok": true }))
            }
            RpcMethod::SetEffectBypass => {
                let ch = parse_uuid(params, "channelId")?;
                let fx = parse_uuid(params, "effectId")?;
                let bypass = params
                    .get("bypass")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.engine.set_effect_bypass(ch, fx, bypass);
                Ok(json!({ "ok": true }))
            }
            RpcMethod::GetDevices => {
                let _ = self.wave.refresh();
                Ok(json!({ "devices": self.wave.list() }))
            }
            RpcMethod::SetDeviceGain => {
                let id = params
                    .get("deviceId")
                    .and_then(|v| v.as_str())
                    .ok_or((-32602, "deviceId".into()))?;
                let gain = params
                    .get("gainDb")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(20.0) as f32;
                self.wave
                    .set_gain(id, gain)
                    .map_err(|e| (-32000, e.to_string()))?;
                Ok(json!({ "ok": true }))
            }
            RpcMethod::SetDeviceMute => {
                let id = params
                    .get("deviceId")
                    .and_then(|v| v.as_str())
                    .ok_or((-32602, "deviceId".into()))?;
                let mute = params
                    .get("mute")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.wave
                    .set_mute(id, mute)
                    .map_err(|e| (-32000, e.to_string()))?;
                Ok(json!({ "ok": true }))
            }
            RpcMethod::SetClipguard => {
                let id = params
                    .get("deviceId")
                    .and_then(|v| v.as_str())
                    .ok_or((-32602, "deviceId".into()))?;
                let on = params
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                self.wave
                    .set_clipguard(id, on)
                    .map_err(|e| (-32000, e.to_string()))?;
                Ok(json!({ "ok": true }))
            }
            RpcMethod::SetMicPcMix => {
                let id = params
                    .get("deviceId")
                    .and_then(|v| v.as_str())
                    .ok_or((-32602, "deviceId".into()))?;
                let mix = params.get("mix").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                self.wave
                    .set_mic_pc_mix(id, mix)
                    .map_err(|e| (-32000, e.to_string()))?;
                Ok(json!({ "ok": true }))
            }
            RpcMethod::SetHeadphoneVolume => {
                let id = params
                    .get("deviceId")
                    .and_then(|v| v.as_str())
                    .ok_or((-32602, "deviceId".into()))?;
                let vol = params.get("volume").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                self.wave
                    .set_headphone_volume(id, vol)
                    .map_err(|e| (-32000, e.to_string()))?;
                Ok(json!({ "ok": true }))
            }
            RpcMethod::SetGainLock => {
                let id = params
                    .get("deviceId")
                    .and_then(|v| v.as_str())
                    .ok_or((-32602, "deviceId".into()))?;
                let lock = params
                    .get("lock")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.wave
                    .set_gain_lock(id, lock)
                    .map_err(|e| (-32000, e.to_string()))?;
                Ok(json!({ "ok": true }))
            }
            RpcMethod::SubscribeMeters | RpcMethod::UnsubscribeMeters => Ok(json!({
                "meters": self.engine.meters(),
            })),
            RpcMethod::GetHalStatus => {
                let mut hal = self.hal.lock();
                hal.refresh_report();
                Ok(serde_json::to_value(&hal.report).unwrap_or(json!({})))
            }
            RpcMethod::InstallHalDriver => {
                let force = params
                    .get("force")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let mut hal = self.hal.lock();
                if !force && !hal.needs_update() {
                    return Ok(json!({
                        "ok": true,
                        "skipped": true,
                        "report": hal.report.clone(),
                    }));
                }
                let result = if force {
                    reinstall_driver(false)
                } else {
                    ensure_driver_installed(false)
                };
                match result {
                    Ok(report) => {
                        hal.refresh_report();
                        Ok(json!({ "ok": true, "report": report }))
                    }
                    Err(e) => Err((-32000, e.to_string())),
                }
            }
            RpcMethod::Unknown
            | RpcMethod::GetLights
            | RpcMethod::RefreshLights
            | RpcMethod::SetLightPower
            | RpcMethod::SetLightBrightness
            | RpcMethod::SetLightTemperature => {
                Err((-32601, format!("method not found: {method}")))
            }
        }
    }

    fn get_output_devices(&self) -> Value {
        let state = self.engine.state();
        let mut devices = vec![json!({
            "id": "system_default",
            "name": "System Default",
            "outputs": [{"id": "generic", "name": "Default"}],
        })];
        let hal = self.hal.lock();
        for d in &hal.devices {
            if d.is_output {
                devices.push(json!({
                    "id": format!("hal-{}", d.id),
                    "name": d.name,
                    "outputs": [{"id": "generic"}],
                }));
            }
        }
        // Also expose wave headphone targets as logical devices when present
        for w in self.wave.list() {
            devices.push(json!({
                "id": w.info.id,
                "name": w.info.name,
                "outputs": [{"id": "headphones"}, {"id": "generic"}],
            }));
        }
        let main = json!({
            "outputDeviceId": state.main_output_device_id.clone().unwrap_or_else(|| "system_default".into()),
            "outputId": state.main_output_id.clone().unwrap_or_else(|| "generic".into()),
        });
        json!({
            "mainOutput": main,
            "outputDevices": devices,
            "halInstalled": hal.is_installed(),
            "halStatus": hal.report.status,
            "halInstalledBuild": hal.report.installed_build,
            "halBundledBuild": hal.report.bundled_build,
            "halNeedsUpdate": hal.needs_update(),
            "issues": self.engine.routing_issues(),
        })
    }
}

fn parse_uuid(params: &Value, key: &str) -> Result<uuid::Uuid, (i32, String)> {
    let s = params
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or((-32602, format!("missing {key}")))?;
    uuid::Uuid::parse_str(s).map_err(|_| (-32602, format!("bad uuid {key}")))
}

fn parse_kind(s: &str) -> EffectKind {
    match s.to_lowercase().as_str() {
        "compressor" => EffectKind::Compressor,
        "expander" => EffectKind::Expander,
        "maximizer" => EffectKind::Maximizer,
        "lowcut" => EffectKind::Lowcut,
        "lowcut_enhanced" | "lowcutenhanced" => EffectKind::LowcutEnhanced,
        "clipguard" => EffectKind::Clipguard,
        "voice_tune" | "voicetune" => EffectKind::VoiceTune,
        "channel_boost" | "channelboost" => EffectKind::ChannelBoost,
        "ducking" => EffectKind::Ducking,
        "voice_focus" | "voicefocus" | "voice_focus_substitute" => EffectKind::VoiceFocusSubstitute,
        "audiounit" | "au" => EffectKind::AudioUnit,
        _ => EffectKind::Equalizer,
    }
}
