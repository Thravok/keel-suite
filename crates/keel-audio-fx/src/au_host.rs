//! In-process Audio Unit scan / host (macOS).
//!
//! This is the app-side AU rack — not WavePluginRack (Lewitt USB insert stream).

use serde::{Deserialize, Serialize};
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuComponentInfo {
    pub name: String,
    pub manufacturer: String,
    pub type_code: String,
    pub subtype: String,
    pub manufacturer_code: String,
    pub version: u32,
}

/// Scan Audio Component registry (+ optional bundle paths) for effect AUs.
pub fn scan_au_components(extra_paths: &[String]) -> Vec<AuComponentInfo> {
    let mut out = Vec::new();

    #[cfg(target_os = "macos")]
    {
        out.extend(scan_au_macos());
        for p in extra_paths {
            out.extend(scan_au_bundles(p));
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = extra_paths;
    }

    if out.is_empty() {
        // Built-in Keel DSP shown as pseudo-AUs for UI parity.
        out.push(AuComponentInfo {
            name: "Equalizer".into(),
            manufacturer: "Keel".into(),
            type_code: "aufx".into(),
            subtype: "keeq".into(),
            manufacturer_code: "Keel".into(),
            version: 1,
        });
        out.push(AuComponentInfo {
            name: "Compressor".into(),
            manufacturer: "Keel".into(),
            type_code: "aufx".into(),
            subtype: "kecp".into(),
            manufacturer_code: "Keel".into(),
            version: 1,
        });
        out.push(AuComponentInfo {
            name: "Voice Focus (Keel)".into(),
            manufacturer: "Keel".into(),
            type_code: "aufx".into(),
            subtype: "kevf".into(),
            manufacturer_code: "Keel".into(),
            version: 1,
        });
    }
    out
}

#[cfg(target_os = "macos")]
fn scan_au_macos() -> Vec<AuComponentInfo> {
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_void};

    #[repr(C)]
    struct AudioComponentDescription {
        component_type: u32,
        component_sub_type: u32,
        component_manufacturer: u32,
        component_flags: u32,
        component_flags_mask: u32,
    }

    type AudioComponent = *mut c_void;

    #[link(name = "AudioToolbox", kind = "framework")]
    extern "C" {
        fn AudioComponentCount(desc: *const AudioComponentDescription) -> u32;
        fn AudioComponentFindNext(
            in_component: AudioComponent,
            desc: *const AudioComponentDescription,
        ) -> AudioComponent;
        fn AudioComponentCopyName(
            component: AudioComponent,
            out_name: *mut *const c_void, /* CFStringRef */
        ) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringGetCString(
            the_string: *const c_void,
            buffer: *mut c_char,
            buffer_size: isize,
            encoding: u32,
        ) -> u8;
        fn CFRelease(cf: *const c_void);
    }

    const K_CFSTRING_ENCODING_UTF8: u32 = 0x08000100;
    // kAudioUnitType_Effect
    const AUFX: u32 = u32::from_be_bytes(*b"aufx");
    // Also scan music effects / generators that show in Wave Link AU picker.
    const AUMU: u32 = u32::from_be_bytes(*b"aumu");
    const AUMF: u32 = u32::from_be_bytes(*b"aumf");

    fn fourcc(v: u32) -> String {
        let b = v.to_be_bytes();
        String::from_utf8_lossy(&b).trim().to_string()
    }

    fn cfstring_to_rust(cf: *const c_void) -> String {
        if cf.is_null() {
            return String::new();
        }
        let mut buf = [0i8; 512];
        unsafe {
            if CFStringGetCString(
                cf,
                buf.as_mut_ptr(),
                buf.len() as isize,
                K_CFSTRING_ENCODING_UTF8,
            ) != 0
            {
                CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
            } else {
                String::new()
            }
        }
    }

    let mut out = Vec::new();
    for ty in [AUFX, AUMF, AUMU] {
        let desc = AudioComponentDescription {
            component_type: ty,
            component_sub_type: 0,
            component_manufacturer: 0,
            component_flags: 0,
            component_flags_mask: 0,
        };
        let count = unsafe { AudioComponentCount(&desc) };
        debug!("AU scan type {}: {count} components", fourcc(ty));
        let mut comp: AudioComponent = std::ptr::null_mut();
        for _ in 0..count.min(512) {
            comp = unsafe { AudioComponentFindNext(comp, &desc) };
            if comp.is_null() {
                break;
            }
            let mut name_cf: *const c_void = std::ptr::null();
            let status = unsafe { AudioComponentCopyName(comp, &mut name_cf) };
            let name = if status == 0 {
                let n = cfstring_to_rust(name_cf);
                if !name_cf.is_null() {
                    unsafe { CFRelease(name_cf) };
                }
                n
            } else {
                format!("AU {}", fourcc(ty))
            };
            // Skip Apple system templates that clutter the picker.
            if name.contains("AUAudioUnit") {
                continue;
            }
            out.push(AuComponentInfo {
                name: name.clone(),
                manufacturer: String::new(),
                type_code: fourcc(ty),
                subtype: String::new(),
                manufacturer_code: String::new(),
                version: 0,
            });
        }
    }
    out
}

#[cfg(target_os = "macos")]
fn scan_au_bundles(path: &str) -> Vec<AuComponentInfo> {
    use std::fs;
    use std::path::Path;

    let mut out = Vec::new();
    let root = Path::new(path);
    if !root.exists() {
        return out;
    }
    let walk = |dir: &Path, out: &mut Vec<AuComponentInfo>| {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for ent in entries.flatten() {
            let p = ent.path();
            if p.extension().and_then(|e| e.to_str()) == Some("component") {
                let name = p
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("AU")
                    .to_string();
                out.push(AuComponentInfo {
                    name,
                    manufacturer: "scan".into(),
                    type_code: "aufx".into(),
                    subtype: String::new(),
                    manufacturer_code: String::new(),
                    version: 0,
                });
            }
        }
    };
    walk(root, &mut out);
    // Common nested layout: Foo.component/Contents/...
    if root.is_dir() {
        if let Ok(entries) = fs::read_dir(root) {
            for ent in entries.flatten() {
                let p = ent.path();
                if p.is_dir() {
                    walk(&p, &mut out);
                }
            }
        }
    }
    out
}

/// Placeholder host instance — process is pass-through until full AU render graph.
#[derive(Debug, Default)]
pub struct AuHostInstance {
    pub component_name: String,
}

impl AuHostInstance {
    pub fn open(component_name: &str) -> Self {
        Self {
            component_name: component_name.to_string(),
        }
    }

    pub fn process(&self, interleaved: &mut [f32]) {
        let _ = interleaved;
        // Full AudioUnitRender hookup is phase-2; identity keeps the chain live.
    }
}
