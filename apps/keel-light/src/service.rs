//! Orchestrates USB light discovery + control.

use keel_light_usb::LightUsbManager;
use keel_rpc::RpcMethod;
use keel_store::{keel_light_dir, load_json, save_json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LightPrefs {
    /// Remember last brightness/temperature per device id for UI restore hints.
    #[serde(default)]
    last: std::collections::HashMap<String, LastLight>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LastLight {
    brightness: u8,
    temperature: u16,
}

pub struct LightService {
    pub lights: Arc<LightUsbManager>,
    prefs_path: PathBuf,
}

impl LightService {
    pub fn start() -> Result<Arc<Self>, String> {
        let dir = keel_light_dir().map_err(|e| e.to_string())?;
        let prefs_path = dir.join("prefs.json");
        let lights = LightUsbManager::new();
        if let Err(e) = lights.refresh() {
            warn!("light refresh: {e}");
        }
        info!("keel-light: {} USB light(s)", lights.list().len());
        Ok(Arc::new(Self { lights, prefs_path }))
    }

    pub fn handle_rpc(&self, method: &str, params: &Value) -> Result<Value, (i32, String)> {
        match RpcMethod::parse(method) {
            RpcMethod::GetLights | RpcMethod::RefreshLights => {
                if matches!(RpcMethod::parse(method), RpcMethod::RefreshLights) {
                    self.lights.refresh().map_err(|e| (-32000, e.to_string()))?;
                }
                Ok(json!({ "lights": self.lights.list() }))
            }
            RpcMethod::SetLightPower => {
                let id = require_str(params, "id")?;
                let on = params.get("on").and_then(|v| v.as_bool()).unwrap_or(true);
                let light = self
                    .lights
                    .set_power(&id, on)
                    .map_err(|e| (-32000, e.to_string()))?;
                self.remember(&light);
                Ok(json!({ "ok": true, "light": light }))
            }
            RpcMethod::SetLightBrightness => {
                let id = require_str(params, "id")?;
                let brightness = params
                    .get("brightness")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(50) as u8;
                let light = self
                    .lights
                    .set_brightness(&id, brightness)
                    .map_err(|e| (-32000, e.to_string()))?;
                self.remember(&light);
                Ok(json!({ "ok": true, "light": light }))
            }
            RpcMethod::SetLightTemperature => {
                let id = require_str(params, "id")?;
                let temperature = params
                    .get("temperature")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(200) as u16;
                let light = self
                    .lights
                    .set_temperature(&id, temperature)
                    .map_err(|e| (-32000, e.to_string()))?;
                self.remember(&light);
                Ok(json!({ "ok": true, "light": light }))
            }
            _ => Err((-32601, format!("method not found: {method}"))),
        }
    }

    fn remember(&self, light: &keel_light_usb::LightState) {
        let mut prefs: LightPrefs = load_json(&self.prefs_path)
            .ok()
            .flatten()
            .unwrap_or_default();
        prefs.last.insert(
            light.info.id.clone(),
            LastLight {
                brightness: light.brightness,
                temperature: light.temperature,
            },
        );
        let _ = save_json(&self.prefs_path, &prefs);
    }
}

fn require_str(params: &Value, key: &str) -> Result<String, (i32, String)> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or((-32602, format!("missing {key}")))
}
