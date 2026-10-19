//! USB HID control for Elgato Key Light Neo (and Mini when HID is present).
//!
//! Wire format (verified on Neo PID `0x00A0`): 512-byte reports, ID `0x02`,
//! framing `idx/total/0x03/len_le/body/0x03`, body = `GET|PUT /elgato/...`.

use hidapi::{HidApi, HidDevice};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, info, warn};

/// Corsair/Elgato USB vendor id.
pub const ELGATO_VID: u16 = 0x0FD9;
/// Key Light Neo (USB HID control).
pub const KEY_LIGHT_NEO_PID: u16 = 0x00A0;

const REPORT_ID: u8 = 0x02;
const FRAME_MAGIC: u8 = 0x03;
const FRAME_SIZE: usize = 512;
const MAX_BODY_PER_FRAME: usize = 505;
const IO_TIMEOUT_MS: i32 = 500;

#[derive(Debug, Error)]
pub enum LightUsbError {
    #[error("hid: {0}")]
    Hid(String),
    #[error("device not found")]
    NotFound,
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, LightUsbError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightFamily {
    KeyLightNeo,
    KeyLightMini,
    UnknownUsb,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightInfo {
    pub id: String,
    pub name: String,
    pub family: LightFamily,
    pub serial: Option<String>,
    pub path: String,
    pub product_id: u16,
    #[serde(default)]
    pub firmware: Option<String>,
    #[serde(default)]
    pub max_brightness: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightState {
    pub info: LightInfo,
    pub on: bool,
    /// 0..=100
    pub brightness: u8,
    /// Mireds (143 cool … 344 warm).
    pub temperature: u16,
}

#[derive(Debug, Deserialize)]
struct LightsResponse {
    #[serde(rename = "numberOfLights")]
    #[allow(dead_code)]
    number_of_lights: Option<u32>,
    lights: Vec<LightValuesJson>,
}

#[derive(Debug, Deserialize, Serialize)]
struct LightValuesJson {
    on: Option<u8>,
    brightness: Option<u8>,
    temperature: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct AccessoryInfoJson {
    #[serde(rename = "productName")]
    product_name: Option<String>,
    #[serde(rename = "serialNumber")]
    serial_number: Option<String>,
    #[serde(rename = "firmwareVersion")]
    firmware_version: Option<String>,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "power-info")]
    power_info: Option<PowerInfoJson>,
}

#[derive(Debug, Deserialize)]
struct PowerInfoJson {
    #[serde(rename = "maximumBrightness")]
    maximum_brightness: Option<u8>,
}

/// Manager for attached USB Key Lights.
pub struct LightUsbManager {
    inner: Mutex<Vec<ManagedLight>>,
}

struct ManagedLight {
    state: LightState,
}

impl LightUsbManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Vec::new()),
        })
    }

    pub fn refresh(&self) -> Result<()> {
        let api = HidApi::new().map_err(|e| LightUsbError::Hid(e.to_string()))?;
        let mut next = Vec::new();
        for dev in api.device_list() {
            if dev.vendor_id() != ELGATO_VID {
                continue;
            }
            if !is_usb_key_light(dev.product_id(), dev.product_string()) {
                continue;
            }
            let path = dev.path().to_string_lossy().into_owned();
            let serial = dev.serial_number().map(|s| s.to_string());
            let product = dev.product_string().unwrap_or("Key Light").to_string();
            let family = classify(dev.product_id(), &product);
            let id = format!(
                "usb:{:04x}:{}",
                dev.product_id(),
                serial.as_deref().unwrap_or("unknown")
            );
            let mut info = LightInfo {
                id: id.clone(),
                name: product.clone(),
                family,
                serial: serial.clone(),
                path: path.clone(),
                product_id: dev.product_id(),
                firmware: None,
                max_brightness: None,
            };

            match open_and_query(&api, &path, &mut info) {
                Ok(values) => {
                    next.push(ManagedLight {
                        state: LightState {
                            info,
                            on: values.on.unwrap_or(0) != 0,
                            brightness: values.brightness.unwrap_or(20).min(100),
                            temperature: values
                                .temperature
                                .unwrap_or(200)
                                .clamp(TEMP_MIN, TEMP_MAX),
                        },
                    });
                    info!("USB light ready: {id}");
                }
                Err(e) => {
                    warn!("USB light {id} open/query failed: {e}");
                    next.push(ManagedLight {
                        state: LightState {
                            info,
                            on: false,
                            brightness: 20,
                            temperature: 200,
                        },
                    });
                }
            }
        }
        if next.is_empty() {
            info!("no USB Key Lights present");
        }
        *self.inner.lock() = next;
        Ok(())
    }

    pub fn list(&self) -> Vec<LightState> {
        self.inner.lock().iter().map(|m| m.state.clone()).collect()
    }

    pub fn set_power(&self, id: &str, on: bool) -> Result<LightState> {
        self.patch(id, Some(on), None, None)
    }

    pub fn set_brightness(&self, id: &str, brightness: u8) -> Result<LightState> {
        self.patch(id, None, Some(brightness.min(100)), None)
    }

    pub fn set_temperature(&self, id: &str, temperature: u16) -> Result<LightState> {
        self.patch(id, None, None, Some(temperature.clamp(TEMP_MIN, TEMP_MAX)))
    }

    fn patch(
        &self,
        id: &str,
        on: Option<bool>,
        brightness: Option<u8>,
        temperature: Option<u16>,
    ) -> Result<LightState> {
        let mut list = self.inner.lock();
        let light = list
            .iter_mut()
            .find(|m| m.state.info.id == id)
            .ok_or(LightUsbError::NotFound)?;

        if let Some(v) = on {
            light.state.on = v;
        }
        if let Some(v) = brightness {
            let max = light.state.info.max_brightness.unwrap_or(100);
            light.state.brightness = v.min(max);
        }
        if let Some(v) = temperature {
            light.state.temperature = v;
        }

        let body = serde_json::json!({
            "lights": [{
                "on": if light.state.on { 1 } else { 0 },
                "brightness": light.state.brightness,
                "temperature": light.state.temperature,
            }]
        });
        let cmd = format!("PUT /elgato/lights {body}");
        let api = HidApi::new().map_err(|e| LightUsbError::Hid(e.to_string()))?;
        let path = std::ffi::CString::new(light.state.info.path.as_str())
            .map_err(|e| LightUsbError::Msg(e.to_string()))?;
        let device = api
            .open_path(&path)
            .map_err(|e| LightUsbError::Hid(e.to_string()))?;
        let resp = hid_request(&device, cmd.as_bytes())?;
        if let Ok(parsed) = serde_json::from_slice::<LightsResponse>(&resp) {
            if let Some(v) = parsed.lights.first() {
                if let Some(on) = v.on {
                    light.state.on = on != 0;
                }
                if let Some(b) = v.brightness {
                    light.state.brightness = b.min(100);
                }
                if let Some(t) = v.temperature {
                    light.state.temperature = t.clamp(TEMP_MIN, TEMP_MAX);
                }
            }
        }
        Ok(light.state.clone())
    }
}

