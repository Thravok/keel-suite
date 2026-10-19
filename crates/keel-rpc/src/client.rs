//! Blocking one-shot JSON-RPC client over localhost WebSocket.

use crate::{bearer_header_value, JsonRpcRequest, JsonRpcResponse, ORIGIN_KEEL};
use serde_json::Value;
use std::net::TcpStream;
use std::time::Duration;
use thiserror::Error;
use tungstenite::client::IntoClientRequest;
use tungstenite::connect;
use tungstenite::http::HeaderValue;
use tungstenite::protocol::Message;
use tungstenite::stream::MaybeTlsStream;

#[derive(Debug, Error)]
pub enum RpcClientError {
    #[error("connect: {0}")]
    Connect(String),
    #[error("handshake: {0}")]
    Handshake(String),
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("rpc error {code}: {message}")]
    Rpc { code: i32, message: String },
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Blocking client for a single JSON-RPC call to a suite engine on loopback.
pub struct RpcClient;

impl RpcClient {
    /// Connect to `127.0.0.1:{port}`, authenticate, call `method`, return `result`.
    pub fn call(
        port: u16,
        token: &str,
        method: &str,
        params: &Value,
    ) -> Result<Value, RpcClientError> {
        let uri = format!("ws://127.0.0.1:{port}/");
        let mut request = uri
            .into_client_request()
            .map_err(|e| RpcClientError::Handshake(e.to_string()))?;
        request
            .headers_mut()
            .insert("Origin", HeaderValue::from_static(ORIGIN_KEEL));
        let auth = bearer_header_value(token);
        request.headers_mut().insert(
            "Authorization",
            HeaderValue::from_str(&auth)
                .map_err(|e| RpcClientError::Handshake(format!("auth header: {e}")))?,
        );

        let (mut ws, _) = connect(request).map_err(|e| RpcClientError::Connect(e.to_string()))?;
        if let MaybeTlsStream::Plain(stream) = ws.get_mut() {
            set_timeouts(stream);
        }

        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(Value::from(1)),
            method: method.to_string(),
            params: params.clone(),
        };
        let text = serde_json::to_string(&req)?;
        ws.send(Message::Text(text.into()))
            .map_err(|e| RpcClientError::Protocol(e.to_string()))?;

        loop {
            let msg = ws
                .read()
                .map_err(|e| RpcClientError::Protocol(e.to_string()))?;
            match msg {
                Message::Text(t) => {
                    let resp: JsonRpcResponse = serde_json::from_str(&t)?;
                    if let Some(err) = resp.error {
                        return Err(RpcClientError::Rpc {
                            code: err.code,
                            message: err.message,
                        });
                    }
                    return Ok(resp.result.unwrap_or(Value::Null));
                }
                Message::Ping(p) => {
                    ws.send(Message::Pong(p))
                        .map_err(|e| RpcClientError::Protocol(e.to_string()))?;
                }
                Message::Close(_) => {
                    return Err(RpcClientError::Protocol("connection closed".into()));
                }
                _ => continue,
            }
        }
    }
}

fn set_timeouts(stream: &TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
}
