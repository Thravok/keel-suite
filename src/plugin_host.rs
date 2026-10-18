//! Stream Deck SDK v2 plugin host: localhost WebSocket + CodePath spawn.

use crate::plugin_store::{InstalledPlugin, PluginStore};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use thiserror::Error;
use tungstenite::{accept, Message, WebSocket};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum PluginHostError {
    #[error("{0}")]
    Msg(String),
}

struct LivePlugin {
    child: Child,
    tx: mpsc::Sender<String>,
    registered: Arc<AtomicBool>,
}

struct HostInner {
    plugins: HashMap<String, LivePlugin>,
    contexts: HashMap<String, (String, String)>,
    key_updates: Vec<PluginKeyUpdate>,
}

#[derive(Debug, Clone)]
pub struct PluginKeyUpdate {
    pub plugin_id: String,
    pub context: String,
    pub event: String,
    pub payload: Value,
}

struct PendingPlugin {
    plugin_id: String,
    outbound_rx: Option<mpsc::Receiver<String>>,
    registered: Arc<AtomicBool>,
}

fn pending_map() -> &'static Mutex<HashMap<String, PendingPlugin>> {
    static PENDING: OnceLock<Mutex<HashMap<String, PendingPlugin>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

pub struct PluginHost {
    store: Arc<PluginStore>,
    port: Arc<AtomicU16>,
    inner: Arc<Mutex<HostInner>>,
    running: Arc<AtomicBool>,
}

