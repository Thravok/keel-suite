//! Offline smoke: import echo fixture, start host, fire one action UUID.

use keel::plugin_host::PluginHost;
use keel::plugin_store::PluginStore;
use std::env;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest_dir
        .join("../../fixtures/com.keel.echo.sdPlugin")
        .canonicalize()
        .unwrap_or_else(|_| manifest_dir.join("../../fixtures/com.keel.echo.sdPlugin"));
    let tmp = env::temp_dir().join(format!("keel-plugin-smoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("tmp");

    let store = Arc::new(PluginStore::open(&tmp).expect("store"));
    let installed = store.import_path(&fixture).unwrap_or_else(|e| {
        eprintln!("import failed: {e}");
        process::exit(1);
    });
    println!("imported {}", installed.id);

    // Copy echo binary next to plugin CodePath resolution (sibling of this smoke bin)
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .expect("exe dir");
    let echo_src = exe_dir.join("keel-echo-plugin");
    if !echo_src.exists() {
        eprintln!("missing {echo_src:?} — build keel-echo-plugin first");
        process::exit(1);
    }

    let host = PluginHost::start(Arc::clone(&store)).expect("host");
    println!("ws port {}", host.port());

    if let Err(e) = host.fire_action(
        &installed.id,
        "com.keel.echo.ping",
        &serde_json::json!({}),
        (0, 0),
    ) {
        eprintln!("fire_action failed: {e}");
        host.stop();
        process::exit(1);
    }

    thread::sleep(Duration::from_millis(300));
    let updates = host.drain_key_updates();
    println!("updates: {updates:?}");
    let ok = updates.iter().any(|u| u.event == "setTitle");
    host.stop();
    let _ = std::fs::remove_dir_all(&tmp);

    if ok {
        println!("PASS: setTitle received from echo plugin");
        process::exit(0);
    } else {
        eprintln!("FAIL: no setTitle from plugin");
        process::exit(1);
    }
}
