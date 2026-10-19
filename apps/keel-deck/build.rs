fn main() {
    #[cfg(target_os = "macos")]
    {
        let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let root = manifest_dir.join("../..");
        let bridge = root.join("packaging/macos/sparkle_bridge.m");
        println!("cargo:rerun-if-changed={}", bridge.display());
        cc::Build::new()
            .file(&bridge)
            .flag("-fobjc-arc")
            .compile("keel_sparkle_bridge");
        println!("cargo:rustc-link-lib=framework=Foundation");
    }
}
