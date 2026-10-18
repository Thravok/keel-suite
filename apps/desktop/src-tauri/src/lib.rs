//! Keel — offline command deck (Tauri commands + tray + HID).

mod actions;
mod hid;
mod layout;
mod model;
mod store;

use actions::{ActionOutcome, ActionRunner};
use hid::{parse_hex_color, HidDeviceManager};
use model::{
    AppSettings, AppStateSnapshot, DeviceInfo, KeyButton, LayoutKind, Page, Profile,
};
use parking_lot::Mutex;
use store::ProfileStore;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use uuid::Uuid;

pub struct AppCtx {
    pub store: Arc<ProfileStore>,
    pub hid: Arc<HidDeviceManager>,
    /// Serializes press handling across virtual UI + HID
    press_lock: Mutex<()>,
}

fn err(e: impl ToString) -> String {
    e.to_string()
}

fn emit_state(app: &AppHandle, ctx: &AppCtx) {
    let snap = ctx.store.snapshot(ctx.hid.devices());
    let _ = app.emit("keel://state", snap);
}

fn sync_hid_page(ctx: &AppCtx) {
    let Some(profile) = ctx.store.active_profile() else {
        return;
    };
    let Some(page) = profile
        .pages
        .iter()
        .find(|p| p.id == profile.current_page_id)
    else {
        return;
    };
    let colors: Vec<(u8, u8, u8)> = page
        .keys
        .iter()
        .map(|k| parse_hex_color(&k.color))
        .collect();
    let titles: Vec<String> = page.keys.iter().map(|k| k.title.clone()).collect();
    let _ = ctx.hid.sync_page_colors(&colors, &titles);
    let brightness = ctx.store.settings().brightness;
    let _ = ctx.hid.set_brightness(brightness);
}

fn apply_outcome(ctx: &AppCtx, outcome: ActionOutcome) -> Result<(), String> {
    match outcome {
        ActionOutcome::None => Ok(()),
        ActionOutcome::Brightness(p) => {
            let mut s = ctx.store.settings();
            s.brightness = p;
            ctx.store.set_settings(s).map_err(err)?;
            let _ = ctx.hid.set_brightness(p);
            Ok(())
        }
        ActionOutcome::GoPage(page_id) => {
            ctx.store
                .update_active_profile(|profile| {
                    if profile.pages.iter().any(|p| p.id == page_id) {
                        profile.page_stack.push(profile.current_page_id.clone());
                        profile.current_page_id = page_id;
                    }
                })
                .map_err(err)?;
            sync_hid_page(ctx);
            Ok(())
        }
        ActionOutcome::GoBack => {
            ctx.store
                .update_active_profile(|profile| {
                    if let Some(prev) = profile.page_stack.pop() {
                        profile.current_page_id = prev;
                    }
                })
                .map_err(err)?;
            sync_hid_page(ctx);
            Ok(())
        }
    }
}

fn press_key_index(ctx: &AppCtx, index: usize) -> Result<(), String> {
    let _guard = ctx.press_lock.lock();
    let profile = ctx
        .store
        .active_profile()
        .ok_or_else(|| "no active profile".to_string())?;
    let page = profile
        .pages
        .iter()
        .find(|p| p.id == profile.current_page_id)
        .ok_or_else(|| "current page missing".to_string())?;
    let key = page
        .keys
        .get(index)
        .ok_or_else(|| format!("key index {index} out of range"))?;
    let outcome = ActionRunner::run(&key.action).map_err(err)?;
    apply_outcome(ctx, outcome)?;
    Ok(())
}

#[tauri::command]
fn get_state(ctx: State<'_, Arc<AppCtx>>) -> AppStateSnapshot {
    ctx.store.snapshot(ctx.hid.devices())
}

#[tauri::command]
fn list_layouts() -> Vec<(&'static str, LayoutKind, u8, u8)> {
    [
        LayoutKind::Mini,
        LayoutKind::Original,
        LayoutKind::Xl,
    ]
    .into_iter()
    .map(|l| (l.label(), l, l.rows(), l.cols()))
    .collect()
}

#[tauri::command]
fn create_profile(
    ctx: State<'_, Arc<AppCtx>>,
    app: AppHandle,
    name: String,
    layout: LayoutKind,
) -> Result<Profile, String> {
    let p = ctx.store.create_profile(name, layout).map_err(err)?;
    emit_state(&app, &ctx);
    Ok(p)
}

#[tauri::command]
fn save_profile(
    ctx: State<'_, Arc<AppCtx>>,
    app: AppHandle,
    profile: Profile,
) -> Result<Profile, String> {
    let p = ctx.store.save_profile(profile).map_err(err)?;
    sync_hid_page(&ctx);
    emit_state(&app, &ctx);
    Ok(p)
}

