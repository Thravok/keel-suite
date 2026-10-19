//! Elgato Stream Deck HID manager (VID 0x0FD9 via elgato-streamdeck).
//!
//! The device handle is not Sync, so it lives on a dedicated worker thread.
//! UI / action code talks to it through channels.

use crate::layout::{kind_display_name, kind_to_layout};
use crate::model::{DeviceInfo, LayoutKind};
use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use elgato_streamdeck::images::convert_image_with_format;
use elgato_streamdeck::info::Kind;
use elgato_streamdeck::{list_devices, new_hidapi, DeviceStateUpdate, StreamDeck};
use image::{DynamicImage, ImageBuffer, Rgba};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub type KeyPressCallback = Arc<dyn Fn(u8) + Send + Sync + 'static>;
pub type EncoderCallback = Arc<dyn Fn(EncoderEvent) + Send + Sync + 'static>;
pub type TouchPointCallback = Arc<dyn Fn(u8) + Send + Sync + 'static>;

#[derive(Debug, Clone, Copy)]
pub enum EncoderEvent {
    Down(u8),
    Up(u8),
    Twist { index: u8, delta: i8 },
}

enum HidCmd {
    SetBrightness(u8),
    PaintKey {
        key: u8,
        rgb: (u8, u8, u8),
        title: String,
        icon: Option<String>,
    },
    SyncPage {
        colors: Vec<(u8, u8, u8)>,
        titles: Vec<String>,
        icons: Vec<Option<String>>,
        encoder_titles: Vec<String>,
    },
    PreferLayout(Option<LayoutKind>),
    PreferSerial(Option<String>),
    Stop,
}

pub struct HidDeviceManager {
    running: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
    /// Currently open device (0 or 1 entry).
    connected: Arc<Mutex<Vec<DeviceInfo>>>,
    /// All enumerated Elgato decks (updated on each scan).
    available: Arc<Mutex<Vec<DeviceInfo>>>,
    prefer_layout: Arc<Mutex<Option<LayoutKind>>>,
    prefer_serial: Arc<Mutex<Option<String>>>,
    cmd_tx: Mutex<Option<Sender<HidCmd>>>,
    on_press: Arc<Mutex<Option<KeyPressCallback>>>,
    on_encoder: Arc<Mutex<Option<EncoderCallback>>>,
    on_touch: Arc<Mutex<Option<TouchPointCallback>>>,
}