impl PluginHost {
    pub fn start(store: Arc<PluginStore>) -> Result<Arc<Self>, PluginHostError> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|e| PluginHostError::Msg(format!("bind ws: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| PluginHostError::Msg(e.to_string()))?
            .port();

        let host = Arc::new(Self {
            store,
            port: Arc::new(AtomicU16::new(port)),
            inner: Arc::new(Mutex::new(HostInner {
                plugins: HashMap::new(),
                contexts: HashMap::new(),
                key_updates: Vec::new(),
            })),
            running: Arc::new(AtomicBool::new(true)),
        });

        let accept_host = Arc::clone(&host);
        thread::spawn(move || {
            while accept_host.running.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let h = Arc::clone(&accept_host);
                        thread::spawn(move || handle_client(h, stream));
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(host)
    }

    pub fn port(&self) -> u16 {
        self.port.load(Ordering::SeqCst)
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        let mut guard = self.inner.lock();
        for (_, mut live) in guard.plugins.drain() {
            let _ = live.child.kill();
            let _ = live.child.wait();
        }
        let _ = TcpStream::connect(("127.0.0.1", self.port()));
    }

    pub fn store(&self) -> &PluginStore {
        &self.store
    }

    pub fn drain_key_updates(&self) -> Vec<PluginKeyUpdate> {
        std::mem::take(&mut self.inner.lock().key_updates)
    }

    /// Ensure plugin process is up, then send willAppear + keyDown + keyUp.
    pub fn fire_action(
        &self,
        plugin_id: &str,
        action_uuid: &str,
        settings: &Value,
        coordinates: (u8, u8),
    ) -> Result<(), PluginHostError> {
        self.ensure_plugin(plugin_id)?;
        let context = Uuid::new_v4().to_string();
        {
            let mut guard = self.inner.lock();
            guard
                .contexts
                .insert(context.clone(), (plugin_id.to_string(), action_uuid.to_string()));
        }

        let device = "keel-virtual-0";
        let (col, row) = coordinates;
        let appear = json!({
            "event": "willAppear",
            "action": action_uuid,
            "context": context,
            "device": device,
            "payload": {
                "settings": settings,
                "coordinates": { "column": col, "row": row },
                "controller": "Keypad",
                "isInMultiAction": false
            }
        });
        let down = json!({
            "event": "keyDown",
            "action": action_uuid,
            "context": context,
            "device": device,
            "payload": {
                "settings": settings,
                "coordinates": { "column": col, "row": row },
                "isInMultiAction": false
            }
        });
        let up = json!({
            "event": "keyUp",
            "action": action_uuid,
            "context": context,
            "device": device,
            "payload": {
                "settings": settings,
                "coordinates": { "column": col, "row": row },
                "isInMultiAction": false
            }
        });

        self.send_to_plugin(plugin_id, &appear.to_string())?;
        thread::sleep(Duration::from_millis(20));
        self.send_to_plugin(plugin_id, &down.to_string())?;
        thread::sleep(Duration::from_millis(40));
        self.send_to_plugin(plugin_id, &up.to_string())?;
        // Allow inbound setTitle / logMessage to land before callers drain.
        thread::sleep(Duration::from_millis(80));
        Ok(())
    }

    fn send_to_plugin(&self, plugin_id: &str, msg: &str) -> Result<(), PluginHostError> {
        let guard = self.inner.lock();
        let live = guard
            .plugins
            .get(plugin_id)
            .ok_or_else(|| PluginHostError::Msg(format!("plugin not running: {plugin_id}")))?;
        live.tx
            .send(msg.to_string())
            .map_err(|_| PluginHostError::Msg("plugin websocket closed".into()))?;
        Ok(())
    }

    fn ensure_plugin(&self, plugin_id: &str) -> Result<(), PluginHostError> {
        {
            let guard = self.inner.lock();
            if let Some(live) = guard.plugins.get(plugin_id) {
                if live.registered.load(Ordering::SeqCst) {
                    return Ok(());
                }
            }
        }

        let installed = self
            .store
            .list()
            .map_err(|e| PluginHostError::Msg(e.to_string()))?
            .into_iter()
            .find(|p| p.id == plugin_id)
            .ok_or_else(|| PluginHostError::Msg(format!("plugin not installed: {plugin_id}")))?;

        if installed.manifest.is_html_code_path() {
            return Err(PluginHostError::Msg(
                "HTML CodePath plugins need an embedded webview (deferred); use a native/JS CodePath package".into(),
            ));
        }

        {
            let mut guard = self.inner.lock();
            if let Some(mut live) = guard.plugins.remove(plugin_id) {
                let _ = live.child.kill();
                let _ = live.child.wait();
            }
        }

        let plugin_uuid = installed
            .manifest
            .uuid
            .clone()
            .unwrap_or_else(|| plugin_id.trim_end_matches(".sdPlugin").to_string());

        let (tx, rx) = mpsc::channel::<String>();
        let registered = Arc::new(AtomicBool::new(false));

        pending_map().lock().insert(
            plugin_uuid.clone(),
            PendingPlugin {
                plugin_id: plugin_id.to_string(),
                outbound_rx: Some(rx),
                registered: Arc::clone(&registered),
            },
        );

        let child = match spawn_codepath(&installed, self.port(), &plugin_uuid) {
            Ok(c) => c,
            Err(e) => {
                pending_map().lock().remove(&plugin_uuid);
                return Err(e);
            }
        };

        {
            let mut guard = self.inner.lock();
            guard.plugins.insert(
                plugin_id.to_string(),
                LivePlugin {
                    child,
                    tx,
                    registered: Arc::clone(&registered),
                },
            );
        }

        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if registered.load(Ordering::SeqCst) {
                return Ok(());
            }
            // Surface early spawn failure
            {
                let mut guard = self.inner.lock();
                if let Some(live) = guard.plugins.get_mut(plugin_id) {
                    if let Ok(Some(status)) = live.child.try_wait() {
                        pending_map().lock().remove(&plugin_uuid);
                        guard.plugins.remove(plugin_id);
                        return Err(PluginHostError::Msg(format!(
                            "plugin exited before register: {status}"
                        )));
                    }
                }
            }
            thread::sleep(Duration::from_millis(25));
        }
        Err(PluginHostError::Msg(format!(
            "plugin did not register within 5s: {plugin_id}"
        )))
    }
}

fn app_info_json() -> String {
    json!({
        "application": {
            "language": "en",
            "platform": std::env::consts::OS,
            "version": "0.1.0",
            "name": "Keel"
        },
        "plugin": {
            "version": "0.1.0"
        },
        "devicePixelRatio": 1,
        "devices": [{
            "id": "keel-virtual-0",
            "name": "Keel Virtual",
            "size": { "columns": 5, "rows": 3 },
            "type": 0
        }]
    })
    .to_string()
}

fn spawn_codepath(
    plugin: &InstalledPlugin,
    port: u16,
    plugin_uuid: &str,
) -> Result<Child, PluginHostError> {
    let code = plugin
        .manifest
        .code_path_for_host()
        .ok_or_else(|| PluginHostError::Msg("manifest missing CodePath".into()))?;

    let exe = resolve_code_path(&plugin.dir, code)?;
    let info = app_info_json();

    let mut cmd = if is_javascript(&exe) {
        let mut c = Command::new("node");
        c.arg(&exe);
        c
    } else {
        Command::new(&exe)
    };

    cmd.arg("-port")
        .arg(port.to_string())
        .arg("-pluginUUID")
        .arg(plugin_uuid)
        .arg("-registerEvent")
        .arg("registerPlugin")
        .arg("-info")
        .arg(&info)
        .current_dir(&plugin.dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    cmd.spawn()
        .map_err(|e| PluginHostError::Msg(format!("spawn {}: {e}", exe.display())))
}

fn is_javascript(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("js") || e.eq_ignore_ascii_case("mjs"))
        .unwrap_or(false)
}

fn resolve_code_path(plugin_dir: &Path, code: &str) -> Result<PathBuf, PluginHostError> {
    let direct = plugin_dir.join(code);
    if direct.exists() {
        return Ok(direct);
    }
    let as_path = PathBuf::from(code);
    if as_path.is_absolute() && as_path.exists() {
        return Ok(as_path);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let cand = dir.join(code);
            if cand.exists() {
                return Ok(cand);
            }
        }
    }
    if let Ok(path) = which_in_path(code) {
        return Ok(path);
    }
    Err(PluginHostError::Msg(format!(
        "CodePath not found: {code} (looked in {})",
        plugin_dir.display()
    )))
}

fn which_in_path(name: &str) -> Result<PathBuf, ()> {
    let path = std::env::var_os("PATH").ok_or(())?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Ok(cand);
        }
    }
    Err(())
}

