#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DEST="$ROOT/packaging/linux/tools"
mkdir -p "$DEST"

fetch() {
  local url="$1" out="$2"
  if [[ -x "$out" ]]; then
    return 0
  fi
  curl -fsSL "$url" -o "$out"
  chmod +x "$out"
}

fetch \
  "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage" \
  "$DEST/linuxdeploy-x86_64.AppImage"

fetch \
  "https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/continuous/linuxdeploy-plugin-qt-x86_64.AppImage" \
  "$DEST/linuxdeploy-plugin-qt-x86_64.AppImage"

echo "linuxdeploy tools ready in $DEST"