impl HidDeviceManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            running: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
            connected: Arc::new(Mutex::new(Vec::new())),
            available: Arc::new(Mutex::new(Vec::new())),
            prefer_layout: Arc::new(Mutex::new(None)),
            prefer_serial: Arc::new(Mutex::new(None)),
            cmd_tx: Mutex::new(None),
            on_press: Arc::new(Mutex::new(None)),
            on_encoder: Arc::new(Mutex::new(None)),
            on_touch: Arc::new(Mutex::new(None)),
        })
    }

    pub fn set_on_press(&self, cb: KeyPressCallback) {
        *self.on_press.lock() = Some(cb);
    }

    pub fn set_on_encoder(&self, cb: EncoderCallback) {
        *self.on_encoder.lock() = Some(cb);
    }

    pub fn set_on_touch_point(&self, cb: TouchPointCallback) {
        *self.on_touch.lock() = Some(cb);
    }

    pub fn set_prefer_layout(&self, layout: Option<LayoutKind>) {
        *self.prefer_layout.lock() = layout;
        self.send(HidCmd::PreferLayout(layout));
    }

    /// Pin HID to a specific serial (takes priority over layout preference).
    pub fn set_prefer_serial(&self, serial: Option<String>) {
        *self.prefer_serial.lock() = serial.clone();
        self.send(HidCmd::PreferSerial(serial));
    }

    pub fn prefer_serial(&self) -> Option<String> {
        self.prefer_serial.lock().clone()
    }

    /// Devices currently open for I/O (usually 0–1).
    pub fn devices(&self) -> Vec<DeviceInfo> {
        self.connected.lock().clone()
    }

    /// All enumerated Stream Decks (for the device picker).
    pub fn available_devices(&self) -> Vec<DeviceInfo> {
        self.available.lock().clone()
    }

    pub fn active_device(&self) -> Option<DeviceInfo> {
        self.connected.lock().first().cloned()
    }

    pub fn start(self: &Arc<Self>) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let (tx, rx) = mpsc::channel::<HidCmd>();
        *self.cmd_tx.lock() = Some(tx);

        let running = Arc::clone(&self.running);
        let connected = Arc::clone(&self.connected);
        let available = Arc::clone(&self.available);
        let prefer_layout = Arc::clone(&self.prefer_layout);
        let prefer_serial = Arc::clone(&self.prefer_serial);
        let on_press = Arc::clone(&self.on_press);
        let on_encoder = Arc::clone(&self.on_encoder);
        let on_touch = Arc::clone(&self.on_touch);

        let handle = thread::spawn(move || {
            worker_loop(
                running,
                connected,
                available,
                prefer_layout,
                prefer_serial,
                on_press,
                on_encoder,
                on_touch,
                rx,
            )
        });
        *self.worker.lock() = Some(handle);
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(tx) = self.cmd_tx.lock().take() {
            let _ = tx.send(HidCmd::Stop);
        }
        if let Some(h) = self.worker.lock().take() {
            let _ = h.join();
        }
        self.connected.lock().clear();
        self.available.lock().clear();
    }

    fn send(&self, cmd: HidCmd) {
        if let Some(tx) = self.cmd_tx.lock().as_ref() {
            let _ = tx.send(cmd);
        }
    }

    pub fn set_brightness(&self, percent: u8) -> Result<(), String> {
        self.send(HidCmd::SetBrightness(percent.min(100)));
        Ok(())
    }

    pub fn paint_key_color(
        &self,
        key: u8,
        rgb: (u8, u8, u8),
        title: &str,
        icon: Option<&str>,
    ) -> Result<(), String> {
        self.send(HidCmd::PaintKey {
            key,
            rgb,
            title: title.to_string(),
            icon: icon.map(|s| s.to_string()),
        });
        Ok(())
    }

    pub fn sync_page_colors(
        &self,
        colors: &[(u8, u8, u8)],
        titles: &[String],
        icons: &[Option<String>],
        encoder_titles: &[String],
    ) -> Result<(), String> {
        self.send(HidCmd::SyncPage {
            colors: colors.to_vec(),
            titles: titles.to_vec(),
            icons: icons.to_vec(),
            encoder_titles: encoder_titles.to_vec(),
        });
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn worker_loop(
    running: Arc<AtomicBool>,
    connected: Arc<Mutex<Vec<DeviceInfo>>>,
    available: Arc<Mutex<Vec<DeviceInfo>>>,
    prefer_layout: Arc<Mutex<Option<LayoutKind>>>,
    prefer_serial: Arc<Mutex<Option<String>>>,
    on_press: Arc<Mutex<Option<KeyPressCallback>>>,
    on_encoder: Arc<Mutex<Option<EncoderCallback>>>,
    on_touch: Arc<Mutex<Option<TouchPointCallback>>>,
    rx: Receiver<HidCmd>,
) {
    let mut deck: Option<Arc<StreamDeck>> = None;
    let mut prefer = *prefer_layout.lock();
    let mut serial_pref = prefer_serial.lock().clone();

    while running.load(Ordering::SeqCst) {
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                HidCmd::Stop => {
                    connected.lock().clear();
                    available.lock().clear();
                    return;
                }
                HidCmd::PreferLayout(l) => {
                    prefer = l;
                    *prefer_layout.lock() = l;
                    // Only reconnect on layout mismatch when no serial pin is set.
                    if serial_pref.is_none() {
                        if let Some(d) = deck.as_ref() {
                            let cur = kind_to_layout(d.kind());
                            if let Some(want) = prefer {
                                if cur != want {
                                    deck = None;
                                    connected.lock().clear();
                                }
                            }
                        }
                    }
                }
                HidCmd::PreferSerial(s) => {
                    let changed = serial_pref != s;
                    serial_pref = s.clone();
                    *prefer_serial.lock() = s;
                    if changed {
                        deck = None;
                        connected.lock().clear();
                    }
                }
                HidCmd::SetBrightness(p) => {
                    if let Some(d) = deck.as_ref() {
                        if d.kind().is_visual() {
                            let _ = d.set_brightness(p);
                        }
                    }
                }
                HidCmd::PaintKey {
                    key,
                    rgb,
                    title,
                    icon,
                } => {
                    if let Some(d) = deck.as_ref() {
                        if d.kind().is_visual() {
                            let (w, h) = d.kind().key_image_format().size;
                            let img = render_key_image(w, h, rgb, &title, icon.as_deref());
                            let _ = d.set_button_image(key, img);
                            let _ = d.flush();
                        }
                    }
                }
                HidCmd::SyncPage {
                    colors,
                    titles,
                    icons,
                    encoder_titles,
                } => {
                    if let Some(d) = deck.as_ref() {
                        if d.kind().is_visual() {
                            let n = d.kind().key_count() as usize;
                            for i in 0..n {
                                let rgb = colors.get(i).copied().unwrap_or((42, 46, 53));
                                let title = titles.get(i).map(|s| s.as_str()).unwrap_or("");
                                let icon = icons.get(i).and_then(|o| o.as_deref());
                                let (w, h) = d.kind().key_image_format().size;
                                let img = render_key_image(w, h, rgb, title, icon);
                                let _ = d.set_button_image(i as u8, img);
                            }
                            let _ = d.flush();
                            paint_lcd_strip(d, &encoder_titles);
                        }
                    }
                }
            }
        }

        if deck.is_none() {
            match try_connect(prefer, serial_pref.as_deref(), &available) {
                Ok(Some((d, info))) => {
                    *connected.lock() = vec![info];
                    deck = Some(d);
                }
                Ok(None) => {
                    connected.lock().clear();
                    thread::sleep(Duration::from_millis(800));
                    continue;
                }
                Err(_) => {
                    connected.lock().clear();
                    thread::sleep(Duration::from_millis(1200));
                    continue;
                }
            }
        }

        if let Some(d) = deck.as_ref() {
            // Refresh enumeration so the device picker stays current.
            refresh_available(&available);

            // If a serial pin no longer matches the open device, drop and reconnect.
            if let Some(want) = serial_pref.as_ref() {
                let cur = connected.lock().first().map(|i| i.serial.clone());
                if cur.as_ref() != Some(want) {
                    deck = None;
                    connected.lock().clear();
                    continue;
                }
            }

            let reader = d.get_reader();
            match reader.read(Some(Duration::from_millis(200))) {
                Ok(updates) => {
                    for u in updates {
                        match u {
                            DeviceStateUpdate::ButtonDown(key) => {
                                if let Some(cb) = on_press.lock().as_ref() {
                                    cb(key);
                                }
                            }
                            DeviceStateUpdate::EncoderDown(i) => {
                                if let Some(cb) = on_encoder.lock().as_ref() {
                                    cb(EncoderEvent::Down(i));
                                }
                            }
                            DeviceStateUpdate::EncoderUp(i) => {
                                if let Some(cb) = on_encoder.lock().as_ref() {
                                    cb(EncoderEvent::Up(i));
                                }
                            }
                            DeviceStateUpdate::EncoderTwist(i, delta) => {
                                if let Some(cb) = on_encoder.lock().as_ref() {
                                    cb(EncoderEvent::Twist { index: i, delta });
                                }
                            }
                            DeviceStateUpdate::TouchPointDown(i) => {
                                if let Some(cb) = on_touch.lock().as_ref() {
                                    cb(i);
                                }
                            }
                            DeviceStateUpdate::ButtonUp(_)
                            | DeviceStateUpdate::TouchPointUp(_)
                            | DeviceStateUpdate::TouchScreenPress(_, _)
                            | DeviceStateUpdate::TouchScreenLongPress(_, _)
                            | DeviceStateUpdate::TouchScreenSwipe(_, _) => {}
                        }
                    }
                }
                Err(_) => {
                    deck = None;
                    connected.lock().clear();
                    thread::sleep(Duration::from_millis(500));
                }
            }
        }
    }

    connected.lock().clear();
}