#[tauri::command]
fn delete_profile(
    ctx: State<'_, Arc<AppCtx>>,
    app: AppHandle,
    id: String,
) -> Result<(), String> {
    ctx.store.delete_profile(&id).map_err(err)?;
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
fn set_active_profile(
    ctx: State<'_, Arc<AppCtx>>,
    app: AppHandle,
    id: String,
) -> Result<(), String> {
    ctx.store.set_active_profile(id).map_err(err)?;
    sync_hid_page(&ctx);
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
fn set_settings(
    ctx: State<'_, Arc<AppCtx>>,
    app: AppHandle,
    settings: AppSettings,
) -> Result<(), String> {
    let brightness = settings.brightness;
    ctx.store.set_settings(settings).map_err(err)?;
    let _ = ctx.hid.set_brightness(brightness);
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
fn press_key(
    ctx: State<'_, Arc<AppCtx>>,
    app: AppHandle,
    index: usize,
) -> Result<(), String> {
    press_key_index(&ctx, index)?;
    emit_state(&app, &ctx);
    Ok(())
}

#[tauri::command]
fn update_key(
    ctx: State<'_, Arc<AppCtx>>,
    app: AppHandle,
    index: usize,
    key: KeyButton,
) -> Result<Profile, String> {
    let profile = ctx
        .store
        .update_active_profile(|profile| {
            if let Ok(keys) = ProfileStore::current_page_mut_keys(profile) {
                if let Some(slot) = keys.get_mut(index) {
                    *slot = key;
                }
            }
        })
        .map_err(err)?;
    sync_hid_page(&ctx);
    emit_state(&app, &ctx);
    Ok(profile)
}

#[tauri::command]
fn add_page(
    ctx: State<'_, Arc<AppCtx>>,
    app: AppHandle,
    name: String,
) -> Result<Profile, String> {
    let profile = ctx
        .store
        .update_active_profile(|profile| {
            let mut page = Page {
                id: Uuid::new_v4().to_string(),
                name,
                keys: vec![],
            };
            layout::ensure_page_size(&mut page, profile.layout);
            profile.pages.push(page);
        })
        .map_err(err)?;
    emit_state(&app, &ctx);
    Ok(profile)
}

#[tauri::command]
fn go_page(
    ctx: State<'_, Arc<AppCtx>>,
    app: AppHandle,
    page_id: String,
    push_stack: bool,
) -> Result<Profile, String> {
    let profile = ctx
        .store
        .update_active_profile(|profile| {
            if profile.pages.iter().any(|p| p.id == page_id) {
                if push_stack {
                    profile.page_stack.push(profile.current_page_id.clone());
                }
                profile.current_page_id = page_id;
            }
        })
        .map_err(err)?;
    sync_hid_page(&ctx);
    emit_state(&app, &ctx);
    Ok(profile)
}

#[tauri::command]
fn navigate_back(
    ctx: State<'_, Arc<AppCtx>>,
    app: AppHandle,
) -> Result<Profile, String> {
    apply_outcome(&ctx, ActionOutcome::GoBack)?;
    let profile = ctx
        .store
        .active_profile()
        .ok_or_else(|| "no active profile".to_string())?;
    emit_state(&app, &ctx);
    Ok(profile)
}

#[tauri::command]
fn list_devices(ctx: State<'_, Arc<AppCtx>>) -> Vec<DeviceInfo> {
    ctx.hid.devices()
}

#[tauri::command]
fn refresh_hid(ctx: State<'_, Arc<AppCtx>>, app: AppHandle) -> Result<(), String> {
    sync_hid_page(&ctx);
    emit_state(&app, &ctx);
    Ok(())
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Keel", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .tooltip("Keel")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data = app
                .path()
                .app_data_dir()
                .map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            let store = ProfileStore::open(&data)
                .map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            let hid = HidDeviceManager::new();

            let ctx = Arc::new(AppCtx {
                store: Arc::clone(&store),
                hid: Arc::clone(&hid),
                press_lock: Mutex::new(()),
            });

            let ctx_press = Arc::clone(&ctx);
            let handle = app.handle().clone();
            hid.set_on_press(Arc::new(move |key| {
                let index = key as usize;
                if let Err(e) = press_key_index(&ctx_press, index) {
                    eprintln!("keel hid press error: {e}");
                }
                emit_state(&handle, &ctx_press);
            }));
            hid.start();

            // Initial HID paint once a device appears (best-effort)
            sync_hid_page(&ctx);

            app.manage(ctx);
            setup_tray(app.handle())?;

            if let Some(win) = app.get_webview_window("main") {
                let start_hidden = store.settings().start_hidden;
                if start_hidden {
                    let _ = win.hide();
                }
                let app_handle = app.handle().clone();
                win.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        // Stay resident in tray
                        api.prevent_close();
                        if let Some(w) = app_handle.get_webview_window("main") {
                            let _ = w.hide();
                        }
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            list_layouts,
            create_profile,
            save_profile,
            delete_profile,
            set_active_profile,
            set_settings,
            press_key,
            update_key,
            add_page,
            go_page,
            navigate_back,
            list_devices,
            refresh_hid,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Keel");
}
