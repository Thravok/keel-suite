//! Low-resource egui control surface for Keel.

use crate::actions::{ActionOutcome, ActionRunner};
use crate::hid::{parse_hex_color, EncoderEvent, HidDeviceManager};
use crate::layout::ensure_page_size;
use crate::model::{ActionSpec, LayoutKind, MediaCommand, Page, Profile};
use crate::plugin_host::PluginHost;
use crate::plugin_store::InstalledPlugin;
use crate::presets::{icon_for_action, PRESETS};
use crate::store::ProfileStore;
use eframe::egui::{self, Color32, CornerRadius, RichText, Sense, Stroke, Vec2};
use egui_phosphor::regular;
use parking_lot::Mutex;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Selection {
    Key(usize),
    Encoder(usize),
    Touch(usize),
}

#[derive(Clone, Copy)]
enum PendingEncoder {
    Press(u8),
    Twist { index: u8, delta: i8 },
}

pub struct KeelApp {
    store: Arc<ProfileStore>,
    hid: Arc<HidDeviceManager>,
    plugin_host: Arc<PluginHost>,
    pending_keys: Arc<Mutex<Vec<u8>>>,
    pending_encoders: Arc<Mutex<Vec<PendingEncoder>>>,
    pending_touch: Arc<Mutex<Vec<u8>>>,
    selection: Option<Selection>,
    status: String,
    new_profile_name: String,
    new_layout: LayoutKind,
    show_settings: bool,
    installed_plugins: Vec<InstalledPlugin>,
    last_device_serial: Option<String>,
    /// When true, auto-adopt is skipped (user picked a device in the dropdown).
    device_manual: bool,
}

impl KeelApp {
    pub fn new(
        store: Arc<ProfileStore>,
        hid: Arc<HidDeviceManager>,
        plugin_host: Arc<PluginHost>,
    ) -> Self {
        let pending_keys = Arc::new(Mutex::new(Vec::new()));
        let pending_encoders = Arc::new(Mutex::new(Vec::new()));
        let pending_touch = Arc::new(Mutex::new(Vec::new()));

        let keys_c = Arc::clone(&pending_keys);
        hid.set_on_press(Arc::new(move |key| {
            keys_c.lock().push(key);
        }));
        let enc_c = Arc::clone(&pending_encoders);
        hid.set_on_encoder(Arc::new(move |ev| {
            let mut q = enc_c.lock();
            match ev {
                EncoderEvent::Down(i) => q.push(PendingEncoder::Press(i)),
                EncoderEvent::Twist { index, delta } => {
                    q.push(PendingEncoder::Twist { index, delta })
                }
                EncoderEvent::Up(_) => {}
            }
        }));
        let touch_c = Arc::clone(&pending_touch);
        hid.set_on_touch_point(Arc::new(move |i| {
            touch_c.lock().push(i);
        }));

        if let Some(p) = store.active_profile() {
            hid.set_prefer_layout(Some(p.layout));
        }
        hid.start();

        let settings = store.settings();
        let _ = hid.set_brightness(settings.brightness);
        let installed_plugins = plugin_host.store().list().unwrap_or_default();
        let app = Self {
            store,
            hid,
            plugin_host,
            pending_keys,
            pending_encoders,
            pending_touch,
            selection: None,
            status: "Ready — offline".into(),
            new_profile_name: "New profile".into(),
            new_layout: LayoutKind::Original,
            show_settings: false,
            installed_plugins,
            last_device_serial: None,
            device_manual: false,
        };
        app.sync_hardware_page();
        app
    }

    fn refresh_plugins(&mut self) {
        self.installed_plugins = self.plugin_host.store().list().unwrap_or_default();
    }