fn paint_lcd_strip(deck: &StreamDeck, encoder_titles: &[String]) {
    let Some(fmt) = deck.kind().lcd_image_format() else {
        return;
    };
    let (w, h) = fmt.size;
    let n = deck.kind().encoder_count().max(1) as usize;
    let seg_w = (w / n).max(1);

    let buf: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(w as u32, h as u32, |x, y| {
        let seg = (x as usize / seg_w).min(n.saturating_sub(1));
        let shade = 36 + (seg as u8).saturating_mul(18);
        // Light divider between dial segments
        if (x as usize).is_multiple_of(seg_w) && x > 0 {
            return Rgba([90, 90, 100, 255]);
        }
        // Top bar accent
        if y < 4 {
            return Rgba([232, 168, 78, 255]);
        }
        let _ = encoder_titles.get(seg);
        Rgba([
            shade,
            shade.saturating_add(4),
            shade.saturating_add(10),
            255,
        ])
    });
    let img = DynamicImage::ImageRgba8(buf);
    if let Ok(data) = convert_image_with_format(fmt, img) {
        let _ = deck.write_lcd_fill(&data);
    }
}

fn refresh_available(available_out: &Mutex<Vec<DeviceInfo>>) {
    let Ok(hid) = new_hidapi() else {
        return;
    };
    let catalog: Vec<DeviceInfo> = list_devices(&hid)
        .into_iter()
        .map(|(kind, serial)| device_info_from_kind(kind, &serial))
        .collect();
    *available_out.lock() = catalog;
}

