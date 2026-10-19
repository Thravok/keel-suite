//! WebSocket JSON-RPC server (Wave Link–compatible methods on keel port).

use crate::service::AudioService;
use keel_rpc::{authorize_ws_upgrade, JsonRpcRequest, JsonRpcResponse, DEFAULT_PORT};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;
use tracing::{info, warn};
use tungstenite::{accept_hdr, Message};

pub fn spawn(service: Arc<AudioService>, port: u16, token: String) -> std::io::Result<()> {
    let bind = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&bind)?;
    info!("keel-audio JSON-RPC listening on ws://{bind}");
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let svc = Arc::clone(&service);
            let token = token.clone();
            thread::spawn(move || {
                if let Err(e) = handle_client(stream, svc, &token) {
                    warn!("rpc client: {e}");
                }
            });
        }
    });
    Ok(())
}

pub fn spawn_default(service: Arc<AudioService>, token: String) -> std::io::Result<()> {
    spawn(service, DEFAULT_PORT, token)
}

fn handle_client(
    stream: std::net::TcpStream,
    service: Arc<AudioService>,
    token: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let expected = token.to_string();
    #[allow(clippy::result_large_err)] // tungstenite handshake ErrorResponse
    let callback = move |req: &tungstenite::handshake::server::Request, response| {
        authorize_ws_upgrade(req, response, &expected)
    };
    let mut ws = accept_hdr(stream, callback)?;
    loop {
        let msg = match ws.read() {
            Ok(m) => m,
            Err(_) => break,
        };
        if msg.is_close() {
            break;
        }
        let Message::Text(text) = msg else {
            continue;
        };
        let req: JsonRpcRequest = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::error(None, -32700, format!("parse error: {e}"));
                ws.send(Message::Text(serde_json::to_string(&resp)?.into()))?;
                continue;
            }
        };
        let resp = match service.handle_rpc(&req.method, &req.params) {
            Ok(result) => JsonRpcResponse::result(req.id.clone(), result),
            Err((code, message)) => JsonRpcResponse::error(req.id.clone(), code, message),
        };
        ws.send(Message::Text(serde_json::to_string(&resp)?.into()))?;
    }
    Ok(())
}