    fn import_plugin_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .set_title("Import Stream Deck plugin")
            .add_filter("Stream Deck plugin", &["streamDeckPlugin", "zip"])
            .pick_file()
            .or_else(|| {
                rfd::FileDialog::new()
                    .set_title("Import .sdPlugin folder")
                    .pick_folder()
            });
        let Some(path) = picked else {
            return;
        };
        match self.plugin_host.store().import_path(&path) {
            Ok(p) => {
                self.status = format!("Imported plugin {}", p.id);
                self.refresh_plugins();
            }
            Err(e) => self.status = format!("Import failed: {e}"),
        }
    }

    fn active_profile(&self) -> Option<Profile> {
        self.store.active_profile()
    }

    fn sync_hardware_page(&self) {
        let Some(profile) = self.active_profile() else {
            return;
        };
        self.hid.set_prefer_layout(Some(profile.layout));
        let Some(page) = profile
            .pages
            .iter()
            .find(|p| p.id == profile.current_page_id)
        else {
            return;
        };
        let colors: Vec<_> = page
            .keys
            .iter()
            .map(|k| parse_hex_color(&k.color))
            .collect();
        let titles: Vec<_> = page.keys.iter().map(|k| k.title.clone()).collect();
        let icons: Vec<_> = page
            .keys
            .iter()
            .map(|k| icon_for_action(&k.action).map(|s| s.to_string()))
            .collect();
        let encoder_titles: Vec<_> = profile.encoders.iter().map(|e| e.title.clone()).collect();
        let _ = self
            .hid
            .sync_page_colors(&colors, &titles, &icons, &encoder_titles);
        let _ = self.hid.set_brightness(self.store.settings().brightness);
    }

    fn maybe_adopt_device(&mut self) {
        if self.device_manual {
            // Still refresh status when the pinned device comes online.
            if let Some(dev) = self.hid.active_device() {
                if self.last_device_serial.as_deref() != Some(dev.serial.as_str()) {
                    self.last_device_serial = Some(dev.serial.clone());
                    self.status = format!("Editing: {} · {}", dev.kind, short_serial(&dev.serial));
                    self.sync_hardware_page();
                }
            }
            return;
        }
        let Some(dev) = self.hid.active_device() else {
            self.last_device_serial = None;
            return;
        };
        let serial = Some(dev.serial.clone());
        if self.last_device_serial == serial {
            return;
        }
        self.last_device_serial = serial;
        let Some(profile) = self.active_profile() else {
            return;
        };
        if profile.layout == dev.layout {
            self.status = format!("Connected: {}", dev.kind);
            return;
        }
        match self.store.adopt_layout(dev.layout) {
            Ok(_) => {
                self.selection = None;
                self.sync_hardware_page();
                self.status = format!("Adopted {} layout", dev.kind);
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    fn select_physical_device(&mut self, serial: Option<String>) {
        self.device_manual = serial.is_some();
        self.hid.set_prefer_serial(serial.clone());
        self.last_device_serial = None; // force status/sync on next connect
        if let Some(serial) = serial {
            if let Some(dev) = self
                .hid
                .available_devices()
                .into_iter()
                .find(|d| d.serial == serial)
            {
                match self.store.adopt_layout(dev.layout) {
                    Ok(_) => {
                        self.selection = None;
                        self.sync_hardware_page();
                        self.status =
                            format!("Editing: {} · {}", dev.kind, short_serial(&dev.serial));
                    }
                    Err(e) => self.status = e.to_string(),
                }
            } else {
                self.status = format!("Waiting for device {}", short_serial(&serial));
            }
        } else {
            self.status = "Device: auto".into();
            self.sync_hardware_page();
        }
    }

    fn press_key_index(&mut self, index: usize) {
        let outcome = {
            let Some(profile) = self.active_profile() else {
                self.status = "No active profile".into();
                return;
            };
            let Some(page) = profile
                .pages
                .iter()
                .find(|p| p.id == profile.current_page_id)
            else {
                self.status = "Current page missing".into();
                return;
            };
            let Some(key) = page.keys.get(index) else {
                self.status = format!("Key {index} out of range");
                return;
            };
            match ActionRunner::run(&key.action) {
                Ok(o) => o,
                Err(e) => {
                    self.status = e.to_string();
                    return;
                }
            }
        };

        match outcome {
            ActionOutcome::None => self.status = format!("Fired key {index}"),
            ActionOutcome::Brightness(p) => {
                let mut s = self.store.settings();
                s.brightness = p;
                let _ = self.store.set_settings(s);
                let _ = self.hid.set_brightness(p);
                self.status = format!("Brightness {p}%");
            }
            ActionOutcome::GoPage(page_id) => {
                let _ = self.store.update_active_profile(|profile| {
                    if profile.pages.iter().any(|p| p.id == page_id) {
                        profile.page_stack.push(profile.current_page_id.clone());
                        profile.current_page_id = page_id;
                    }
                });
                self.selection = None;
                self.sync_hardware_page();
                self.status = "Opened page".into();
            }
            ActionOutcome::GoBack => {
                let _ = self.store.update_active_profile(|profile| {
                    if let Some(prev) = profile.page_stack.pop() {
                        profile.current_page_id = prev;
                    }
                });
                self.selection = None;
                self.sync_hardware_page();
                self.status = "Back".into();
            }
            ActionOutcome::Plugin {
                plugin_id,
                action_uuid,
                settings,
            } => {
                let coords = index_to_coords(index, &self.active_profile());
                match self
                    .plugin_host
                    .fire_action(&plugin_id, &action_uuid, &settings, coords)
                {
                    Ok(()) => {
                        let updates = self.plugin_host.drain_key_updates();
                        if let Some(title) = updates.iter().rev().find_map(|u| {
                            if u.event == "setTitle" {
                                u.payload
                                    .get("title")
                                    .and_then(|t| t.as_str())
                                    .map(|s| s.to_string())
                            } else {
                                None
                            }
                        }) {
                            self.status = format!("Plugin {action_uuid} → {title}");
                        } else {
                            self.status = format!("Plugin {action_uuid} fired");
                        }
                    }
                    Err(e) => self.status = e.to_string(),
                }
            }
        }
    }

    fn drain_hid(&mut self) {
        let keys: Vec<u8> = self.pending_keys.lock().drain(..).collect();
        for k in keys {
            self.press_key_index(k as usize);
        }
        let encoders: Vec<PendingEncoder> = self.pending_encoders.lock().drain(..).collect();
        for ev in encoders {
            match ev {
                PendingEncoder::Press(i) => self.fire_encoder_press(i as usize),
                PendingEncoder::Twist { index, delta } => {
                    self.fire_encoder_twist(index as usize, delta)
                }
            }
        }
        let touches: Vec<u8> = self.pending_touch.lock().drain(..).collect();
        for i in touches {
            self.fire_touch(i as usize);
        }
    }

    fn apply_outcome(&mut self, outcome: ActionOutcome, label: &str) {
        match outcome {
            ActionOutcome::None => self.status = label.into(),
            ActionOutcome::Brightness(p) => {
                let mut s = self.store.settings();
                s.brightness = p;
                let _ = self.store.set_settings(s);
                let _ = self.hid.set_brightness(p);
                self.status = format!("Brightness {p}%");
            }
            ActionOutcome::GoPage(page_id) => {
                let _ = self.store.update_active_profile(|profile| {
                    if profile.pages.iter().any(|p| p.id == page_id) {
                        profile.page_stack.push(profile.current_page_id.clone());
                        profile.current_page_id = page_id;
                    }
                });
                self.selection = None;
                self.sync_hardware_page();
                self.status = "Opened page".into();
            }
            ActionOutcome::GoBack => {
                let _ = self.store.update_active_profile(|profile| {
                    if let Some(prev) = profile.page_stack.pop() {
                        profile.current_page_id = prev;
                    }
                });
                self.selection = None;
                self.sync_hardware_page();
                self.status = "Back".into();
            }
            ActionOutcome::Plugin {
                plugin_id,
                action_uuid,
                settings,
            } => {
                match self
                    .plugin_host
                    .fire_action(&plugin_id, &action_uuid, &settings, (0, 0))
                {
                    Ok(()) => self.status = format!("Plugin {action_uuid} fired"),
                    Err(e) => self.status = e.to_string(),
                }
            }
        }
    }

    fn fire_encoder_press(&mut self, index: usize) {
        let Some(profile) = self.active_profile() else {
            return;
        };
        let Some(enc) = profile.encoders.get(index) else {
            return;
        };
        match ActionRunner::run(&enc.press) {
            Ok(o) => self.apply_outcome(o, &format!("Dial {} press", index + 1)),
            Err(e) => self.status = e.to_string(),
        }
    }

    fn fire_encoder_twist(&mut self, index: usize, delta: i8) {
        let Some(profile) = self.active_profile() else {
            return;
        };
        let Some(enc) = profile.encoders.get(index) else {
            return;
        };
        let action = if delta >= 0 {
            &enc.twist_cw
        } else {
            &enc.twist_ccw
        };
        // Fire once per event; large deltas still one action (keeps volume usable)
        match ActionRunner::run(action) {
            Ok(o) => self.apply_outcome(
                o,
                &format!(
                    "Dial {} {}",
                    index + 1,
                    if delta >= 0 { "CW" } else { "CCW" }
                ),
            ),
            Err(e) => self.status = e.to_string(),
        }
    }

    fn fire_touch(&mut self, index: usize) {
        let Some(profile) = self.active_profile() else {
            return;
        };
        let Some(tp) = profile.touch_points.get(index) else {
            return;
        };
        match ActionRunner::run(&tp.action) {
            Ok(o) => self.apply_outcome(o, &format!("Touch {}", index + 1)),
            Err(e) => self.status = e.to_string(),
        }
    }

    fn press_key_index_back(&mut self) {
        let _ = self.store.update_active_profile(|profile| {
            if let Some(prev) = profile.page_stack.pop() {
                profile.current_page_id = prev;
            }
        });
        self.sync_hardware_page();
    }

    fn add_folder_page(&mut self, key_index: usize) -> Result<(), String> {
        self.store
            .update_active_profile(|profile| {
                let new_id = Uuid::new_v4().to_string();
                let mut page = Page {
                    id: new_id.clone(),
                    name: format!("Folder {}", profile.pages.len() + 1),
                    keys: vec![],
                };
                ensure_page_size(&mut page, profile.layout);
                if let Some(k) = page.keys.get_mut(0) {
                    k.title = "Back".into();
                    k.color = "#5a4030".into();
                    k.action = ActionSpec::GoBack;
                }
                let page_id = profile.current_page_id.clone();
                if let Some(cur) = profile.pages.iter_mut().find(|p| p.id == page_id) {
                    if let Some(btn) = cur.keys.get_mut(key_index) {
                        btn.title = page.name.clone();
                        btn.color = "#3d4a5c".into();
                        btn.action = ActionSpec::GoPage {
                            page_id: new_id.clone(),
                        };
                    }
                }
                profile.pages.push(page);
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn edit_key_panel(&mut self, ui: &mut egui::Ui, profile: &mut Profile, index: usize) {
        let page_id = profile.current_page_id.clone();
        let Some(page) = profile.pages.iter_mut().find(|p| p.id == page_id) else {
            ui.label("Invalid page");
            return;
        };
        let Some(button) = page.keys.get_mut(index) else {
            ui.label("Invalid key");
            return;
        };

        ui.label(format!("Key #{index}"));
        ui.label(
            RichText::new("Quick assign")
                .strong()
                .color(Color32::from_rgb(200, 180, 140)),
        );
        ui.label(
            RichText::new("Media & deck — one click")
                .small()
                .color(Color32::from_rgb(110, 118, 128)),
        );
        ui.add_space(4.0);

        let mut applied_preset: Option<&'static str> = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(6.0);
            for preset in PRESETS {
                let tip = format!("{} ({})", preset.title, preset.id.replace('_', " "));
                let resp = ui
                    .add_sized(
                        Vec2::splat(40.0),
                        egui::Button::new(
                            RichText::new(preset.icon)
                                .size(20.0)
                                .color(Color32::from_rgb(235, 232, 225)),
                        )
                        .fill(hex_to_color32(preset.color)),
                    )
                    .on_hover_text(tip);
                if resp.clicked() {
                    preset.apply_to(button);
                    applied_preset = Some(preset.title);
                }
            }
        });
        if let Some(name) = applied_preset {
            self.status = format!("Assigned {name}");
        }

        ui.add_space(8.0);
        ui.separator();
        ui.label(
            RichText::new("Customize")
                .strong()
                .color(Color32::from_rgb(200, 180, 140)),
        );
        ui.horizontal(|ui| {
            ui.label("Title");
            ui.text_edit_singleline(&mut button.title);
        });
        ui.horizontal(|ui| {
            ui.label("Color");
            ui.text_edit_singleline(&mut button.color);
        });

        let mut kind = action_kind(&button.action);
        egui::ComboBox::from_label("Action")
            .selected_text(kind_label(kind))
            .show_ui(ui, |ui| {
                for k in ActionKind::ALL {
                    ui.selectable_value(&mut kind, k, kind_label(k));
                }
            });
        if kind != action_kind(&button.action) {
            button.action = default_action(kind, &self.installed_plugins);
        }
        edit_action(ui, &mut button.action, &self.installed_plugins);

        let title = button.title.clone();
        let color = button.color.clone();
        let icon = icon_for_action(&button.action).map(|s| s.to_string());
        let preset_just_applied = applied_preset.is_some();
        if ui
            .button(format!("{} Save key", regular::FLOPPY_DISK))
            .clicked()
        {
            let _ = self.store.save_profile(profile.clone());
            let rgb = parse_hex_color(&color);
            let _ = self
                .hid
                .paint_key_color(index as u8, rgb, &title, icon.as_deref());
            self.status = "Key saved".into();
        } else {
            let _ = self.store.save_profile(profile.clone());
            if preset_just_applied {
                self.sync_hardware_page();
            }
        }

        ui.separator();
        if ui
            .button(format!(
                "{} Add folder page from this key",
                regular::FOLDER_PLUS
            ))
            .clicked()
        {
            if let Err(e) = self.add_folder_page(index) {
                self.status = e;
            } else {
                self.status = "Folder page linked".into();
                self.sync_hardware_page();
            }
        }
    }

    fn edit_encoder_panel(&mut self, ui: &mut egui::Ui, profile: &mut Profile, index: usize) {
        let Some(enc) = profile.encoders.get_mut(index) else {
            ui.label("No dial at this index");
            return;
        };
        ui.label(format!("{} Dial {}", regular::CIRCLE, index + 1));
        ui.horizontal(|ui| {
            ui.label("Title");
            ui.text_edit_singleline(&mut enc.title);
        });

        ui.separator();
        ui.label(RichText::new("Press").strong());
        edit_action_slot(ui, &mut enc.press, &self.installed_plugins, "enc_press");
        ui.separator();
        ui.label(RichText::new("Twist clockwise").strong());
        edit_action_slot(ui, &mut enc.twist_cw, &self.installed_plugins, "enc_cw");
        ui.separator();
        ui.label(RichText::new("Twist counter-clockwise").strong());
        edit_action_slot(ui, &mut enc.twist_ccw, &self.installed_plugins, "enc_ccw");

        let _ = self.store.save_profile(profile.clone());
        self.sync_hardware_page();
    }

    fn edit_touch_panel(&mut self, ui: &mut egui::Ui, profile: &mut Profile, index: usize) {
        let Some(tp) = profile.touch_points.get_mut(index) else {
            ui.label("No touch point at this index");
            return;
        };
        ui.label(format!("{} Touch {}", regular::HAND_TAP, index + 1));
        ui.horizontal(|ui| {
            ui.label("Title");
            ui.text_edit_singleline(&mut tp.title);
        });
        ui.separator();
        edit_action_slot(ui, &mut tp.action, &self.installed_plugins, "touch");
        let _ = self.store.save_profile(profile.clone());
    }
}

impl eframe::App for KeelApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.maybe_adopt_device();
        self.drain_hid();
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        apply_theme(ctx);

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("KEEL")
                        .size(22.0)
                        .color(Color32::from_rgb(232, 168, 78))
                        .strong(),
                );
                ui.label(
                    RichText::new("offline command deck")
                        .size(13.0)
                        .color(Color32::from_rgb(140, 148, 160)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(format!("{} Import plugin…", regular::PUZZLE_PIECE))
                        .clicked()
                    {
                        self.import_plugin_dialog();
                    }
                    if ui.button(format!("{} Settings", regular::GEAR)).clicked() {
                        self.show_settings = !self.show_settings;
                    }

                    let available = self.hid.available_devices();
                    let active = self.hid.active_device();
                    let prefer = self.hid.prefer_serial();
                    let selected_label = if let Some(serial) = prefer.as_ref() {
                        available
                            .iter()
                            .find(|d| &d.serial == serial)
                            .map(|d| format!("{} · {}", d.kind, short_serial(&d.serial)))
                            .or_else(|| {
                                active
                                    .as_ref()
                                    .filter(|d| &d.serial == serial)
                                    .map(|d| format!("{} · {}", d.kind, short_serial(&d.serial)))
                            })
                            .unwrap_or_else(|| format!("Waiting · {}", short_serial(serial)))
                    } else if let Some(d) = active.as_ref() {
                        format!("Auto · {} · {}", d.kind, short_serial(&d.serial))
                    } else if available.is_empty() {
                        "No HID device".into()
                    } else {
                        format!("Auto · {} available", available.len())
                    };

                    let mut pick: Option<Option<String>> = None;
                    egui::ComboBox::from_id_salt("physical_device")
                        .selected_text(format!("{} {selected_label}", regular::USB))
                        .width(260.0)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(prefer.is_none(), "Auto (first match)")
                                .clicked()
                            {
                                pick = Some(None);
                            }
                            ui.separator();
                            if available.is_empty() {
                                ui.label(
                                    RichText::new("No Stream Decks detected")
                                        .small()
                                        .color(Color32::from_rgb(120, 128, 136)),
                                );
                            }
                            for d in &available {
                                let label = format!(
                                    "{} · {} · {}",
                                    d.kind,
                                    d.layout.label(),
                                    short_serial(&d.serial)
                                );
                                let is_sel = prefer.as_deref() == Some(d.serial.as_str())
                                    || (prefer.is_none()
                                        && active
                                            .as_ref()
                                            .map(|a| a.serial == d.serial)
                                            .unwrap_or(false));
                                if ui.selectable_label(is_sel, label).clicked() {
                                    pick = Some(Some(d.serial.clone()));
                                }
                            }
                        });
                    if let Some(serial) = pick {
                        self.select_physical_device(serial);
                    }
                });
            });
        });

        egui::TopBottomPanel::bottom("bottom").show(ctx, |ui| {
            ui.label(
                RichText::new(&self.status)
                    .size(12.0)
                    .color(Color32::from_rgb(160, 168, 176)),
            );
        });

        egui::SidePanel::left("profiles")
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.heading("Profiles");
                let profiles = self.store.list_profiles();
                let active = self.store.settings().active_profile_id.clone();
                for p in &profiles {
                    let selected = active.as_deref() == Some(p.id.as_str());
                    if ui.selectable_label(selected, &p.name).clicked() {
                        let _ = self.store.set_active_profile(p.id.clone());
                        self.selection = None;
                        self.sync_hardware_page();
                        self.status = format!("Active: {}", p.name);
                    }
                }
                ui.separator();
                ui.label("New");
                ui.text_edit_singleline(&mut self.new_profile_name);
                egui::ComboBox::from_id_salt("layout")
                    .selected_text(self.new_layout.label())
                    .show_ui(ui, |ui| {
                        for layout in LayoutKind::ALL {
                            ui.selectable_value(&mut self.new_layout, layout, layout.label());
                        }
                    });
                if ui.button("Create profile").clicked() {
                    match self
                        .store
                        .create_profile(self.new_profile_name.clone(), self.new_layout)
                    {
                        Ok(p) => {
                            let _ = self.store.set_active_profile(p.id);
                            self.sync_hardware_page();
                            self.status = "Profile created".into();
                        }
                        Err(e) => self.status = e.to_string(),
                    }
                }
                if let Some(id) = &active {
                    if profiles.len() > 1 && ui.button("Delete active").clicked() {
                        let _ = self.store.delete_profile(id);
                        self.sync_hardware_page();
                    }
                }
            });

        if self.show_settings {
            egui::SidePanel::right("settings")
                .default_width(240.0)
                .show(ctx, |ui| {
                    ui.heading(format!("{} Settings", regular::GEAR));
                    let mut settings = self.store.settings();
                    ui.label(format!("{} Brightness", regular::SUN));
                    if ui
                        .add(egui::Slider::new(&mut settings.brightness, 0..=100))
                        .changed()
                    {
                        let _ = self.hid.set_brightness(settings.brightness);
                        let _ = self.store.set_settings(settings.clone());
                    }
                    ui.checkbox(&mut settings.start_hidden, "Start hidden");
                    if ui
                        .button(format!("{} Save settings", regular::FLOPPY_DISK))
                        .clicked()
                    {
                        let _ = self.store.set_settings(settings);
                        self.status = "Settings saved".into();
                    }
                    ui.separator();
                    ui.heading(format!("{} Plugins", regular::PUZZLE_PIECE));
                    ui.label(
                        RichText::new(format!(
                            "{} installed · WS :{}",
                            self.installed_plugins.len(),
                            self.plugin_host.port()
                        ))
                        .small()
                        .color(Color32::from_rgb(120, 128, 136)),
                    );
                    for p in &self.installed_plugins {
                        let name = p.manifest.name.as_deref().unwrap_or(p.id.as_str());
                        ui.label(format!("· {name}"));
                    }
                    if ui
                        .button(format!("{} Import plugin…", regular::DOWNLOAD_SIMPLE))
                        .clicked()
                    {
                        self.import_plugin_dialog();
                    }
                    ui.separator();
                    ui.label(
                        RichText::new("Fully offline. No telemetry.")
                            .small()
                            .color(Color32::from_rgb(120, 128, 136)),
                    );
                });
        }

        egui::SidePanel::right("editor")
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.heading(format!("{} Editor", regular::SQUARE));
                let Some(sel) = self.selection else {
                    ui.label("Select a key, dial, or touch pad.");
                    return;
                };
                let mut profile = match self.active_profile() {
                    Some(p) => p,
                    None => return,
                };

                match sel {
                    Selection::Key(index) => {
                        self.edit_key_panel(ui, &mut profile, index);
                    }
                    Selection::Encoder(index) => {
                        self.edit_encoder_panel(ui, &mut profile, index);
                    }
                    Selection::Touch(index) => {
                        self.edit_touch_panel(ui, &mut profile, index);
                    }
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(profile) = self.active_profile() else {
                ui.label("Create a profile to begin.");
                return;
            };
            let Some(page) = profile
                .pages
                .iter()
                .find(|p| p.id == profile.current_page_id)
            else {
                ui.label("Page missing");
                return;
            };

            ui.horizontal(|ui| {
                ui.heading(&profile.name);
                ui.label(format!("· {}", page.name));
                ui.label(
                    RichText::new(profile.layout.label()).color(Color32::from_rgb(120, 128, 136)),
                );
                if !profile.page_stack.is_empty()
                    && ui.button(format!("{} Back", regular::ARROW_LEFT)).clicked()
                {
                    self.press_key_index_back();
                }
            });

            ui.add_space(8.0);
            let mut pressed_key: Option<usize> = None;
            let mut pressed_enc: Option<usize> = None;
            let mut twist_enc: Option<(usize, i8)> = None;
            let mut pressed_touch: Option<usize> = None;
            let mut selection = self.selection;
            draw_deck(
                ui,
                &profile,
                page,
                &mut selection,
                &mut pressed_key,
                &mut pressed_enc,
                &mut twist_enc,
                &mut pressed_touch,
            );
            self.selection = selection;
            if let Some(idx) = pressed_key {
                self.press_key_index(idx);
            }
            if let Some(idx) = pressed_enc {
                self.fire_encoder_press(idx);
            }
            if let Some((idx, delta)) = twist_enc {
                self.fire_encoder_twist(idx, delta);
            }
            if let Some(idx) = pressed_touch {
                self.fire_touch(idx);
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.hid.stop();
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_deck(
    ui: &mut egui::Ui,
    profile: &Profile,
    page: &Page,
    selection: &mut Option<Selection>,
    pressed_key: &mut Option<usize>,
    pressed_enc: &mut Option<usize>,
    twist_enc: &mut Option<(usize, i8)>,
    pressed_touch: &mut Option<usize>,
) {
    let cols = profile.layout.cols() as usize;
    let rows = profile.layout.rows() as usize;
    let gap = 10.0;
    let key_size = if cols >= 8 { 56.0 } else { 76.0 };
    let fg = Color32::from_rgb(235, 232, 225);
    let fg_dim = Color32::from_rgb(180, 176, 168);

    for r in 0..rows {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(gap);
            for c in 0..cols {
                let idx = r * cols + c;
                let key = page.keys.get(idx);
                let title = key.map(|k| k.title.as_str()).unwrap_or("");
                let color = key
                    .map(|k| hex_to_color32(&k.color))
                    .unwrap_or(Color32::from_rgb(42, 46, 53));
                let icon = key.and_then(|k| icon_for_action(&k.action));
                let is_sel = *selection == Some(Selection::Key(idx));

                let (rect, response) =
                    ui.allocate_exact_size(Vec2::splat(key_size), Sense::click());
                let stroke = if is_sel {
                    Stroke::new(2.0, Color32::from_rgb(232, 168, 78))
                } else {
                    Stroke::new(1.0, Color32::from_rgb(60, 66, 74))
                };
                ui.painter().rect(
                    rect,
                    CornerRadius::same(10),
                    color,
                    stroke,
                    egui::epaint::StrokeKind::Inside,
                );

                if let Some(glyph) = icon {
                    let icon_pos = if title.is_empty() {
                        rect.center()
                    } else {
                        rect.center() - Vec2::new(0.0, 8.0)
                    };
                    ui.painter().text(
                        icon_pos,
                        egui::Align2::CENTER_CENTER,
                        glyph,
                        egui::FontId::proportional(26.0),
                        fg,
                    );
                    if !title.is_empty() {
                        ui.painter().text(
                            rect.center() + Vec2::new(0.0, 18.0),
                            egui::Align2::CENTER_CENTER,
                            title,
                            egui::FontId::proportional(11.0),
                            fg_dim,
                        );
                    }
                } else {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        if title.is_empty() { "·" } else { title },
                        egui::FontId::proportional(12.0),
                        fg,
                    );
                }

                if response.clicked() {
                    *selection = Some(Selection::Key(idx));
                    *pressed_key = Some(idx);
                }
                if response.secondary_clicked() {
                    *selection = Some(Selection::Key(idx));
                }
            }
        });
        ui.add_space(gap);
    }

    // Dials (Plus / Plus XL)
    if !profile.encoders.is_empty() {
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!("{} Dials", regular::CIRCLE))
                .strong()
                .color(Color32::from_rgb(200, 180, 140)),
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(12.0);
            for (i, enc) in profile.encoders.iter().enumerate() {
                ui.vertical(|ui| {
                    let is_sel = *selection == Some(Selection::Encoder(i));
                    let size = Vec2::splat(52.0);
                    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
                    let stroke = if is_sel {
                        Stroke::new(2.0, Color32::from_rgb(232, 168, 78))
                    } else {
                        Stroke::new(1.5, Color32::from_rgb(80, 88, 100))
                    };
                    ui.painter()
                        .circle(rect.center(), 24.0, Color32::from_rgb(40, 44, 52), stroke);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        regular::CIRCLE,
                        egui::FontId::proportional(18.0),
                        fg,
                    );
                    ui.label(RichText::new(&enc.title).small().color(fg_dim));
                    ui.horizontal(|ui| {
                        if ui.small_button("−").clicked() {
                            *selection = Some(Selection::Encoder(i));
                            *twist_enc = Some((i, -1));
                        }
                        if ui.small_button("+").clicked() {
                            *selection = Some(Selection::Encoder(i));
                            *twist_enc = Some((i, 1));
                        }
                    });
                    if response.clicked() {
                        *selection = Some(Selection::Encoder(i));
                        *pressed_enc = Some(i);
                    }
                    if response.secondary_clicked() {
                        *selection = Some(Selection::Encoder(i));
                    }
                });
            }
        });
    }

    // Neo touch points
    if !profile.touch_points.is_empty() {
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!("{} Touch", regular::HAND_TAP))
                .strong()
                .color(Color32::from_rgb(200, 180, 140)),
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(10.0);
            for (i, tp) in profile.touch_points.iter().enumerate() {
                let is_sel = *selection == Some(Selection::Touch(i));
                let label = format!("{} {}", regular::HAND_TAP, tp.title);
                let resp = ui.add(
                    egui::Button::new(RichText::new(label).color(fg))
                        .fill(if is_sel {
                            Color32::from_rgb(70, 56, 32)
                        } else {
                            Color32::from_rgb(36, 40, 46)
                        })
                        .min_size(Vec2::new(100.0, 36.0)),
                );
                if resp.clicked() {
                    *selection = Some(Selection::Touch(i));
                    *pressed_touch = Some(i);
                }
                if resp.secondary_clicked() {
                    *selection = Some(Selection::Touch(i));
                }
            }
        });
    }

    ui.label(
        RichText::new("Click: run · Right-click: select for edit")
            .small()
            .color(Color32::from_rgb(110, 118, 128)),
    );
}

