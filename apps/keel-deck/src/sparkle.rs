//! macOS Sparkle updater bridge (no-op on other platforms).

#[cfg(target_os = "macos")]
mod ffi {
    extern "C" {
        pub fn keel_sparkle_init();
        pub fn keel_sparkle_check_for_updates();
    }
}

/// Start background Sparkle checks when the framework is bundled.
pub fn init() {
    #[cfg(target_os = "macos")]
    unsafe {
        ffi::keel_sparkle_init();
    }
}

/// Show the Sparkle check-for-updates UI (macOS only).
pub fn check_for_updates() {
    #[cfg(target_os = "macos")]
    unsafe {
        ffi::keel_sparkle_check_for_updates();
    }
}
