#!/usr/bin/env bash
# Bump workspace.package.version and keep HAL marketing version in sync.
# Usage: scripts/bump-version.sh 0.2.0
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NEW="${1:?usage: $0 X.Y.Z}"

if [[ ! "$NEW" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z]+)?$ ]]; then
  echo "invalid version: $NEW" >&2
  exit 1
fi

# shellcheck source=../packaging/common/version.sh
source "$ROOT/packaging/common/version.sh"
OLD="$(keel_version)"

# Cargo.toml workspace.package version
perl -0pi -e "s/(\[workspace\.package\]\n(?:(?!\[).)*?version = )\"[^\"]+\"/\${1}\"$NEW\"/s" \
  "$ROOT/Cargo.toml"

# HAL marketing version string (not CFBundleVersion / BUNDLED_DRIVER_BUILD)
if grep -q 'BUNDLED_DRIVER_VERSION' "$ROOT/crates/keel-audio-hal/src/install.rs"; then
  perl -pi -e "s/(BUNDLED_DRIVER_VERSION:\s*&str\s*=\s*)\"[^\"]+\"/\${1}\"$NEW\"/" \
    "$ROOT/crates/keel-audio-hal/src/install.rs"
fi

PLIST="$ROOT/packaging/macos/KeelAudioVirtual.driver/Contents/Info.plist"
if [[ -f "$PLIST" ]]; then
  perl -pi -e "s/(<key>CFBundleShortVersionString<\/key>\s*<string>)[^<]+/\${1}$NEW/" "$PLIST"
fi

# Desktop Tauri (legacy) — keep in sync if present
TAURI_CARGO="$ROOT/apps/desktop/src-tauri/Cargo.toml"
if [[ -f "$TAURI_CARGO" ]]; then
  perl -pi -e "s/^version = \"[^\"]+\"/version = \"$NEW\"/" "$TAURI_CARGO" || true
fi
TAURI_JSON="$ROOT/apps/desktop/src-tauri/tauri.conf.json"
if [[ -f "$TAURI_JSON" ]]; then
  perl -pi -e "s/\"version\":\s*\"[^\"]+\"/\"version\": \"$NEW\"/" "$TAURI_JSON" || true
fi

echo "Bumped $OLD → $NEW"
echo
echo "Note: HAL CFBundleVersion / BUNDLED_DRIVER_BUILD are NOT auto-bumped."
echo "Bump those manually when the HAL binary itself changes."
echo
echo "Next:"
echo "  git add -A && git commit -m \"chore: bump version to $NEW\""
echo "  git tag -a v$NEW -m \"Keel Suite v$NEW\""
echo "  git push && git push origin v$NEW"