fn edit_action_slot(
    ui: &mut egui::Ui,
    action: &mut ActionSpec,
    plugins: &[InstalledPlugin],
    id_salt: &str,
) {
    let mut kind = action_kind(action);
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(kind_label(kind))
        .show_ui(ui, |ui| {
            for k in ActionKind::ALL {
                ui.selectable_value(&mut kind, k, kind_label(k));
            }
        });
    if kind != action_kind(action) {
        *action = default_action(kind, plugins);
    }
    edit_action(ui, action, plugins);
}

fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals.dark_mode = true;
    style.visuals.panel_fill = Color32::from_rgb(18, 20, 24);
    style.visuals.window_fill = Color32::from_rgb(22, 24, 28);
    style.visuals.override_text_color = Some(Color32::from_rgb(220, 222, 226));
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(36, 40, 46);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 52, 60);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(70, 56, 32);
    style.visuals.selection.bg_fill = Color32::from_rgb(90, 64, 28);
    ctx.set_style(style);
}

fn hex_to_color32(s: &str) -> Color32 {
    let (r, g, b) = parse_hex_color(s);
    Color32::from_rgb(r, g, b)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActionKind {
    Empty,
    Hotkey,
    OpenUrl,
    OpenApp,
    RunCommand,
    Media,
    Text,
    Brightness,
    GoPage,
    GoBack,
    Plugin,
    AudioRpc,
    LightRpc,
}

impl ActionKind {
    const ALL: [ActionKind; 13] = [
        ActionKind::Empty,
        ActionKind::Hotkey,
        ActionKind::OpenUrl,
        ActionKind::OpenApp,
        ActionKind::RunCommand,
        ActionKind::Media,
        ActionKind::Text,
        ActionKind::Brightness,
        ActionKind::GoPage,
        ActionKind::GoBack,
        ActionKind::Plugin,
        ActionKind::AudioRpc,
        ActionKind::LightRpc,
    ];
}

fn kind_label(k: ActionKind) -> &'static str {
    match k {
        ActionKind::Empty => "Empty",
        ActionKind::Hotkey => "Hotkey",
        ActionKind::OpenUrl => "Open URL",
        ActionKind::OpenApp => "Open app",
        ActionKind::RunCommand => "Run command",
        ActionKind::Media => "Media",
        ActionKind::Text => "Type text",
        ActionKind::Brightness => "Brightness",
        ActionKind::GoPage => "Go to page",
        ActionKind::GoBack => "Go back",
        ActionKind::Plugin => "Plugin action",
        ActionKind::AudioRpc => "Keel Audio RPC",
        ActionKind::LightRpc => "Keel Light RPC",
    }
}

