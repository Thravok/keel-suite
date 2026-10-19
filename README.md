# Keel

Offline Elgato-adjacent toolkit for macOS (Windows/Linux audio backends later).

| App | Role |
| --- | --- |
| **keel-deck** (`keel`) | Stream Deck alternative — virtual + USB HID, SDK plugins, egui |
| **keel-audio** | Wave Link alternative — mixer, HAL virtual devices, AU/DSP, Wave USB, Qt UI |
| **keel-light** | Control Center alternative — USB Key Light Neo/Mini (HID), Qt UI |

No accounts, marketplace, or telemetry. See [docs/PRIVACY.md](docs/PRIVACY.md).

## Workspace

```
apps/keel-deck/       # Stream Deck app (egui)
apps/keel-audio/      # Wave Link app (Rust engine + Qt UI)
apps/keel-light/      # Control Center app (USB lights + Qt UI)
apps/desktop/         # Legacy Tauri UI (kept; see DO_NOT_DELETE.md)
crates/keel-rpc/      # Shared JSON-RPC types
crates/keel-store/
crates/keel-audio-engine/
crates/keel-audio-fx/
crates/keel-audio-wave/
crates/keel-audio-hal/
crates/keel-light-usb/
packaging/macos/      # HAL driver, .app/DMG scripts, Sparkle bridge
packaging/windows/    # NSIS installers
packaging/linux/      # AppImage + udev rules
packaging/sparkle/    # Appcast + public EdDSA key
```

Wave Link REA: `/Users/isaac/Projects/rea/.scratch/wavelink-3.4.0/evidence/`.  
Control Center ledger: [docs/KEEL_LIGHT_LEDGER.md](docs/KEEL_LIGHT_LEDGER.md) (USB now; Wi‑Fi/BLE/scenes later).

## Develop

```bash
# Deck
cargo run -p keel-deck --bin keel

# Audio engine + RPC (headless)
cargo run -p keel-audio --bin keel-audio -- --headless

# Audio RPC smoke
cargo run -p keel-audio --bin keel-audio-rpc-smoke

# Qt UI (requires Qt 6 via Homebrew)
apps/keel-audio/ui/build.sh
# then, with engine running:
./apps/keel-audio/ui/build/keel-audio-ui
# or: cargo run -p keel-audio --bin keel-audio   # launches UI if built

# HAL virtual devices — usually automatic on launch (macOS admin prompt)
# when missing or outdated. Manual / force:
packaging/macos/install-hal.sh
packaging/macos/install-hal.sh --force

# Skip auto-prompt:
cargo run -p keel-audio --bin keel-audio -- --headless --skip-hal-install
# or: KEEL_SKIP_HAL_INSTALL=1
```

```bash
# Light engine + RPC (headless)
cargo run -p keel-light --bin keel-light -- --headless

# Light RPC smoke
cargo run -p keel-light --bin keel-light-rpc-smoke

# Light Qt UI
apps/keel-light/ui/build.sh
./apps/keel-light/ui/build/keel-light-ui
```

JSON-RPC audio: `ws://127.0.0.1:1885` — requires Origin `keel://` / `streamdeck://` and `Authorization: Bearer` (token in `~/Library/Application Support/app.Keel.KeelAudio/rpc.token`). Wave Link compat port: `--compat-port` → 1884 (same auth).  
JSON-RPC light: `ws://127.0.0.1:1886` — same Origin + Bearer rules (`…/app.Keel.KeelLight/rpc.token`).  
Deck actions: **Keel Audio RPC** / **Keel Light RPC** read those token files automatically.

HAL status / install RPC: `getHalStatus`, `installHalDriver` (`{"force": true}` to reinstall).
Bump `BUNDLED_DRIVER_BUILD` in `crates/keel-audio-hal/src/install.rs` **and** `CFBundleVersion` in the driver `Info.plist` whenever the HAL binary changes — launch then auto-upgrades.

## CI and releases

GitHub Actions:

- [`.github/workflows/ci.yml`](.github/workflows/ci.yml) — fmt, clippy, tests, Qt UI smoke on push/PR
- [`.github/workflows/release.yml`](.github/workflows/release.yml) — tag `v*` builds signed installers

| App | macOS DMG | Windows NSIS | Linux AppImage |
| --- | --- | --- | --- |
| keel-deck | yes | yes | yes (+ udev rules) |
| keel-light | yes | yes | yes |
| keel-audio | yes | — | — |

Bump version, tag, and push:

```bash
scripts/bump-version.sh 0.2.0
git add -A && git commit -m "chore: bump version to 0.2.0"
git tag -a v0.2.0 -m "Keel Suite v0.2.0"
git push && git push origin v0.2.0
```

Local macOS packages (unsigned unless Apple secrets are in the env):

```bash
packaging/macos/build-release.sh
```

### Signing secrets (GitHub Actions)

| Secret | Purpose |
| --- | --- |
| `APPLE_CERTIFICATE_P12` | Base64 Developer ID Application `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | P12 password |
| `APPLE_ID` / `APPLE_APP_SPECIFIC_PASSWORD` / `APPLE_TEAM_ID` | `notarytool` (Apple ID) |
| `APPLE_API_KEY` / `APPLE_API_KEY_ID` / `APPLE_API_ISSUER` | `notarytool` (App Store Connect API key; preferred) |
| `WINDOWS_CERTIFICATE_PFX` | Base64 Authenticode `.pfx` |
| `WINDOWS_CERTIFICATE_PASSWORD` | PFX password |
| `SPARKLE_PRIVATE_KEY` | Sparkle EdDSA private key (see [`packaging/sparkle/README.md`](packaging/sparkle/README.md)) |
| `SPARKLE_PUBLIC_ED_KEY` | Optional override of committed public key |
| `GPG_PRIVATE_KEY` / `GPG_PASSPHRASE` | Optional `SHA256SUMS` signature |

Commit the Sparkle **public** key in [`packaging/sparkle/public_ed_key.txt`](packaging/sparkle/public_ed_key.txt). macOS apps use Sparkle for opt-in update checks only (no telemetry).

## Privacy

See [docs/PRIVACY.md](docs/PRIVACY.md).
