# Keel

**Offline cross-platform command deck** for macOS, Windows, and Linux.

Virtual Stream Deck UI plus real Elgato hardware (USB HID VID `0x0FD9`) when connected. Built-in actions plus an offline Stream Deck SDK v2 plugin host (plaintext packages only). No marketplace, no cloud, no telemetry.

## Features (v1)

- Layouts: Mini 3×2, Original/Mk2 5×3, XL 8×4
- Profiles, pages, folder navigation (`go_page` / `go_back`)
- Actions: hotkey, open URL, open app/file, run command, media keys, type text, brightness
- **Plugin import:** `.streamDeckPlugin` ZIP or `.sdPlugin` folder (plaintext `manifest.json`); localhost WebSocket host (`registerPlugin`, `willAppear`, `keyDown`/`keyUp`, `setTitle`/`setImage`)
- System tray (close hides to tray)
- Key colors synced to hardware when a deck is present

## Develop

Requirements: Rust stable, platform C toolchains (Xcode CLT on macOS).

```bash
cargo run --bin keel
```

### Plugin host smoke (offline)

```bash
cargo build --bin keel-echo-plugin
cargo run --bin keel-plugin-smoke
```

Fixture: [`fixtures/com.keel.echo.sdPlugin`](fixtures/com.keel.echo.sdPlugin).

Import UI: **Import plugin…** (toolbar or Settings). ELGATO-sealed manifests (installed Elgato Application Support copies) are rejected with a message to import the original plaintext package.

## Build

```bash
cargo build --release --bin keel
```

### Linux HID

```bash
sudo cp packaging/linux/40-keel-streamdeck.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
```

### macOS

Grant **Accessibility** (and Input Monitoring if prompted) so Enigo can simulate hotkeys / text / media keys.

## Data

macOS example:

- Profiles: `~/Library/Application Support/app.Keel.Keel/keel/state.json`
- Plugins: `~/Library/Application Support/app.Keel.Keel/keel/plugins/*.sdPlugin`

## Privacy

See [docs/PRIVACY.md](docs/PRIVACY.md).
