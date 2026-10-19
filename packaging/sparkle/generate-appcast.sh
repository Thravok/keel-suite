#!/usr/bin/env bash
# Build a minimal Sparkle appcast.xml for macOS DMG assets in a directory.
# Usage: generate-appcast.sh --dir dist/ --version 0.1.0 --out appcast.xml
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DIR=""
VERSION=""
OUT="appcast.xml"
TITLE="Keel Suite"
BASE_URL="${SPARKLE_BASE_URL:-https://github.com/Thravok/keel-suite/releases/download}"
SPARKLE_ACCOUNT="${SPARKLE_ACCOUNT:-keel-suite}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dir) DIR="$2"; shift 2 ;;
    --version) VERSION="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --title) TITLE="$2"; shift 2 ;;
    *) echo "unknown: $1" >&2; exit 1 ;;
  esac
done

if [[ -z "$DIR" || -z "$VERSION" ]]; then
  echo "usage: $0 --dir DIR --version VER [--out appcast.xml]" >&2
  exit 1
fi

SIGN_UPDATE="${SPARKLE_SIGN_UPDATE:-$ROOT/packaging/sparkle/bin/sign_update}"
PRIVATE_KEY_FILE="${SPARKLE_PRIVATE_KEY_FILE:-}"

if [[ -n "${SPARKLE_PRIVATE_KEY:-}" && -z "$PRIVATE_KEY_FILE" ]]; then
  PRIVATE_KEY_FILE="$(mktemp)"
  # Private key file is a single base64 line (no trailing commentary)
  printf '%s' "$SPARKLE_PRIVATE_KEY" | tr -d '\n' > "$PRIVATE_KEY_FILE"
  trap 'rm -f "$PRIVATE_KEY_FILE"' EXIT
fi

sign_dmg() {
  local dmg="$1"
  if [[ ! -x "$SIGN_UPDATE" ]]; then
    return 1
  fi
  if [[ -n "$PRIVATE_KEY_FILE" && -f "$PRIVATE_KEY_FILE" ]]; then
    "$SIGN_UPDATE" --ed-key-file "$PRIVATE_KEY_FILE" -p "$dmg" 2>/dev/null
  else
    # Prefer Keychain account on developer Macs
    "$SIGN_UPDATE" --account "$SPARKLE_ACCOUNT" -p "$dmg" 2>/dev/null
  fi
}

ITEMS=""
shopt -s nullglob
for dmg in "$DIR"/Keel*-macos*.dmg; do
  name="$(basename "$dmg")"
  length="$(wc -c < "$dmg" | tr -d ' ')"
  url="${BASE_URL}/v${VERSION}/${name}"
  sig=""
  if sig="$(sign_dmg "$dmg")"; then
    sig="$(echo "$sig" | tr -d '[:space:]')"
  else
    sig=""
  fi
  enc_attrs="url=\"${url}\" length=\"${length}\" type=\"application/octet-stream\""
  if [[ -n "$sig" ]]; then
    enc_attrs+=" sparkle:edSignature=\"${sig}\""
  else
    echo "warning: no Sparkle signature for $name (set SPARKLE_PRIVATE_KEY or use Keychain account $SPARKLE_ACCOUNT)" >&2
  fi
  ITEMS+="
    <item>
      <title>${name} ${VERSION}</title>
      <sparkle:version>${VERSION}</sparkle:version>
      <sparkle:shortVersionString>${VERSION}</sparkle:shortVersionString>
      <pubDate>$(date -u +"%a, %d %b %Y %H:%M:%S +0000")</pubDate>
      <enclosure ${enc_attrs}/>
    </item>"
done

cat > "$OUT" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle" xmlns:dc="http://purl.org/dc/elements/1.1/">
  <channel>
    <title>${TITLE}</title>
    <link>${BASE_URL}/v${VERSION}/</link>
    <description>Keel suite updates</description>
    <language>en</language>${ITEMS}
  </channel>
</rss>
EOF

echo "wrote $OUT"
