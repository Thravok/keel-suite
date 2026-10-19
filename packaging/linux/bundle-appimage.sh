#!/usr/bin/env bash
# Build an AppImage for keel-deck or keel-light.
# Usage: bundle-appimage.sh --app deck|light --version 0.1.0 --bin-dir DIR --out-dir DIR
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
APP=""
VERSION=""
BIN_DIR=""
OUT_DIR=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app) APP="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --bin-dir) BIN_DIR="$2"; shift 2 ;;
    --out-dir) OUT_DIR="$2"; shift 2 ;;
    *) echo "unknown: $1" >&2; exit 1 ;;
  esac
done

if [[ -z "$APP" || -z "$VERSION" || -z "$BIN_DIR" || -z "$OUT_DIR" ]]; then
  echo "usage: $0 --app deck|light --version V --bin-dir D --out-dir O" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

case "$APP" in
  deck)
    APP_NAME="KeelDeck"
    EXEC_NAME="keel"
    DESKTOP_NAME="keel-deck"
    COMMENT="Offline Stream Deck alternative"
    ;;
  light)
    APP_NAME="KeelLight"
    EXEC_NAME="keel-light"
    DESKTOP_NAME="keel-light"
    COMMENT="USB Key Light control"
    ;;
  *)
    echo "app must be deck or light" >&2
    exit 1
    ;;
esac

APPDIR="$STAGE/${APP_NAME}.AppDir"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/metainfo"

cp "$BIN_DIR/$EXEC_NAME" "$APPDIR/usr/bin/"
chmod +x "$APPDIR/usr/bin/$EXEC_NAME"

if [[ "$APP" == "light" && -f "$BIN_DIR/keel-light-ui" ]]; then
  cp "$BIN_DIR/keel-light-ui" "$APPDIR/usr/bin/"
  chmod +x "$APPDIR/usr/bin/keel-light-ui"
  # Best-effort Qt deploy via linuxdeploy plugin if available
fi

if [[ "$APP" == "deck" ]]; then
  mkdir -p "$APPDIR/usr/share/keel"
  cp "$ROOT/packaging/linux/40-keel-streamdeck.rules" "$APPDIR/usr/share/keel/"
  cat > "$APPDIR/usr/share/keel/README-udev.txt" <<'EOF'
Install udev rules for Stream Deck HID access:
  sudo cp usr/share/keel/40-keel-streamdeck.rules /etc/udev/rules.d/
  sudo udevadm control --reload-rules && sudo udevadm trigger
EOF
fi

cat > "$APPDIR/usr/share/applications/${DESKTOP_NAME}.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=${APP_NAME}
Comment=${COMMENT}
Exec=${EXEC_NAME}
Icon=${DESKTOP_NAME}
Categories=Utility;
Terminal=false
EOF

# Minimal placeholder icon (1x1 PNG) — replace later with real branding
printf '\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90wS\xde\x00\x00\x00\x0cIDATx\x9cc\xf8\x0f\x00\x00\x01\x01\x00\x05\x18\xd8N\x00\x00\x00\x00IEND\xaeB`\x82' \
  > "$APPDIR/${DESKTOP_NAME}.png"
ln -sf "${DESKTOP_NAME}.png" "$APPDIR/.DirIcon"

cat > "$APPDIR/AppRun" <<EOF
#!/bin/sh
HERE="\$(dirname "\$(readlink -f "\$0")")"
export PATH="\$HERE/usr/bin:\$PATH"
exec "\$HERE/usr/bin/${EXEC_NAME}" "\$@"
EOF
chmod +x "$APPDIR/AppRun"
cp "$APPDIR/usr/share/applications/${DESKTOP_NAME}.desktop" "$APPDIR/"

LINUXDEPLOY="${LINUXDEPLOY:-}"
if [[ -z "$LINUXDEPLOY" ]]; then
  if [[ -x "$ROOT/packaging/linux/tools/linuxdeploy-x86_64.AppImage" ]]; then
    LINUXDEPLOY="$ROOT/packaging/linux/tools/linuxdeploy-x86_64.AppImage"
  elif command -v linuxdeploy >/dev/null 2>&1; then
    LINUXDEPLOY="$(command -v linuxdeploy)"
  fi
fi

OUT_FILE="$OUT_DIR/${APP_NAME}-${VERSION}-linux-x64.AppImage"

if [[ -n "$LINUXDEPLOY" ]]; then
  export OUTPUT="$(basename "$OUT_FILE")"
  mkdir -p "$STAGE/out"
  EXTRA_ARGS=()
  if [[ "$APP" == "light" ]] && [[ -n "${LINUXDEPLOY_PLUGIN_QT:-}" || -x "$ROOT/packaging/linux/tools/linuxdeploy-plugin-qt-x86_64.AppImage" ]]; then
    PLUGIN="${LINUXDEPLOY_PLUGIN_QT:-$ROOT/packaging/linux/tools/linuxdeploy-plugin-qt-x86_64.AppImage}"
    EXTRA_ARGS+=(--plugin qt)
    export PATH="$(dirname "$PLUGIN"):$PATH"
  fi
  (
    cd "$STAGE/out"
    "$LINUXDEPLOY" --appdir "$APPDIR" --output appimage "${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"}"
  )
  built="$(find "$STAGE/out" -name '*.AppImage' | head -1)"
  mv "$built" "$OUT_FILE"
else
  # Fallback: tar.gz if linuxdeploy unavailable
  OUT_FILE="$OUT_DIR/${APP_NAME}-${VERSION}-linux-x64.tar.gz"
  tar -C "$APPDIR" -czf "$OUT_FILE" .
  echo "linuxdeploy not found — wrote tarball $OUT_FILE" >&2
fi

echo "$OUT_FILE"