fn device_info_from_kind(kind: Kind, serial: &str) -> DeviceInfo {
    let layout = kind_to_layout(kind);
    DeviceInfo {
        kind: kind_display_name(kind).to_string(),
        serial: serial.to_string(),
        layout,
        rows: kind.row_count(),
        cols: kind.column_count(),
        key_count: kind.key_count(),
        encoder_count: kind.encoder_count(),
        touch_point_count: kind.touchpoint_count(),
        has_lcd: kind.lcd_strip_size().is_some(),
        is_visual: kind.is_visual(),
    }
}

fn try_connect(
    prefer: Option<LayoutKind>,
    prefer_serial: Option<&str>,
    available_out: &Mutex<Vec<DeviceInfo>>,
) -> Result<Option<(Arc<StreamDeck>, DeviceInfo)>, String> {
    let hid = new_hidapi().map_err(|e| e.to_string())?;
    let devices = list_devices(&hid);

    let catalog: Vec<DeviceInfo> = devices
        .iter()
        .map(|(kind, serial)| device_info_from_kind(*kind, serial))
        .collect();
    *available_out.lock() = catalog;

    if devices.is_empty() {
        return Ok(None);
    }

    // Serial pin wins; then layout preference; else first device.
    let pick = if let Some(want_serial) = prefer_serial {
        devices
            .iter()
            .find(|(_, serial)| serial == want_serial)
            .cloned()
            .or_else(|| devices.first().cloned())
    } else if let Some(want) = prefer {
        devices
            .iter()
            .find(|(kind, _)| kind_to_layout(*kind) == want)
            .cloned()
            .or_else(|| devices.first().cloned())
    } else {
        devices.first().cloned()
    };

    let Some((kind, serial)) = pick else {
        return Ok(None);
    };

    let deck = StreamDeck::connect(&hid, kind, &serial).map_err(|e| format!("{e:?}"))?;
    #[allow(clippy::arc_with_non_send_sync)] // StreamDeck is !Sync; Arc shared only on this worker
    let deck = Arc::new(deck);
    let _ = deck.reset();
    let info = device_info_from_kind(kind, &serial);
    Ok(Some((deck, info)))
}