fn action_kind(a: &ActionSpec) -> ActionKind {
    match a {
        ActionSpec::Empty => ActionKind::Empty,
        ActionSpec::Hotkey { .. } => ActionKind::Hotkey,
        ActionSpec::OpenUrl { .. } => ActionKind::OpenUrl,
        ActionSpec::OpenApp { .. } => ActionKind::OpenApp,
        ActionSpec::RunCommand { .. } => ActionKind::RunCommand,
        ActionSpec::Media { .. } => ActionKind::Media,
        ActionSpec::Text { .. } => ActionKind::Text,
        ActionSpec::Brightness { .. } => ActionKind::Brightness,
        ActionSpec::GoPage { .. } => ActionKind::GoPage,
        ActionSpec::GoBack => ActionKind::GoBack,
        ActionSpec::PluginAction { .. } => ActionKind::Plugin,
        ActionSpec::AudioRpc { .. } => ActionKind::AudioRpc,
        ActionSpec::LightRpc { .. } => ActionKind::LightRpc,
    }
}

fn default_action(k: ActionKind, plugins: &[InstalledPlugin]) -> ActionSpec {
    match k {
        ActionKind::Empty => ActionSpec::Empty,
        ActionKind::Hotkey => ActionSpec::Hotkey {
            cmd: true,
            ctrl: false,
            alt: false,
            shift: false,
            key: "c".into(),
        },
        ActionKind::OpenUrl => ActionSpec::OpenUrl {
            url: "https://example.local".into(),
        },
        ActionKind::OpenApp => ActionSpec::OpenApp {
            path: String::new(),
        },
        ActionKind::RunCommand => ActionSpec::RunCommand {
            program: String::new(),
            args: vec![],
        },
        ActionKind::Media => ActionSpec::Media {
            command: MediaCommand::PlayPause,
        },
        ActionKind::Text => ActionSpec::Text {
            text: String::new(),
        },
        ActionKind::Brightness => ActionSpec::Brightness { percent: 70 },
        ActionKind::GoPage => ActionSpec::GoPage {
            page_id: String::new(),
        },
        ActionKind::GoBack => ActionSpec::GoBack,
        ActionKind::Plugin => {
            let (plugin_id, action_uuid) = first_plugin_action(plugins);
            ActionSpec::PluginAction {
                plugin_id,
                action_uuid,
                settings: serde_json::json!({}),
            }
        }
        ActionKind::AudioRpc => ActionSpec::AudioRpc {
            port: keel_rpc::DEFAULT_PORT,
            method: "getMixerState".into(),
            params: serde_json::json!({}),
        },
        ActionKind::LightRpc => ActionSpec::LightRpc {
            port: keel_rpc::LIGHT_PORT,
            method: "getLights".into(),
            params: serde_json::json!({}),
        },
    }
}

