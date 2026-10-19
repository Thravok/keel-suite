#!/usr/bin/env bash
# Install or upgrade Keel Audio virtual HAL driver (requires sudo / admin).
# Prefer: let `keel-audio` auto-prompt on launch when missing/outdated.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SRC="$ROOT/packaging/macos/KeelAudioVirtual.driver"
DST="/Library/Audio/Plug-Ins/HAL/KeelAudioVirtual.driver"
NATIVE="$ROOT/crates/keel-audio-hal/native/keel_hal_plugin.c"
BIN_DIR="$SRC/Contents/MacOS"
PLIST="$SRC/Contents/Info.plist"
mkdir -p "$BIN_DIR"

bundled_build="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$PLIST" 2>/dev/null || echo 0)"
installed_build=0
if [[ -f "$DST/Contents/Info.plist" ]]; then
  installed_build="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$DST/Contents/Info.plist" 2>/dev/null || echo 0)"
fi

echo "Bundled HAL build: $bundled_build · installed: $installed_build"
if [[ "${1:-}" != "--force" && "$installed_build" -ge "$bundled_build" && -d "$DST" ]]; then
  echo "Already up to date. Pass --force to reinstall."
  exit 0
fi

echo "Compiling HAL plug-in…"
clang -bundle -o "$BIN_DIR/KeelAudioVirtual" "$NATIVE" \
  -framework CoreAudio -framework CoreFoundation -fPIC

echo "Installing to $DST (sudo)…"
sudo rm -rf "$DST"
sudo cp -R "$SRC" "$DST"
sudo chown -R root:wheel "$DST"

echo "Restarting coreaudiod…"
sudo killall coreaudiod || true
echo "Done. Check Audio MIDI Setup for Keel Mix / Keel virtual devices."
