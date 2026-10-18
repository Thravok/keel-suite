//! Elgato Stream Deck HID manager (VID 0x0FD9 via elgato-streamdeck).
//!
//! The device handle is not Sync, so it lives on a dedicated worker thread.
//! UI / action code talks to it through channels.

use crate::model::DeviceInfo;
use elgato_streamdeck::{list_devices, new_hidapi, DeviceStateUpdate, StreamDeck};
use image::{DynamicImage, ImageBuffer, Rgba};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub type KeyPressCallback = Arc<dyn Fn(u8) + Send + Sync + 'static>;

enum HidCmd {
    SetBrightness(u8),
    PaintKey {
        key: u8,
        rgb: (u8, u8, u8),
        title: String,
    },
    SyncPage {
        colors: Vec<(u8, u8, u8)>,
        titles: Vec<String>,
    },
    Stop,
}

pub struct HidDeviceManager {
    running: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
    connected: Arc<Mutex<Vec<DeviceInfo>>>,
    cmd_tx: Mutex<Option<Sender<HidCmd>>>,
    on_press: Arc<Mutex<Option<KeyPressCallback>>>,
}

impl HidDeviceManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            running: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
            connected: Arc::new(Mutex::new(Vec::new())),
            cmd_tx: Mutex::new(None),
            on_press: Arc::new(Mutex::new(None)),
        })
    }

    pub fn set_on_press(&self, cb: KeyPressCallback) {
        *self.on_press.lock() = Some(cb);
    }

    pub fn devices(&self) -> Vec<DeviceInfo> {
        self.connected.lock().clone()
    }

    pub fn start(self: &Arc<Self>) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let (tx, rx) = mpsc::channel::<HidCmd>();
        *self.cmd_tx.lock() = Some(tx);

        let running = Arc::clone(&self.running);
        let connected = Arc::clone(&self.connected);
        let on_press = Arc::clone(&self.on_press);

        let handle = thread::spawn(move || worker_loop(running, connected, on_press, rx));
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

    pub fn paint_key_color(&self, key: u8, rgb: (u8, u8, u8), title: &str) -> Result<(), String> {
        self.send(HidCmd::PaintKey {
            key,
            rgb,
            title: title.to_string(),
        });
        Ok(())
    }

    pub fn sync_page_colors(
        &self,
        colors: &[(u8, u8, u8)],
        titles: &[String],
    ) -> Result<(), String> {
        self.send(HidCmd::SyncPage {
            colors: colors.to_vec(),
            titles: titles.to_vec(),
        });
        Ok(())
    }
}

fn worker_loop(
    running: Arc<AtomicBool>,
    connected: Arc<Mutex<Vec<DeviceInfo>>>,
    on_press: Arc<Mutex<Option<KeyPressCallback>>>,
    rx: Receiver<HidCmd>,
) {
    let mut deck: Option<Arc<StreamDeck>> = None;

    while running.load(Ordering::SeqCst) {
        // Process queued commands
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                HidCmd::Stop => {
                    connected.lock().clear();
                    return;
                }
                HidCmd::SetBrightness(p) => {
                    if let Some(d) = deck.as_ref() {
                        let _ = d.set_brightness(p);
                    }
                }
                HidCmd::PaintKey { key, rgb, title } => {
                    if let Some(d) = deck.as_ref() {
                        let (w, h) = d.kind().key_image_format().size;
                        let img = render_key_image(w, h, rgb, &title);
                        let _ = d.set_button_image(key, img);
                        let _ = d.flush();
                    }
                }
                HidCmd::SyncPage { colors, titles } => {
                    if let Some(d) = deck.as_ref() {
                        let n = d.kind().key_count() as usize;
                        for i in 0..n {
                            let rgb = colors.get(i).copied().unwrap_or((42, 46, 53));
                            let title = titles.get(i).map(|s| s.as_str()).unwrap_or("");
                            let (w, h) = d.kind().key_image_format().size;
                            let img = render_key_image(w, h, rgb, title);
                            let _ = d.set_button_image(i as u8, img);
                        }
                        let _ = d.flush();
                    }
                }
            }
        }

        if deck.is_none() {
            match try_connect() {
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
            let reader = d.get_reader();
            match reader.read(Some(Duration::from_millis(200))) {
                Ok(updates) => {
                    for u in updates {
                        if let DeviceStateUpdate::ButtonDown(key) = u {
                            if let Some(cb) = on_press.lock().as_ref() {
                                cb(key);
                            }
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

fn try_connect() -> Result<Option<(Arc<StreamDeck>, DeviceInfo)>, String> {
    let hid = new_hidapi().map_err(|e| e.to_string())?;
    let devices = list_devices(&hid);
    let pick = devices.into_iter().find(|(kind, _)| kind.is_visual());
    let Some((kind, serial)) = pick else {
        return Ok(None);
    };
    let deck = StreamDeck::connect(&hid, kind, &serial).map_err(|e| format!("{e:?}"))?;
    let deck = Arc::new(deck);
    let _ = deck.reset();
    let info = DeviceInfo {
        kind: format!("{kind:?}"),
        serial: serial.clone(),
        rows: kind.row_count(),
        cols: kind.column_count(),
        key_count: kind.key_count(),
        encoder_count: kind.encoder_count(),
    };
    Ok(Some((deck, info)))
}

fn render_key_image(w: usize, h: usize, rgb: (u8, u8, u8), _title: &str) -> DynamicImage {
    let (r, g, b) = rgb;
    let buf: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_fn(w as u32, h as u32, |_x, _y| Rgba([r, g, b, 255]));
    DynamicImage::ImageRgba8(buf)
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
