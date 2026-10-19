# Privacy

Keel is designed to run **fully offline**.

## Shared principles

- No telemetry, analytics, crash phone-home, or accounts
- No marketplace or cloud sync
- Profiles and settings are stored only under the OS app data directory
- Network use only happens when **you** configure an action that opens a URL or an app that itself uses the network, or when you use **opt-in Sparkle update checks** (macOS; signed appcast fetch only — no telemetry, crash reporting, or accounts)

## Keel Deck (`keel`)

- Profiles: Application Support `app.Keel.Keel/keel/state.json`
- `run_command` spawns an explicit program path with arguments (no shell)
- HID access talks only to attached Elgato Stream Deck devices (USB VID `0x0FD9`)
- Suite RPC actions (`AudioRpc` / `LightRpc`) call loopback engines only, using each engine’s local `rpc.token`
- Sparkle (macOS): optional signed appcast fetch for updates (“Check for Updates…” in the tray menu)

## Keel Audio (`keel-audio`)

- Mixer state: Application Support `app.Keel.KeelAudio/mixer.json`
- Local JSON-RPC WebSocket on `127.0.0.1` only (default port **1885**; optional Wave Link compat **1884**)
- WebSocket handshake requires Origin `keel://` or `streamdeck://` **and** `Authorization: Bearer <token>`
- Session token is written each start to Application Support `app.Keel.KeelAudio/rpc.token` (mode `0600`); Qt UI receives it via `KEEL_AUDIO_RPC_TOKEN`
- No open localhost RPC: unauthenticated upgrades are rejected (including Wave Link compat port)
- No Sentry, Elgato accounts, Marketplace, or iCloud
- Sparkle (macOS): optional signed appcast fetch for updates only; dismissible / “Check for Updates…”; no crash phone-home
- Voice Focus uses an **open local denoise substitute** — proprietary `libaic` is never shipped
- Virtual HAL driver is local Core Audio only
- Wave USB control talks only to attached Elgato/Wave hardware (VID `0x0FD9`)

## Keel Light (`keel-light`)

- Prefs: Application Support `app.Keel.KeelLight/prefs.json`
- Local JSON-RPC WebSocket on `127.0.0.1` only (port **1886**)
- WebSocket handshake requires Origin `keel://` or `streamdeck://` **and** `Authorization: Bearer <token>`
- Session token is written each start to Application Support `app.Keel.KeelLight/rpc.token` (mode `0600`); Qt UI receives it via `KEEL_LIGHT_RPC_TOKEN`
- USB HID talks only to attached Elgato Key Light devices (VID `0x0FD9`, e.g. Neo PID `0x00A0`)
- No Sentry, AI cloud APIs, accounts, or Elgato firmware blobs
- Sparkle (macOS): optional signed appcast fetch for updates only; same rules as audio
- Wi‑Fi/BLE light control is **not** enabled in the USB phase (see ledger “Later”)

## Suite RPC (deck ↔ audio ↔ light)

- `keel-deck` calls audio/light over loopback only, using the engine token files above
- No WAN bind, no cloud relay, no shared network secret beyond the local token file
