//! Shared JSON-RPC protocol for keel-audio / keel-light / keel-deck.

mod auth;
mod client;

pub use auth::{
    authorize_ws_upgrade, bearer_header_value, generate_rpc_token, origin_allowed, read_rpc_token,
    write_rpc_token, TokenError, BEARER_PREFIX, RPC_TOKEN_FILENAME,
};
pub use client::{RpcClient, RpcClientError};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Default listen port for keel-audio. Prefer 1885 so we can coexist with Wave Link on 1884.
pub const DEFAULT_PORT: u16 = 1885;
/// Wave Link's observed port — used when keel-audio claims compatibility mode.
pub const WAVELINK_COMPAT_PORT: u16 = 1884;
/// keel-light JSON-RPC (Control Center alternative).
pub const LIGHT_PORT: u16 = 1886;
pub const ORIGIN_KEEL: &str = "keel://";
pub const ORIGIN_STREAMDECK: &str = "streamdeck://";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn result(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// Confirmed Wave Link methods + keel extensions from FEATURE_LEDGER.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcMethod {
    GetOutputDevices,
    SetOutputDevice,
    GetMixerState,
    SetChannelVolume,
    SetChannelMute,
    SetMixVolume,
    SetMixMute,
    SetRouting,
    ListEffects,
    AddEffect,
    RemoveEffect,
    SetEffectBypass,
    GetDevices,
    SetDeviceGain,
    SetDeviceMute,
    SetClipguard,
    SetMicPcMix,
    SetHeadphoneVolume,
    SetGainLock,
    SubscribeMeters,
    UnsubscribeMeters,
    GetHalStatus,
    InstallHalDriver,
    // keel-light
    GetLights,
    RefreshLights,
    SetLightPower,
    SetLightBrightness,
    SetLightTemperature,
    Unknown,
}

impl RpcMethod {
    pub fn parse(name: &str) -> Self {
        match name {
            "getOutputDevices" => Self::GetOutputDevices,
            "setOutputDevice" => Self::SetOutputDevice,
            "getMixerState" => Self::GetMixerState,
            "setChannelVolume" => Self::SetChannelVolume,
            "setChannelMute" => Self::SetChannelMute,
            "setMixVolume" => Self::SetMixVolume,
            "setMixMute" => Self::SetMixMute,
            "setRouting" => Self::SetRouting,
            "listEffects" => Self::ListEffects,
            "addEffect" => Self::AddEffect,
            "removeEffect" => Self::RemoveEffect,
            "setEffectBypass" => Self::SetEffectBypass,
            "getDevices" => Self::GetDevices,
            "setDeviceGain" => Self::SetDeviceGain,
            "setDeviceMute" => Self::SetDeviceMute,
            "setClipguard" => Self::SetClipguard,
            "setMicPcMix" => Self::SetMicPcMix,
            "setHeadphoneVolume" => Self::SetHeadphoneVolume,
            "setGainLock" => Self::SetGainLock,
            "subscribeMeters" => Self::SubscribeMeters,
            "unsubscribeMeters" => Self::UnsubscribeMeters,
            "getHalStatus" => Self::GetHalStatus,
            "installHalDriver" => Self::InstallHalDriver,
            "getLights" => Self::GetLights,
            "refreshLights" => Self::RefreshLights,
            "setLightPower" => Self::SetLightPower,
            "setLightBrightness" => Self::SetLightBrightness,
            "setLightTemperature" => Self::SetLightTemperature,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputDeviceInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub outputs: Vec<OutputEndpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputEndpoint {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainOutput {
    #[serde(rename = "outputDeviceId")]
    pub output_device_id: String,
    #[serde(rename = "outputId")]
    pub output_id: String,
}

/// Deck action payloads for mute/volume/mix control from keel-deck.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeckAudioAction {
    ToggleChannelMute {
        channel_id: String,
    },
    SetChannelVolume {
        channel_id: String,
        volume: f32,
    },
    ToggleMixMute {
        mix_id: String,
    },
    SetMixVolume {
        mix_id: String,
        volume: f32,
    },
    SetMainOutput {
        device_id: String,
        output_id: String,
    },
}

impl DeckAudioAction {
    /// Map to a keel-audio JSON-RPC method name and params object.
    pub fn to_rpc(&self) -> (&'static str, Value) {
        match self {
            Self::ToggleChannelMute { channel_id } => (
                "setChannelMute",
                json!({ "channelId": channel_id, "toggle": true }),
            ),
            Self::SetChannelVolume { channel_id, volume } => (
                "setChannelVolume",
                json!({ "channelId": channel_id, "volume": volume }),
            ),
            Self::ToggleMixMute { mix_id } => {
                ("setMixMute", json!({ "mixId": mix_id, "toggle": true }))
            }
            Self::SetMixVolume { mix_id, volume } => {
                ("setMixVolume", json!({ "mixId": mix_id, "volume": volume }))
            }
            Self::SetMainOutput {
                device_id,
                output_id,
            } => (
                "setOutputDevice",
                json!({ "outputDeviceId": device_id, "outputId": output_id }),
            ),
        }
    }
}

/// Deck action payloads for Key Light control from keel-deck.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DeckLightAction {
    SetPower { id: String, on: bool },
    TogglePower { id: String },
    SetBrightness { id: String, brightness: u8 },
    SetTemperature { id: String, temperature: u16 },
}

impl DeckLightAction {
    /// Map to a keel-light JSON-RPC method name and params object.
    pub fn to_rpc(&self) -> (&'static str, Value) {
        match self {
            Self::SetPower { id, on } => ("setLightPower", json!({ "id": id, "on": on })),
            Self::TogglePower { id } => ("setLightPower", json!({ "id": id, "toggle": true })),
            Self::SetBrightness { id, brightness } => (
                "setLightBrightness",
                json!({ "id": id, "brightness": brightness }),
            ),
            Self::SetTemperature { id, temperature } => (
                "setLightTemperature",
                json!({ "id": id, "temperature": temperature }),
            ),
        }
    }
}
