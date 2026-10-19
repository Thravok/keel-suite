//! Offline smoke: enumerate USB lights and exercise authenticated WS RPC.

use keel_light::{rpc_server, LightService};
use keel_rpc::{
    generate_rpc_token, read_rpc_token, write_rpc_token, RpcClient, RPC_TOKEN_FILENAME,
};
use serde_json::json;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() {
    let svc = LightService::start().expect("start");

    let token = generate_rpc_token();
    let token_path = keel_store::keel_light_dir()
        .expect("light dir")
        .join(RPC_TOKEN_FILENAME);
    write_rpc_token(&token_path, &token).expect("write token");

    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    rpc_server::spawn(Arc::clone(&svc), port, token.clone()).expect("rpc spawn");
    thread::sleep(Duration::from_millis(100));

    let bare = tungstenite::connect(format!("ws://127.0.0.1:{port}/"));
    if bare.is_ok() {
        eprintln!("FAIL: unauthenticated WS upgrade succeeded");
        std::process::exit(1);
    }

    let file_token = read_rpc_token(&token_path).expect("read token");
    assert_eq!(file_token, token);

    let methods = [("getLights", json!({})), ("refreshLights", json!({}))];
    for (m, p) in methods {
        match RpcClient::call(port, &token, m, &p) {
            Ok(v) => println!("OK {m}: {}", serde_json::to_string(&v).unwrap()),
            Err(e) => {
                eprintln!("FAIL {m}: {e}");
                std::process::exit(1);
            }
        }
    }
    let lights = svc.lights.list();
    if let Some(light) = lights.first() {
        let id = &light.info.id;
        let _ = RpcClient::call(
            port,
            &token,
            "setLightBrightness",
            &json!({"id": id, "brightness": 35}),
        );
        let _ = RpcClient::call(
            port,
            &token,
            "setLightTemperature",
            &json!({"id": id, "temperature": 210}),
        );
        let _ = RpcClient::call(
            port,
            &token,
            "setLightPower",
            &json!({"id": id, "on": true}),
        );
        println!("exercised light {} brightness={} temp={}", id, 35, 210);
    } else {
        println!("no USB lights attached — discovery-only smoke ok");
    }
    println!("rpc smoke passed (authenticated WS on port {port})");
}
