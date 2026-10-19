# keel-audio ↔ Wave Link FEATURE_LEDGER sign-off

Source ledger: `/Users/isaac/Projects/rea/.scratch/wavelink-3.4.0/evidence/FEATURE_LEDGER.md`

| Ledger section | Module | Status |
| --- | --- | --- |
| A Platform (offline subset) | `apps/keel-audio` | Implement — no Sentry/Marketplace/iCloud; Sparkle = opt-in signed appcast only |
| B HAL virtual devices | `keel-audio-hal` + `packaging/macos` | Implement — driver binary built; install via `install-hal.sh` |
| C Mixer graph | `keel-audio-engine` | Implement — 8+4 channels, 5 mixes, routing, meters, issues |
| D Effects | `keel-audio-fx` | Implement — built-ins + AU scan surface; Voice Focus = substitute |
| E Wave hardware | `keel-audio-wave` | Implement — HID discovery + control writes (opcode map from REA) |
| F Control / RPC / deck | `keel-rpc` + deck `AudioRpc` | Implement — WS `:1885`, Origin + Bearer token, Wave Link method compat, Qt UI |
| G Privacy | `docs/PRIVACY.md` | Implement |

## Run

```bash
cargo run -p keel-audio --bin keel-audio -- --headless
./apps/keel-audio/ui/build/keel-audio-ui
cargo run -p keel-audio --bin keel-audio-rpc-smoke
```
