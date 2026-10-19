//! keel-light — USB Key Light control (Rust RPC + optional Qt UI).

use keel_light::{rpc_server, LightService};
use keel_rpc::{generate_rpc_token, write_rpc_token, LIGHT_PORT, RPC_TOKEN_FILENAME};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tracing::info;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();
    let headless = args.iter().any(|a| a == "--headless");
    let port = args
        .windows(2)
        .find(|w| w[0] == "--port")
        .and_then(|w| w[1].parse().ok())
        .unwrap_or(LIGHT_PORT);

    let service = match LightService::start() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to start keel-light: {e}");
            std::process::exit(1);
        }
    };

    let token = generate_rpc_token();
    let token_path = match keel_store::keel_light_dir() {
        Ok(dir) => dir.join(RPC_TOKEN_FILENAME),
        Err(e) => {
            eprintln!("rpc token path: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = write_rpc_token(&token_path, &token) {
        eprintln!("failed to write rpc token: {e}");
        std::process::exit(1);
    }
    info!("rpc token written to {}", token_path.display());

    if let Err(e) = rpc_server::spawn(Arc::clone(&service), port, token.clone()) {
        eprintln!("RPC listen failed: {e}");
        std::process::exit(1);
    }
    info!(
        "keel-light ready (usb_lights={}, port={})",
        service.lights.list().len(),
        port
    );

    if headless {
        info!("headless mode — Ctrl+C to exit");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }

    if let Some(ui) = find_qt_ui() {
        info!("launching Qt UI: {}", ui.display());
        let status = Command::new(&ui)
            .env("KEEL_LIGHT_PORT", port.to_string())
            .env("KEEL_LIGHT_RPC_TOKEN", &token)
            .status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                eprintln!("Qt UI exited: {s}");
                std::process::exit(s.code().unwrap_or(1));
            }
            Err(e) => {
                eprintln!("failed to launch Qt UI ({e}). Build with apps/keel-light/ui/build.sh");
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(3600));
                }
            }
        }
    } else {
        eprintln!(
            "Qt UI binary not found. Build with:\n  apps/keel-light/ui/build.sh\n\
             Running engine headless on ws://127.0.0.1:{port}"
        );
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }
}

fn find_qt_ui() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("apps/keel-light/ui/build/keel-light-ui"),
        PathBuf::from("ui/build/keel-light-ui"),
        PathBuf::from("/usr/local/bin/keel-light-ui"),
    ];
    candidates.into_iter().find(|p| p.exists()).or_else(|| {
        std::env::current_exe().ok().and_then(|exe| {
            let p = exe.parent()?.join("keel-light-ui");
            p.exists().then_some(p)
        })
    })
}
