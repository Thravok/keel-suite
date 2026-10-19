# keel-light ↔ Elgato Control Center FEATURE_LEDGER

**Artifact:** `/Users/isaac/Downloads/ElgatoControlCenter-1.9.20829.app.zip`  
**App:** Elgato Control Center 1.9 (20829) · `com.corsair.ControlCenter` · min macOS 14  
**Scratch:** `scratch/control-center-1.9/`  
**Keel:** Offline USB-first Control Center alternative (Qt + Rust)

| Tag | Meaning |
| --- | --- |
| **Implement** | Build now (USB phase) |
| **Later** | Documented approach — build when needed |
| **Drop** | Telemetry / accounts / cloud / proprietary blobs |

---

## A. Platform / packaging

| Feature | Evidence | Keel |
| --- | --- | --- |
| Menu-bar accessory (`LSUIElement`) | Info.plist | Later (Qt can dock; optional tray) |
| Local network / Bluetooth / location prompts | Info.plist usage strings | Later (Wi‑Fi / BLE only) |
| Sentry DSN | Info.plist | **Drop** |
| Sparkle updates | Info.plist | **Implement** — opt-in signed appcast only (no telemetry) |
| AI cloud scene API | ArtificialIntelligence* strings | **Drop** |
| App Group `group.com.corsair.elgato` | Info.plist | **Drop** |
| Bundled `Firmware_*.bin` | Resources/ | **Drop** — never redistribute |

---

## B. USB lights (phase 1 — Implement)

| Feature | Evidence | Keel |
| --- | --- | --- |
| USB HID discovery | `ACCUSBDevice`, `USBDeviceBrowser`, live Neo `idProduct=0xA0` | Implement `keel-light-usb` |
| Key Light Neo USB control | HID report ID `0x02`, framed GET/PUT `/elgato/lights` | Implement |
| Key Light Mini (if HID “hid” feature) | `ACCKeyLightMiniDevice` (often Wi‑Fi-only USB-C power) | Implement when HID present; else Later via Wi‑Fi |
| Power / brightness / temperature | Same JSON as Wi‑Fi API (`on`, `brightness` 0–100, `temperature` mired 143–344) | Implement |
| USB power brightness cap | `power-info.maximumBrightness`, ECC string | Surface in state; clamp writes |
| Accessory info | `GET /elgato/accessory-info` | Implement |

### USB wire format (Neo — verified live)

512-byte HID reports, report ID `0x02`:

```
[0]=0x02  [1]=frame_idx  [2]=frame_total  [3]=0x03
[4..5]=body_len u16 LE  [6..]=body  then 0x03  zero-pad
```

Body is ASCII: `GET /elgato/lights` or `PUT /elgato/lights {"lights":[...]}` (no HTTP headers).

VID `0x0FD9`, Neo PID `0x00A0`.

---

## C. Later — when we want the rest

| Feature | ECC evidence | How to implement later |
| --- | --- | --- |
| Wi‑Fi Key Light / Air / Ring / Strip | `BonjourBrowserDelegate`, HTTP poll in Defaults.plist | mDNS `_elg._tcp` + HTTP `http://ip:9123/elgato/lights` (LAN only; reject WAN bind) |
| BLE Wi‑Fi commissioning | `BluetoothSetupWindowController` | CoreBluetooth / btleplug provisioning; never required for USB Neo |
| Scenes / favorites | `DefaultFavoriteScenes.json`, JS scene strings | Local JSON under Application Support; apply via USB and/or HTTP PUT with `scene` array |
| Light Remote | pairing / lux strings | BLE HID pairing + lux→brightness mapping |
| Firmware update | `Firmware_*.bin`, update UI strings | User-supplied file only; open updater — **do not** ship Elgato firmware |
| Eve Light Strip | localization | Separate HomeKit/Eve path or skip |
| Deck actions | Stream Deck Origin | Implement — deck `LightRpc` → authenticated WS `:1886` |
| Control Center JSON-RPC bridge | `ControlCenterJsonRpc.swift` | Optional compat if REA recovers methods; keel uses own RPC |

---

## D. Keel RPC (Implement)

Port **1886** (audio stays 1885 / Wave Link 1884).

| Method | Params | Result |
| --- | --- | --- |
| `getLights` | `{}` | `{ lights: [...] }` |
| `refreshLights` | `{}` | same |
| `setLightPower` | `{ id, on }` | `{ ok, light }` |
| `setLightBrightness` | `{ id, brightness }` | `{ ok, light }` |
| `setLightTemperature` | `{ id, temperature }` | `{ ok, light }` |

Origins: `keel://`, `streamdeck://`. Bearer token required (`rpc.token` under Application Support).
