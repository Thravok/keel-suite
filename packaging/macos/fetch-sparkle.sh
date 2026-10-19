#!/usr/bin/env bash
# Download a pinned Sparkle release and extract Sparkle.framework + tools.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DEST="$ROOT/packaging/sparkle"
VERSION="${SPARKLE_VERSION:-2.6.4}"
URL="https://github.com/sparkle-project/Sparkle/releases/download/${VERSION}/Sparkle-${VERSION}.tar.xz"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if [[ -d "$DEST/Sparkle.framework" && -x "$DEST/bin/sign_update" ]]; then
  echo "Sparkle already present at $DEST"
  exit 0
fi

echo "Fetching Sparkle ${VERSION}..."
curl -fsSL "$URL" -o "$TMP/sparkle.tar.xz"
tar -xJf "$TMP/sparkle.tar.xz" -C "$TMP"

# Layout varies slightly by Sparkle version
FW="$(find "$TMP" -type d -name 'Sparkle.framework' | head -1)"
BIN_DIR="$(find "$TMP" -type d -name 'bin' | head -1)"
if [[ -z "$FW" ]]; then
  echo "Sparkle.framework not found in archive" >&2
  exit 1
fi
rm -rf "$DEST/Sparkle.framework"
cp -R "$FW" "$DEST/Sparkle.framework"
mkdir -p "$DEST/bin"
if [[ -n "$BIN_DIR" ]]; then
  cp -R "$BIN_DIR"/. "$DEST/bin/" || true
fi
# Also copy top-level tools if present
for tool in sign_update generate_keys; do
  found="$(find "$TMP" -type f -name "$tool" | head -1 || true)"
  if [[ -n "$found" ]]; then
    cp "$found" "$DEST/bin/"
    chmod +x "$DEST/bin/$tool"
  fi
done

echo "Sparkle ready under $DEST"
