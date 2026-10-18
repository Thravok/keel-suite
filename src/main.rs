//! Keel — offline, low-resource Stream Deck alternative.
//!
//! Rust + egui/eframe (OpenGL via glow). No WebView, no Qt runtime.

use keel::hid::HidDeviceManager;
use keel::plugin_host::PluginHost;
use keel::plugin_store::PluginStore;
use keel::store::ProfileStore;
use keel::ui::KeelApp;
use eframe::egui;
use std::sync::Arc;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::TrayIconBuilder;

fn main() -> eframe::Result<()> {
    let Some(dirs) = directories::ProjectDirs::from("app", "Keel", "Keel") else {
        eprintln!("could not resolve app data directory");
        std::process::exit(1);
    };

    let store = match ProfileStore::open(dirs.data_dir()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to open profile store: {e}");
            std::process::exit(1);
        }
    };
    let hid = HidDeviceManager::new();

    let plugin_store = match PluginStore::open(dirs.data_dir()) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("failed to open plugin store: {e}");
            std::process::exit(1);
        }
    };
    let plugin_host = match PluginHost::start(Arc::clone(&plugin_store)) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("failed to start plugin host: {e}");
            std::process::exit(1);
        }
    };

    let tray_menu = Menu::new();
    let open_item = MenuItem::new("Open Keel", true, None);
    let quit_item = MenuItem::new("Quit Keel", true, None);
    let _ = tray_menu.append(&open_item);
    let _ = tray_menu.append(&quit_item);
    let open_id = open_item.id().clone();
    let quit_id = quit_item.id().clone();
    let _tray = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("Keel — offline command deck")
        .with_title("Keel")
        .build()
        .ok();

    let start_hidden = store.settings().start_hidden;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Keel")
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([800.0, 520.0])
            .with_visible(!start_hidden),
        ..Default::default()
    };

    let menu_rx = MenuEvent::receiver();
    let plugin_host_exit = Arc::clone(&plugin_host);

    eframe::run_native(
        "Keel",
        options,
        Box::new(move |cc| {
            let mut fonts = egui::FontDefinitions::default();
            egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
            cc.egui_ctx.set_fonts(fonts);

            Ok(Box::new(RootApp {
                inner: KeelApp::new(
                    Arc::clone(&store),
                    Arc::clone(&hid),
                    Arc::clone(&plugin_host),
                ),
                hid,
                plugin_host: plugin_host_exit,
                open_id,
                quit_id,
                menu_rx,
            }))
        }),
    )
}

struct RootApp {
    inner: KeelApp,
    hid: Arc<HidDeviceManager>,
    plugin_host: Arc<PluginHost>,
    open_id: tray_icon::menu::MenuId,
    quit_id: tray_icon::menu::MenuId,
    menu_rx: &'static crossbeam_channel::Receiver<MenuEvent>,
}

impl eframe::App for RootApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Close button hides to tray; Quit from tray exits.
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        while let Ok(ev) = self.menu_rx.try_recv() {
            if ev.id == self.open_id {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            } else if ev.id == self.quit_id {
                self.hid.stop();
                self.plugin_host.stop();
                self.inner.on_exit(None);
                std::process::exit(0);
            }
        }
        self.inner.update(ctx, frame);
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.hid.stop();
        self.plugin_host.stop();
        self.inner.on_exit(gl);
    }
}
