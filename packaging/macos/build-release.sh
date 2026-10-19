#!/usr/bin/env bash
# Build signed (optional) macOS release DMGs for deck, light, and audio.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../common/version.sh
source "$ROOT/packaging/common/version.sh"

VERSION="${VERSION:-$(keel_version)}"
OUT_DIR="${OUT_DIR:-$ROOT/packaging/out/macos}"
TARGET_DIR="${TARGET_DIR:-$ROOT/target/release}"
SIGN="${SIGN:-1}"
UNIVERSAL="${UNIVERSAL:-0}"

mkdir -p "$OUT_DIR"
"$ROOT/packaging/macos/fetch-sparkle.sh"

build_rust() {
  local packages=("keel-deck" "keel-light" "keel-audio")
  if [[ "$UNIVERSAL" == "1" ]]; then
    rustup target add aarch64-apple-darwin x86_64-apple-darwin
    for p in "${packages[@]}"; do
      cargo build --release -p "$p" --target aarch64-apple-darwin
      cargo build --release -p "$p" --target x86_64-apple-darwin
    done
    mkdir -p "$OUT_DIR/lipo"
    lipo -create \
      "$ROOT/target/aarch64-apple-darwin/release/keel" \
      "$ROOT/target/x86_64-apple-darwin/release/keel" \
      -output "$OUT_DIR/lipo/keel"
    lipo -create \
      "$ROOT/target/aarch64-apple-darwin/release/keel-light" \
      "$ROOT/target/x86_64-apple-darwin/release/keel-light" \
      -output "$OUT_DIR/lipo/keel-light"
    lipo -create \
      "$ROOT/target/aarch64-apple-darwin/release/keel-audio" \
      "$ROOT/target/x86_64-apple-darwin/release/keel-audio" \
      -output "$OUT_DIR/lipo/keel-audio"
    TARGET_DIR="$OUT_DIR/lipo"
  else
    cargo build --release -p keel-deck -p keel-light -p keel-audio
  fi
}

build_qt_ui() {
  "$ROOT/apps/keel-audio/ui/build.sh"
  "$ROOT/apps/keel-light/ui/build.sh"
}

bundle_deck() {
  local app
  app="$("$ROOT/packaging/macos/bundle-app.sh" \
    --name "Keel Deck" \
    --id "app.Keel.Keel" \
    --exe "$TARGET_DIR/keel" \
    --out "$OUT_DIR" \
    --version "$VERSION")"
  if [[ "$SIGN" == "1" ]]; then
    APPLE_BUNDLE_ID="app.Keel.Keel" "$ROOT/packaging/macos/sign-notarize.sh" "$app" || true
  fi
  local dmg="$OUT_DIR/KeelDeck-${VERSION}-macos-universal.dmg"
  if [[ "$UNIVERSAL" != "1" ]]; then
    dmg="$OUT_DIR/KeelDeck-${VERSION}-macos-$(uname -m).dmg"
  fi
  "$ROOT/packaging/macos/make-dmg.sh" --app "$app" --volume "Keel Deck" --out "$dmg"
  if [[ "$SIGN" == "1" ]]; then
    APPLE_BUNDLE_ID="app.Keel.Keel" "$ROOT/packaging/macos/sign-notarize.sh" "$dmg" || true
  fi
}

bundle_light() {
  local ui="$ROOT/apps/keel-light/ui/build/keel-light-ui"
  local app
  app="$("$ROOT/packaging/macos/bundle-app.sh" \
    --name "Keel Light" \
    --id "app.Keel.KeelLight" \
    --exe "$TARGET_DIR/keel-light" \
    --out "$OUT_DIR" \
    --version "$VERSION" \
    --extra "${ui}:keel-light-ui")"
  "$ROOT/packaging/macos/deploy-qt.sh" "$app/Contents/MacOS/keel-light-ui" || \
    "$ROOT/packaging/macos/deploy-qt.sh" "$app" || true
  if [[ "$SIGN" == "1" ]]; then
    APPLE_BUNDLE_ID="app.Keel.KeelLight" "$ROOT/packaging/macos/sign-notarize.sh" "$app" || true
  fi
  local dmg="$OUT_DIR/KeelLight-${VERSION}-macos-universal.dmg"
  if [[ "$UNIVERSAL" != "1" ]]; then
    dmg="$OUT_DIR/KeelLight-${VERSION}-macos-$(uname -m).dmg"
  fi
  "$ROOT/packaging/macos/make-dmg.sh" --app "$app" --volume "Keel Light" --out "$dmg"
  if [[ "$SIGN" == "1" ]]; then
    APPLE_BUNDLE_ID="app.Keel.KeelLight" "$ROOT/packaging/macos/sign-notarize.sh" "$dmg" || true
  fi
}

bundle_audio() {
  local ui="$ROOT/apps/keel-audio/ui/build/keel-audio-ui"
  local res="$OUT_DIR/audio-resources"
  mkdir -p "$res"
  cp -R "$ROOT/packaging/macos/KeelAudioVirtual.driver" "$res/" 2>/dev/null || true
  cp "$ROOT/packaging/macos/install-hal.sh" "$res/"
  cat > "$res/HAL-README.txt" <<EOF
Keel Audio virtual HAL driver
-----------------------------
On first launch, Keel Audio can install the driver (admin password).
Manual install: run install-hal.sh from this folder, or:
  packaging/macos/install-hal.sh
EOF
  local app
  app="$("$ROOT/packaging/macos/bundle-app.sh" \
    --name "Keel Audio" \
    --id "app.Keel.KeelAudio" \
    --exe "$TARGET_DIR/keel-audio" \
    --out "$OUT_DIR" \
    --version "$VERSION" \
    --extra "${ui}:keel-audio-ui" \
    --resources "$res")"
  "$ROOT/packaging/macos/deploy-qt.sh" "$app/Contents/MacOS/keel-audio-ui" || \
    "$ROOT/packaging/macos/deploy-qt.sh" "$app" || true
  if [[ "$SIGN" == "1" ]]; then
    APPLE_BUNDLE_ID="app.Keel.KeelAudio" "$ROOT/packaging/macos/sign-notarize.sh" "$app" || true
  fi
  local dmg="$OUT_DIR/KeelAudio-${VERSION}-macos-universal.dmg"
  if [[ "$UNIVERSAL" != "1" ]]; then
    dmg="$OUT_DIR/KeelAudio-${VERSION}-macos-$(uname -m).dmg"
  fi
  "$ROOT/packaging/macos/make-dmg.sh" \
    --app "$app" \
    --volume "Keel Audio" \
    --out "$dmg" \
    --extra "$ROOT/packaging/macos/install-hal.sh"
  if [[ "$SIGN" == "1" ]]; then
    APPLE_BUNDLE_ID="app.Keel.KeelAudio" "$ROOT/packaging/macos/sign-notarize.sh" "$dmg" || true
  fi
}

build_rust
build_qt_ui
bundle_deck
bundle_light
bundle_audio
echo "macOS artifacts in $OUT_DIR"
