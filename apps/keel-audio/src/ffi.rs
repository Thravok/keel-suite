//! C ABI for the Qt UI (`apps/keel-audio/ui`).

use crate::service::AudioService;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;
use std::sync::Arc;
use std::sync::OnceLock;

static SERVICE: OnceLock<Arc<AudioService>> = OnceLock::new();

fn svc() -> Option<&'static Arc<AudioService>> {
    SERVICE.get()
}

/// Start engine + RPC. Returns 0 on success.
#[no_mangle]
pub extern "C" fn keel_audio_start(port: u16) -> i32 {
    let started = match AudioService::start() {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let port = if port == 0 {
        keel_rpc::DEFAULT_PORT
    } else {
        port
    };
    let token = keel_rpc::generate_rpc_token();
    let Ok(dir) = keel_store::keel_audio_dir() else {
        return -3;
    };
    let token_path = dir.join(keel_rpc::RPC_TOKEN_FILENAME);
    if keel_rpc::write_rpc_token(&token_path, &token).is_err() {
        return -3;
    }
    if crate::rpc_server::spawn(Arc::clone(&started.service), port, token).is_err() {
        return -2;
    }
    // Leak runtime on this thread — cpal streams are !Send and must outlive the process.
    if let Some(rt) = started.runtime {
        std::mem::forget(rt);
    }
    let _ = SERVICE.set(started.service);
    0
}

#[no_mangle]
pub extern "C" fn keel_audio_stop() {
    // Process exit tears down listeners; reserved for future clean shutdown.
}

/// JSON snapshot of mixer state. Caller must `keel_audio_string_free`.
#[no_mangle]
pub extern "C" fn keel_audio_mixer_json() -> *mut c_char {
    let Some(s) = svc() else {
        return ptr::null_mut();
    };
    let json = serde_json::to_string(&s.engine.state()).unwrap_or_else(|_| "{}".into());
    CString::new(json)
        .map(|c| c.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// # Safety
/// `method` must be a valid NUL-terminated C string. `params_json` may be null or a valid C string.
#[no_mangle]
pub unsafe extern "C" fn keel_audio_rpc(
    method: *const c_char,
    params_json: *const c_char,
) -> *mut c_char {
    let Some(s) = svc() else {
        return ptr::null_mut();
    };
    if method.is_null() {
        return ptr::null_mut();
    }
    let method = unsafe { CStr::from_ptr(method) }.to_string_lossy();
    let params = if params_json.is_null() {
        serde_json::json!({})
    } else {
        let raw = unsafe { CStr::from_ptr(params_json) }.to_string_lossy();
        serde_json::from_str(&raw).unwrap_or(serde_json::json!({}))
    };
    let value = match s.handle_rpc(&method, &params) {
        Ok(v) => serde_json::json!({"ok": true, "result": v}),
        Err((code, message)) => serde_json::json!({"ok": false, "code": code, "message": message}),
    };
    let text = value.to_string();
    CString::new(text)
        .map(|c| c.into_raw())
        .unwrap_or(ptr::null_mut())
}

/// # Safety
/// `ptr` must be null or a pointer previously returned by a `keel_audio_*` string API.
#[no_mangle]
pub unsafe extern "C" fn keel_audio_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(ptr));
    }
}

#[no_mangle]
pub extern "C" fn keel_audio_hal_installed() -> i32 {
    svc()
        .map(|s| if s.hal.lock().is_installed() { 1 } else { 0 })
        .unwrap_or(0)
}