pub const TEMP_MIN: u16 = 143;
pub const TEMP_MAX: u16 = 344;

fn is_usb_key_light(pid: u16, product: Option<&str>) -> bool {
    if pid == KEY_LIGHT_NEO_PID {
        return true;
    }
    let p = product.unwrap_or("").to_lowercase();
    // Avoid Stream Deck / Wave / cameras.
    if p.contains("stream deck") || p.contains("wave") || p.contains("cam") {
        return false;
    }
    p.contains("key light")
}

fn classify(pid: u16, product: &str) -> LightFamily {
    let p = product.to_lowercase();
    if pid == KEY_LIGHT_NEO_PID || p.contains("neo") {
        LightFamily::KeyLightNeo
    } else if p.contains("mini") {
        LightFamily::KeyLightMini
    } else {
        LightFamily::UnknownUsb
    }
}

fn open_and_query(api: &HidApi, path: &str, info: &mut LightInfo) -> Result<LightValuesJson> {
    let c_path = std::ffi::CString::new(path).map_err(|e| LightUsbError::Msg(e.to_string()))?;
    let device = api
        .open_path(&c_path)
        .map_err(|e| LightUsbError::Hid(e.to_string()))?;

    if let Ok(raw) = hid_request(&device, b"GET /elgato/accessory-info") {
        if let Ok(acc) = serde_json::from_slice::<AccessoryInfoJson>(&raw) {
            if let Some(name) = acc.display_name.filter(|s| !s.is_empty()) {
                info.name = name;
            } else if let Some(name) = acc.product_name {
                info.name = name;
            }
            if info.serial.is_none() {
                info.serial = acc.serial_number;
            }
            info.firmware = acc.firmware_version;
            info.max_brightness = acc.power_info.and_then(|p| p.maximum_brightness);
        }
    }

    let raw = hid_request(&device, b"GET /elgato/lights")?;
    let parsed: LightsResponse = serde_json::from_slice(&raw)
        .map_err(|e| LightUsbError::Protocol(format!("lights json: {e}")))?;
    parsed
        .lights
        .into_iter()
        .next()
        .ok_or_else(|| LightUsbError::Protocol("empty lights array".into()))
}

