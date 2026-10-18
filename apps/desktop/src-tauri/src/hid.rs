//! Elgato Stream Deck HID manager (VID 0x0FD9 via elgato-streamdeck).
//!
//! The live `StreamDeck` handle stays on a dedicated worker thread (hidapi
//! devices are not Sync). Other threads talk through a command channel.

use crate::model::DeviceInfo;
use elgato_streamdeck::{list_devices, new_hidapi, DeviceStateUpdate, StreamDeck};
use image::{DynamicImage, ImageBuffer, Rgba};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub type KeyPressCallback = Arc<dyn Fn(u8) + Send + Sync + 'static>;

enum HidCmd {
    SetBrightness(u8),
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
    tx: Mutex<Option<Sender<HidCmd>>>,
    on_press: Arc<Mutex<Option<KeyPressCallback>>>,
}

impl HidDeviceManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            running: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
            connected: Arc::new(Mutex::new(Vec::new())),
            tx: Mutex::new(None),
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
        *self.tx.lock() = Some(tx);

        let running = Arc::clone(&self.running);
        let connected = Arc::clone(&self.connected);
        let on_press = Arc::clone(&self.on_press);

        let handle = thread::spawn(move || {
            let mut deck: Option<Arc<StreamDeck>> = None;
            let mut reader = None;

            while running.load(Ordering::SeqCst) {
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        HidCmd::Stop => {
                            running.store(false, Ordering::SeqCst);
                        }
                        HidCmd::SetBrightness(p) => {
                            if let Some(d) = deck.as_ref() {
                                let _ = d.set_brightness(p.min(100));
                            }
                        }
                        HidCmd::SyncPage { colors, titles } => {
                            if let Some(d) = deck.as_ref() {
                                let kind = d.kind();
                                if !kind.is_visual() {
                                    continue;
                                }
                                let n = kind.key_count() as usize;
                                for i in 0..n {
                                    let rgb = colors.get(i).copied().unwrap_or((42, 46, 53));
                                    let title =
                                        titles.get(i).map(|s| s.as_str()).unwrap_or("");
                                    let (w, h) = kind.key_image_format().size;
                                    let img =
                                        render_key_image(w as u32, h as u32, rgb, title);
                                    let _ = d.set_button_image(i as u8, img);
                                }
                                let _ = d.flush();
                            }
                        }
                    }
                }

                if !running.load(Ordering::SeqCst) {
                    break;
                }

                if deck.is_none() {
                    match try_connect_arc(&connected) {
                        Ok(Some(d)) => {
                            reader = Some(d.get_reader());
                            deck = Some(d);
                        }
                        Ok(None) => {
                            thread::sleep(Duration::from_millis(800));
                            continue;
                        }
                        Err(_) => {
                            thread::sleep(Duration::from_millis(1200));
                            continue;
                        }
                    }
                }

                let Some(r) = reader.as_ref() else {
                    thread::sleep(Duration::from_millis(100));
                    continue;
                };

                match r.read(Some(Duration::from_millis(200))) {
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
                        reader = None;
                        connected.lock().clear();
                        thread::sleep(Duration::from_millis(500));
                    }
                }
            }

            connected.lock().clear();
        });

        *self.worker.lock() = Some(handle);
    }

    pub fn stop(&self) {
        if let Some(tx) = self.tx.lock().as_ref() {
            let _ = tx.send(HidCmd::Stop);
        }
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = self.worker.lock().take() {
            let _ = h.join();
        }
        *self.tx.lock() = None;
        self.connected.lock().clear();
    }

    pub fn set_brightness(&self, percent: u8) -> Result<(), String> {
        let tx = self.tx.lock();
        let tx = tx.as_ref().ok_or("hid worker not started")?;
        tx.send(HidCmd::SetBrightness(percent.min(100)))
            .map_err(|e| e.to_string())
    }

    pub fn sync_page_colors(
        &self,
        colors: &[(u8, u8, u8)],
        titles: &[String],
    ) -> Result<(), String> {
        let tx = self.tx.lock();
        let tx = tx.as_ref().ok_or("hid worker not started")?;
        tx.send(HidCmd::SyncPage {
            colors: colors.to_vec(),
            titles: titles.to_vec(),
        })
        .map_err(|e| e.to_string())
    }
}

fn try_connect_arc(
    connected: &Mutex<Vec<DeviceInfo>>,
) -> Result<Option<Arc<StreamDeck>>, String> {
    let hid = new_hidapi().map_err(|e| e.to_string())?;
    let devices = list_devices(&hid);
    let pick = devices.into_iter().find(|(kind, _)| kind.is_visual());
    let Some((kind, serial)) = pick else {
        connected.lock().clear();
        return Ok(None);
    };
    let deck = StreamDeck::connect(&hid, kind, &serial).map_err(|e| format!("{e:?}"))?;
    let _ = deck.reset();
    *connected.lock() = vec![DeviceInfo {
        kind: format!("{kind:?}"),
        serial: serial.clone(),
        rows: kind.row_count(),
        cols: kind.column_count(),
        key_count: kind.key_count(),
        encoder_count: kind.encoder_count(),
    }];
    Ok(Some(Arc::new(deck)))
}

fn render_key_image(w: u32, h: u32, rgb: (u8, u8, u8), _title: &str) -> DynamicImage {
    let (r, g, b) = rgb;
    let buf: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_fn(w, h, |_x, _y| Rgba([r, g, b, 255]));
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
