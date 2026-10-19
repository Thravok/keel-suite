//! Offline smoke: start service, bind authenticated WS RPC, exercise methods.

use keel_audio::{rpc_server, AudioService};
use keel_rpc::{
    generate_rpc_token, read_rpc_token, write_rpc_token, RpcClient, RPC_TOKEN_FILENAME,
};
use serde_json::json;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() {
    // Never pop the macOS admin sheet in CI / smoke tests.
    let started = AudioService::start_with_options(true).expect("start");
    let _runtime = started.runtime;
    let svc = started.service;

    let token = generate_rpc_token();
    let token_path = keel_store::keel_audio_dir()
        .expect("audio dir")
        .join(RPC_TOKEN_FILENAME);
    write_rpc_token(&token_path, &token).expect("write token");

    // Ephemeral port to avoid colliding with a running engine.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    rpc_server::spawn(Arc::clone(&svc), port, token.clone()).expect("rpc spawn");
    thread::sleep(Duration::from_millis(100));

    // Unauthenticated connect must fail.
    let bare = tungstenite::connect(format!("ws://127.0.0.1:{port}/"));
    if bare.is_ok() {
        eprintln!("FAIL: unauthenticated WS upgrade succeeded");
        std::process::exit(1);
    }

    let file_token = read_rpc_token(&token_path).expect("read token");
    assert_eq!(file_token, token);

    let methods = [
        ("getOutputDevices", json!({})),
        ("getMixerState", json!({})),
        ("listEffects", json!({})),
        ("getDevices", json!({})),
        ("subscribeMeters", json!({})),
    ];
    for (m, p) in methods {
        match RpcClient::call(port, &token, m, &p) {
            Ok(v) => println!("OK {m}: {}", serde_json::to_string(&v).unwrap()),
            Err(e) => {
                eprintln!("FAIL {m}: {e}");
                std::process::exit(1);
            }
        }
    }
    let state = svc.engine.state();
    if let Some(ch) = state.channels.first() {
        let _ = RpcClient::call(
            port,
            &token,
            "addEffect",
            &json!({"channelId": ch.id, "kind": "equalizer"}),
        );
        let _ = RpcClient::call(
            port,
            &token,
            "setChannelVolume",
            &json!({"channelId": ch.id, "volume": 0.8}),
        );
        if let Some(mix) = state.mixes.first() {
            let _ = RpcClient::call(
                port,
                &token,
                "setRouting",
                &json!({"mixId": mix.id, "channelId": ch.id, "level": 1.0}),
            );
        }
    }
    println!("rpc smoke passed (authenticated WS on port {port})");
}