fn first_plugin_action(plugins: &[InstalledPlugin]) -> (String, String) {
    for p in plugins {
        if let Some(a) = p.manifest.actions.first() {
            return (p.id.clone(), a.uuid.clone());
        }
    }
    (String::new(), String::new())
}

fn edit_action(ui: &mut egui::Ui, action: &mut ActionSpec, plugins: &[InstalledPlugin]) {
    match action {
        ActionSpec::Empty | ActionSpec::GoBack => {}
        ActionSpec::Hotkey {
            cmd,
            ctrl,
            alt,
            shift,
            key,
        } => {
            ui.checkbox(cmd, "Cmd/Meta");
            ui.checkbox(ctrl, "Ctrl");
            ui.checkbox(alt, "Alt");
            ui.checkbox(shift, "Shift");
            ui.horizontal(|ui| {
                ui.label("Key");
                ui.text_edit_singleline(key);
            });
        }
        ActionSpec::OpenUrl { url } => {
            ui.text_edit_singleline(url);
        }
        ActionSpec::OpenApp { path } => {
            ui.text_edit_singleline(path);
        }
        ActionSpec::RunCommand { program, args } => {
            ui.horizontal(|ui| {
                ui.label("Program");
                ui.text_edit_singleline(program);
            });
            let mut joined = args.join(" ");
            if ui.text_edit_singleline(&mut joined).changed() {
                *args = joined.split_whitespace().map(|s| s.to_string()).collect();
            }
            ui.label(RichText::new("Args space-separated; no shell").small());
        }
        ActionSpec::Media { command } => {
            egui::ComboBox::from_label("Media")
                .selected_text(format!("{command:?}"))
                .show_ui(ui, |ui| {
                    for c in [
                        MediaCommand::PlayPause,
                        MediaCommand::Next,
                        MediaCommand::Previous,
                        MediaCommand::VolumeUp,
                        MediaCommand::VolumeDown,
                        MediaCommand::Mute,
                    ] {
                        ui.selectable_value(command, c, format!("{c:?}"));
                    }
                });
        }
        ActionSpec::Text { text } => {
            ui.text_edit_multiline(text);
        }
        ActionSpec::Brightness { percent } => {
            ui.add(egui::Slider::new(percent, 0..=100));
        }
        ActionSpec::GoPage { page_id } => {
            ui.horizontal(|ui| {
                ui.label("Page id");
                ui.text_edit_singleline(page_id);
            });
        }
        ActionSpec::AudioRpc {
            port,
            method,
            params,
        } => {
            edit_suite_rpc(
                ui,
                port,
                method,
                params,
                "Local keel-audio JSON-RPC (127.0.0.1 + Bearer token)",
                &[
                    ("getMixerState", serde_json::json!({})),
                    ("getOutputDevices", serde_json::json!({})),
                    (
                        "setChannelMute",
                        serde_json::json!({"channelId": "", "mute": true}),
                    ),
                    (
                        "setChannelVolume",
                        serde_json::json!({"channelId": "", "volume": 0.8}),
                    ),
                ],
            );
        }
        ActionSpec::LightRpc {
            port,
            method,
            params,
        } => {
            edit_suite_rpc(
                ui,
                port,
                method,
                params,
                "Local keel-light JSON-RPC (127.0.0.1 + Bearer token)",
                &[
                    ("getLights", serde_json::json!({})),
                    ("refreshLights", serde_json::json!({})),
                    ("setLightPower", serde_json::json!({"id": "", "on": true})),
                    (
                        "setLightBrightness",
                        serde_json::json!({"id": "", "brightness": 50}),
                    ),
                    (
                        "setLightTemperature",
                        serde_json::json!({"id": "", "temperature": 210}),
                    ),
                ],
            );
        }
        ActionSpec::PluginAction {
            plugin_id,
            action_uuid,
            ..
        } => {
            if plugins.is_empty() {
                ui.label("No plugins installed — use Import plugin…");
                return;
            }
            let mut choices: Vec<(String, String, String)> = Vec::new();
            for p in plugins {
                let pname = p.manifest.name.clone().unwrap_or_else(|| p.id.clone());
                for a in &p.manifest.actions {
                    let aname = a.name.clone().unwrap_or_else(|| a.uuid.clone());
                    choices.push((p.id.clone(), a.uuid.clone(), format!("{pname} / {aname}")));
                }
            }
            let current_label = choices
                .iter()
                .find(|(pid, aid, _)| pid == plugin_id && aid == action_uuid)
                .map(|(_, _, l)| l.as_str())
                .unwrap_or("(select)");
            egui::ComboBox::from_label("Plugin action")
                .selected_text(current_label)
                .show_ui(ui, |ui| {
                    for (pid, aid, label) in &choices {
                        if ui
                            .selectable_label(pid == plugin_id && aid == action_uuid, label)
                            .clicked()
                        {
                            *plugin_id = pid.clone();
                            *action_uuid = aid.clone();
                        }
                    }
                });
        }
    }
}

