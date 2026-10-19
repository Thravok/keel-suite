#!/usr/bin/env bash
# Build a macOS .app bundle.
# Usage: bundle-app.sh --name Keel --id app.Keel.Keel --exe path/to/binary [--extra src:dest ...] [--resources dir]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../common/version.sh
source "$ROOT/packaging/common/version.sh"

BUNDLE_NAME=""
BUNDLE_ID=""
EXECUTABLE_SRC=""
OUT_DIR="${OUT_DIR:-$ROOT/packaging/out/macos}"
VERSION="${VERSION:-$(keel_version)}"
SPARKLE_FEED_URL="${SPARKLE_FEED_URL:-https://github.com/Thravok/keel-suite/releases/latest/download/appcast.xml}"
SPARKLE_PUBLIC_ED_KEY="${SPARKLE_PUBLIC_ED_KEY:-}"
EXTRAS=()
RESOURCES_DIR=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --name) BUNDLE_NAME="$2"; shift 2 ;;
    --id) BUNDLE_ID="$2"; shift 2 ;;
    --exe) EXECUTABLE_SRC="$2"; shift 2 ;;
    --out) OUT_DIR="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --resources) RESOURCES_DIR="$2"; shift 2 ;;
    --extra) EXTRAS+=("$2"); shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 1 ;;
  esac
done

if [[ -z "$BUNDLE_NAME" || -z "$BUNDLE_ID" || -z "$EXECUTABLE_SRC" ]]; then
  echo "usage: $0 --name NAME --id BUNDLE_ID --exe BINARY [--extra src:relpath]..." >&2
  exit 1
fi

if [[ -z "$SPARKLE_PUBLIC_ED_KEY" ]]; then
  SPARKLE_PUBLIC_ED_KEY="$(grep -v '^#' "$ROOT/packaging/sparkle/public_ed_key.txt" | head -1 | tr -d '[:space:]')"
fi

APP="$OUT_DIR/${BUNDLE_NAME}.app"
CONTENTS="$APP/Contents"
MACOS="$CONTENTS/MacOS"
RES="$CONTENTS/Resources"
mkdir -p "$MACOS" "$RES"

EXECUTABLE_NAME="$(basename "$EXECUTABLE_SRC")"
cp "$EXECUTABLE_SRC" "$MACOS/$EXECUTABLE_NAME"
chmod +x "$MACOS/$EXECUTABLE_NAME"

for extra in "${EXTRAS[@]+"${EXTRAS[@]}"}"; do
  src="${extra%%:*}"
  dest="${extra#*:}"
  mkdir -p "$(dirname "$MACOS/$dest")"
  cp -R "$src" "$MACOS/$dest"
done

if [[ -n "$RESOURCES_DIR" && -d "$RESOURCES_DIR" ]]; then
  cp -R "$RESOURCES_DIR"/. "$RES/"
fi

sed \
  -e "s|@EXECUTABLE@|${EXECUTABLE_NAME}|g" \
  -e "s|@BUNDLE_ID@|${BUNDLE_ID}|g" \
  -e "s|@BUNDLE_NAME@|${BUNDLE_NAME}|g" \
  -e "s|@VERSION@|${VERSION}|g" \
  -e "s|@SPARKLE_FEED_URL@|${SPARKLE_FEED_URL}|g" \
  -e "s|@SPARKLE_PUBLIC_ED_KEY@|${SPARKLE_PUBLIC_ED_KEY}|g" \
  "$ROOT/packaging/macos/Info.plist.in" > "$CONTENTS/Info.plist"

# Sparkle.framework if present (downloaded by fetch-sparkle.sh)
SPARKLE_FW="${SPARKLE_FRAMEWORK:-$ROOT/packaging/sparkle/Sparkle.framework}"
if [[ -d "$SPARKLE_FW" ]]; then
  mkdir -p "$CONTENTS/Frameworks"
  rm -rf "$CONTENTS/Frameworks/Sparkle.framework"
  cp -R "$SPARKLE_FW" "$CONTENTS/Frameworks/"
fi

echo "$APP"