fn render_key_image(
    w: usize,
    h: usize,
    rgb: (u8, u8, u8),
    title: &str,
    icon: Option<&str>,
) -> DynamicImage {
    let (r, g, b) = rgb;
    let mut buf: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_fn(w as u32, h as u32, |_x, _y| Rgba([r, g, b, 255]));

    let fg = Rgba([245u8, 242, 235, 255]);
    let has_icon = icon.map(|s| !s.is_empty()).unwrap_or(false);
    let has_title = !title.trim().is_empty();

    if has_icon {
        if let Some(glyph) = icon {
            let icon_scale = (h as f32 * 0.42).clamp(22.0, 48.0);
            let y = if has_title {
                h as f32 * 0.28
            } else {
                h as f32 * 0.50
            };
            draw_centered_text(&mut buf, glyph, icon_scale, y, fg, FontKind::Phosphor);
        }
    }

    if has_title {
        let title_scale = if has_icon {
            (h as f32 * 0.16).clamp(10.0, 18.0)
        } else {
            (h as f32 * 0.22).clamp(12.0, 24.0)
        };
        let y = if has_icon {
            h as f32 * 0.78
        } else {
            h as f32 * 0.50
        };
        // Trim long titles so they fit a key face
        let label = truncate_title(title.trim(), 10);
        draw_centered_text(&mut buf, &label, title_scale, y, fg, FontKind::Latin);
    }

    DynamicImage::ImageRgba8(buf)
}

enum FontKind {
    Latin,
    Phosphor,
}

fn truncate_title(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn draw_centered_text(
    buf: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    text: &str,
    px: f32,
    center_y: f32,
    color: Rgba<u8>,
    kind: FontKind,
) {
    let font_bytes: &'static [u8] = match kind {
        FontKind::Latin => epaint_default_fonts::HACK_REGULAR,
        FontKind::Phosphor => egui_phosphor::Variant::Regular.font_bytes(),
    };
    let Ok(font) = FontRef::try_from_slice(font_bytes) else {
        return;
    };
    let scale = PxScale::from(px);
    let scaled = font.as_scaled(scale);

    let mut width = 0.0f32;
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        width += scaled.h_advance(gid);
    }

    let w = buf.width() as f32;
    let mut x = ((w - width) * 0.5).max(1.0);
    let baseline = center_y + scaled.ascent() * 0.35;

    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        let glyph = gid.with_scale_and_position(scale, ab_glyph::point(x, baseline));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, cover| {
                let px_x = bounds.min.x as i32 + gx as i32;
                let px_y = bounds.min.y as i32 + gy as i32;
                if px_x < 0 || px_y < 0 {
                    return;
                }
                let px_x = px_x as u32;
                let px_y = px_y as u32;
                if px_x >= buf.width() || px_y >= buf.height() {
                    return;
                }
                let cover = cover.clamp(0.0, 1.0);
                if cover < 0.05 {
                    return;
                }
                let dst = buf.get_pixel_mut(px_x, px_y);
                let a = cover;
                dst.0[0] = ((color.0[0] as f32) * a + (dst.0[0] as f32) * (1.0 - a)) as u8;
                dst.0[1] = ((color.0[1] as f32) * a + (dst.0[1] as f32) * (1.0 - a)) as u8;
                dst.0[2] = ((color.0[2] as f32) * a + (dst.0[2] as f32) * (1.0 - a)) as u8;
                dst.0[3] = 255;
            });
        }
        x += scaled.h_advance(gid);
    }
}

pub fn parse_hex_color(s: &str) -> (u8, u8, u8) {
    let s = s.trim().trim_start_matches('#');
    if s.len() >= 6 {
        let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(42);
        let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(46);
        let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(53);
        (r, g, b)
    } else {
        (42, 46, 53)
    }
}
