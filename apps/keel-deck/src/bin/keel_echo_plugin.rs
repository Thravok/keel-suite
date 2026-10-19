//! Minimal Stream Deck SDK plugin for Keel host verification (offline).
//!
//! Argv: -port -pluginUUID -registerEvent -info
//! Connects to ws://127.0.0.1:<port>, sends registerPlugin, logs key events,
//! and replies with setTitle on keyUp.

use serde_json::{json, Value};
use std::env;
use std::process;
use tungstenite::Message;

fn main() {
    let args: Vec<String> = env::args().collect();
    let port = arg_value(&args, "-port").unwrap_or_else(|| fatal("missing -port"));
    let uuid = arg_value(&args, "-pluginUUID").unwrap_or_else(|| fatal("missing -pluginUUID"));
    let register_event =
        arg_value(&args, "-registerEvent").unwrap_or_else(|| "registerPlugin".into());
    let _info = arg_value(&args, "-info").unwrap_or_else(|| "{}".into());

    let (mut ws, _) = tungstenite::connect(format!("ws://127.0.0.1:{port}"))
        .unwrap_or_else(|e| fatal(&format!("ws: {e}")));

    let reg = json!({ "event": register_event, "uuid": uuid });
    if ws.send(Message::Text(reg.to_string().into())).is_err() {
        fatal("failed to send register");
    }

    loop {
        match ws.read() {
            Ok(Message::Text(text)) => {
                let Ok(v) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                let event = v.get("event").and_then(|e| e.as_str()).unwrap_or("");
                let action = v.get("action").and_then(|a| a.as_str()).unwrap_or("");
                let context = v.get("context").and_then(|c| c.as_str()).unwrap_or("");
                match event {
                    "willAppear" | "keyDown" => {
                        let _ = ws.send(Message::Text(
                            json!({
                                "event": "logMessage",
                                "payload": { "message": format!("echo {event} {action}") }
                            })
                            .to_string()
                            .into(),
                        ));
                    }
                    "keyUp" => {
                        let _ = ws.send(Message::Text(
                            json!({
                                "event": "setTitle",
                                "context": context,
                                "payload": {
                                    "title": "OK",
                                    "target": 0
                                }
                            })
                            .to_string()
                            .into(),
                        ));
                        let _ = ws.send(Message::Text(
                            json!({
                                "event": "logMessage",
                                "payload": { "message": format!("echo keyUp {action}") }
                            })
                            .to_string()
                            .into(),
                        ));
                    }
                    _ => {}
                }
            }
            Ok(Message::Ping(p)) => {
                let _ = ws.send(Message::Pong(p));
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn fatal(msg: &str) -> ! {
    eprintln!("keel-echo-plugin: {msg}");
    process::exit(1);
}