fn edit_suite_rpc(
    ui: &mut egui::Ui,
    port: &mut u16,
    method: &mut String,
    params: &mut serde_json::Value,
    hint: &str,
    presets: &[(&str, serde_json::Value)],
) {
    ui.add(egui::DragValue::new(port).range(1..=65535).prefix("Port "));
    egui::ComboBox::from_label("Preset")
        .selected_text(method.as_str())
        .show_ui(ui, |ui| {
            for (name, preset_params) in presets {
                if ui.selectable_label(method == *name, *name).clicked() {
                    *method = (*name).to_string();
                    *params = preset_params.clone();
                }
            }
        });
    ui.horizontal(|ui| {
        ui.label("Method");
        ui.text_edit_singleline(method);
    });
    let mut raw = params.to_string();
    if ui.text_edit_multiline(&mut raw).changed() {
        if let Ok(v) = serde_json::from_str(&raw) {
            *params = v;
        }
    }
    ui.label(RichText::new(hint).small());
}

fn index_to_coords(index: usize, profile: &Option<Profile>) -> (u8, u8) {
    let cols = profile
        .as_ref()
        .map(|p| p.layout.cols() as usize)
        .unwrap_or(5);
    let row = (index / cols) as u8;
    let col = (index % cols) as u8;
    (col, row)
}

fn short_serial(serial: &str) -> String {
    if serial.len() <= 8 {
        serial.to_string()
    } else {
        format!("…{}", &serial[serial.len().saturating_sub(6)..])
    }
}
