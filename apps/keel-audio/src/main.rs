//! keel-audio — starts the Rust engine + JSON-RPC, then launches the Qt UI if built.

use keel_audio::{rpc_server, AudioService};
use keel_rpc::{generate_rpc_token, write_rpc_token, RPC_TOKEN_FILENAME};
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
    let compat_1884 = args.iter().any(|a| a == "--compat-port");
    let skip_hal = args.iter().any(|a| a == "--skip-hal-install");
    let port = if compat_1884 {
        keel_rpc::WAVELINK_COMPAT_PORT
    } else {
        keel_rpc::DEFAULT_PORT
    };

    let started = match AudioService::start_with_options(skip_hal) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to start keel-audio: {e}");
            std::process::exit(1);
        }
    };
    // cpal streams must stay on this thread (!Send).
    let _audio_runtime = started.runtime;
    let service = started.service;

    let token = generate_rpc_token();
    let token_path = match keel_store::keel_audio_dir() {
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
    {
        let hal = service.hal.lock();
        info!(
            "keel-audio engine ready (hal_installed={}, hal_status={:?}, build={:?}/{}, wave_devices={})",
            hal.is_installed(),
            hal.report.status,
            hal.report.installed_build,
            hal.report.bundled_build,
            service.wave.list().len()
        );
    }

    if headless {
        info!("headless mode — Ctrl+C to exit");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }

    if let Some(ui) = find_qt_ui() {
        info!("launching Qt UI: {}", ui.display());
        let status = Command::new(&ui)
            .env("KEEL_AUDIO_PORT", port.to_string())
            .env("KEEL_AUDIO_RPC_TOKEN", &token)
            .status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                eprintln!("Qt UI exited: {s}");
                std::process::exit(s.code().unwrap_or(1));
            }
            Err(e) => {
                eprintln!(
                    "failed to launch Qt UI ({e}). Build it with apps/keel-audio/ui/build.sh"
                );
                eprintln!(
                    "Engine remains up on ws://127.0.0.1:{port} — use --headless to skip UI."
                );
                // Keep engine alive so RPC still works.
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(3600));
                }
            }
        }
    } else {
        eprintln!(
            "Qt UI binary not found. Build with:\n  apps/keel-audio/ui/build.sh\n\
             Running engine headless on ws://127.0.0.1:{port}"
        );
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }
}

fn find_qt_ui() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("apps/keel-audio/ui/build/keel-audio-ui"),
        PathBuf::from("ui/build/keel-audio-ui"),
        PathBuf::from("/usr/local/bin/keel-audio-ui"),
        dirs_next_candidate(),
    ];
    candidates.into_iter().find(|p| p.exists())
}

fn dirs_next_candidate() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("keel-audio-ui");
            if p.exists() {
                return p;
            }
        }
    }
    PathBuf::from("keel-audio-ui")
}
