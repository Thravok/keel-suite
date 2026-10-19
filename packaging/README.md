# Packaging

Cross-platform release helpers used by CI and local builds.

| Path | Role |
| --- | --- |
| `macos/` | `.app` bundle, `macdeployqt`, DMG, codesign/notarize, HAL driver, Sparkle ObjC bridge |
| `windows/` | NSIS scripts + `bundle.ps1` (Authenticode when secrets set) |
| `linux/` | AppImage via linuxdeploy, Stream Deck udev rules |
| `sparkle/` | Public EdDSA key + `generate-appcast.sh` |
| `common/version.sh` | Read workspace version / parse `v*` tags |

## Quick local builds

```bash
# macOS (all three apps)
VERSION=0.1.0 SIGN=0 UNIVERSAL=0 packaging/macos/build-release.sh

# Windows (from PowerShell, after cargo + Qt UI build)
.\packaging\windows\bundle.ps1 -App deck -Version 0.1.0 -BinDir target\release -OutDir dist\windows

# Linux
packaging/linux/fetch-linuxdeploy.sh
packaging/linux/bundle-appimage.sh --app deck --version 0.1.0 --bin-dir target/release --out-dir dist/linux
```

See root [README.md](../README.md) for GitHub Actions secrets and release tagging.