fn build_frames(payload: &[u8]) -> Vec<Vec<u8>> {
    let chunks: Vec<&[u8]> = if payload.is_empty() {
        vec![&[][..]]
    } else {
        payload.chunks(MAX_BODY_PER_FRAME).collect()
    };
    let total = chunks.len() as u8;
    chunks
        .into_iter()
        .enumerate()
        .map(|(idx, chunk)| {
            let mut frame = Vec::with_capacity(FRAME_SIZE);
            frame.push(REPORT_ID);
            frame.push(idx as u8);
            frame.push(total);
            frame.push(FRAME_MAGIC);
            frame.extend_from_slice(&(chunk.len() as u16).to_le_bytes());
            frame.extend_from_slice(chunk);
            frame.push(FRAME_MAGIC);
            frame.resize(FRAME_SIZE, 0);
            frame
        })
        .collect()
}

fn hid_request(device: &HidDevice, payload: &[u8]) -> Result<Vec<u8>> {
    for frame in build_frames(payload) {
        device
            .write(&frame)
            .map_err(|e| LightUsbError::Hid(format!("write: {e}")))?;
    }

    let mut chunks: Vec<(u8, Vec<u8>)> = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);

    while std::time::Instant::now() < deadline {
        let mut buf = [0u8; FRAME_SIZE];
        let n = device
            .read_timeout(&mut buf, IO_TIMEOUT_MS)
            .map_err(|e| LightUsbError::Hid(format!("read: {e}")))?;
        if n == 0 {
            continue;
        }
        // Some stacks omit report ID on read; accept either.
        let (idx, total, body) = if buf[0] == REPORT_ID && n >= 6 {
            let len = u16::from_le_bytes([buf[4], buf[5]]) as usize;
            let start = 6;
            let end = (start + len).min(n);
            (buf[1], buf[2], buf[start..end].to_vec())
        } else if n >= 5 {
            let len = u16::from_le_bytes([buf[3], buf[4]]) as usize;
            let start = 5;
            let end = (start + len).min(n);
            (buf[0], buf[1], buf[start..end].to_vec())
        } else {
            continue;
        };
        if !chunks.iter().any(|(i, _)| *i == idx) {
            chunks.push((idx, body));
        }
        if chunks.len() as u8 >= total.max(1) {
            break;
        }
    }

    if chunks.is_empty() {
        return Err(LightUsbError::Protocol("no USB response".into()));
    }
    chunks.sort_by_key(|(i, _)| *i);
    let out: Vec<u8> = chunks.into_iter().flat_map(|(_, b)| b).collect();
    debug!("usb resp {} bytes", out.len());
    Ok(out)
}

/// Kelvin helpers (UI convenience).
pub fn mired_to_kelvin(mired: u16) -> u32 {
    if mired == 0 {
        return 0;
    }
    1_000_000 / u32::from(mired)
}

pub fn kelvin_to_mired(kelvin: u32) -> u16 {
    let k = kelvin.clamp(2900, 7000);
    (1_000_000 / k) as u16
}