fn handle_client(host: Arc<PluginHost>, stream: TcpStream) {
    // Short read timeout so we can interleave outbound host→plugin frames
    // without deadlocking on a shared WebSocket (single-threaded pump).
    let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));

    let mut ws = match accept(stream) {
        Ok(ws) => ws,
        Err(_) => return,
    };

    let uuid = match read_register(&mut ws) {
        Some(u) => u,
        None => return,
    };

    let (plugin_id, outbound_rx, registered) = {
        let mut pending = pending_map().lock();
        match pending.remove(&uuid) {
            Some(p) => (
                p.plugin_id,
                p.outbound_rx.expect("outbound rx"),
                p.registered,
            ),
            None => return,
        }
    };

    registered.store(true, Ordering::SeqCst);

    loop {
        while let Ok(msg) = outbound_rx.try_recv() {
            if ws.send(Message::Text(msg.into())).is_err() {
                cleanup_plugin(&host, &plugin_id);
                return;
            }
        }

        match ws.read() {
            Ok(Message::Text(text)) => {
                handle_plugin_message(&host, &plugin_id, &text);
            }
            Ok(Message::Ping(p)) => {
                let _ = ws.send(Message::Pong(p));
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // Drain outbound again on next loop.
            }
            Err(_) => break,
        }
    }

    cleanup_plugin(&host, &plugin_id);
}

fn cleanup_plugin(host: &PluginHost, plugin_id: &str) {
    let mut guard = host.inner.lock();
    if let Some(mut live) = guard.plugins.remove(plugin_id) {
        let _ = live.child.kill();
        let _ = live.child.wait();
    }
}

fn read_register(ws: &mut WebSocket<TcpStream>) -> Option<String> {
    match ws.read() {
        Ok(Message::Text(text)) => {
            let v: Value = serde_json::from_str(&text).ok()?;
            let event = v.get("event")?.as_str()?;
            if event != "registerPlugin" && event != "registerPropertyInspector" {
                return None;
            }
            v.get("uuid")?.as_str().map(|s| s.to_string())
        }
        _ => None,
    }
}

fn handle_plugin_message(host: &PluginHost, plugin_id: &str, text: &str) {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return;
    };
    let event = v.get("event").and_then(|e| e.as_str()).unwrap_or("");
    match event {
        "setImage" | "setTitle" | "setState" | "showOk" | "showAlert" | "logMessage"
        | "openUrl" | "setSettings" | "getSettings" | "setGlobalSettings" | "getGlobalSettings" => {
            let context = v
                .get("context")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            let payload = v.get("payload").cloned().unwrap_or(Value::Null);
            if event == "openUrl" {
                if let Some(url) = payload.get("url").and_then(|u| u.as_str()) {
                    let _ = open::that(url);
                }
            }
            if event == "logMessage" {
                if let Some(msg) = payload.get("message").and_then(|m| m.as_str()) {
                    eprintln!("[plugin {plugin_id}] {msg}");
                }
            }
            host.inner.lock().key_updates.push(PluginKeyUpdate {
                plugin_id: plugin_id.to_string(),
                context,
                event: event.to_string(),
                payload,
            });
        }
        _ => {}
    }
}
