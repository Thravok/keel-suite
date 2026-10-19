#!/usr/bin/env bash
# Codesign and notarize a DMG or .app.
# Requires env: APPLE_CERTIFICATE_P12 (base64), APPLE_CERTIFICATE_PASSWORD,
# and either APPLE_API_KEY+APPLE_API_KEY_ID+APPLE_API_ISSUER or
# APPLE_ID+APPLE_APP_SPECIFIC_PASSWORD+APPLE_TEAM_ID.
set -euo pipefail

TARGET="${1:?usage: sign-notarize.sh path/to/App.app|file.dmg}"
IDENTITY="${APPLE_SIGN_IDENTITY:-Developer ID Application}"
BUNDLE_ID="${APPLE_BUNDLE_ID:-app.Keel.Keel}"

if [[ -z "${APPLE_CERTIFICATE_P12:-}" ]]; then
  echo "APPLE_CERTIFICATE_P12 not set — skipping codesign/notarize (unsigned artifact)" >&2
  exit 0
fi

KEYCHAIN_PWD="${APPLE_KEYCHAIN_PASSWORD:-keel-ci-temp}"
KEYCHAIN="keel-signing.keychain-db"
CERT_PATH="$(mktemp).p12"
echo "$APPLE_CERTIFICATE_P12" | base64 --decode > "$CERT_PATH"

security create-keychain -p "$KEYCHAIN_PWD" "$KEYCHAIN" || true
security set-keychain-settings -lut 21600 "$KEYCHAIN"
security unlock-keychain -p "$KEYCHAIN_PWD" "$KEYCHAIN"
security import "$CERT_PATH" -k "$KEYCHAIN" -P "${APPLE_CERTIFICATE_PASSWORD:-}" -T /usr/bin/codesign -T /usr/bin/security
security set-key-partition-list -S apple-tool:,apple: -s -k "$KEYCHAIN_PWD" "$KEYCHAIN"
security list-keychains -d user -s "$KEYCHAIN" $(security list-keychains -d user | sed 's/"//g')
rm -f "$CERT_PATH"

# Resolve full identity if short name given
if [[ "$IDENTITY" == "Developer ID Application" ]]; then
  FOUND="$(security find-identity -v -p codesigning "$KEYCHAIN" | grep 'Developer ID Application' | head -1 | sed -E 's/.*"(.+)"/\1/' || true)"
  if [[ -n "$FOUND" ]]; then
    IDENTITY="$FOUND"
  fi
fi

sign_app() {
  local app="$1"
  codesign --force --deep --options runtime --timestamp --sign "$IDENTITY" "$app"
  codesign --verify --deep --strict "$app"
}

if [[ "$TARGET" == *.app ]]; then
  sign_app "$TARGET"
  NOTARIZE_PATH="$TARGET"
elif [[ "$TARGET" == *.dmg ]]; then
  # Sign nested .app first if present
  while IFS= read -r -d '' app; do
    sign_app "$app"
  done < <(find /Volumes -maxdepth 0 2>/dev/null; true)
  # Remount not needed — sign apps before DMG creation preferred.
  # If DMG already made, sign the DMG itself:
  codesign --force --timestamp --sign "$IDENTITY" "$TARGET"
  NOTARIZE_PATH="$TARGET"
else
  codesign --force --options runtime --timestamp --sign "$IDENTITY" "$TARGET"
  NOTARIZE_PATH="$TARGET"
fi

submit_notarize() {
  local path="$1"
  if [[ -n "${APPLE_API_KEY:-}" && -n "${APPLE_API_KEY_ID:-}" && -n "${APPLE_API_ISSUER:-}" ]]; then
    local keyfile
    keyfile="$(mktemp).p8"
    echo "$APPLE_API_KEY" > "$keyfile"
    xcrun notarytool submit "$path" \
      --key "$keyfile" \
      --key-id "$APPLE_API_KEY_ID" \
      --issuer "$APPLE_API_ISSUER" \
      --wait
    rm -f "$keyfile"
  elif [[ -n "${APPLE_ID:-}" && -n "${APPLE_APP_SPECIFIC_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
    xcrun notarytool submit "$path" \
      --apple-id "$APPLE_ID" \
      --password "$APPLE_APP_SPECIFIC_PASSWORD" \
      --team-id "$APPLE_TEAM_ID" \
      --wait
  else
    echo "No notarization credentials (API key or Apple ID) — codesigned only" >&2
    return 0
  fi
  xcrun stapler staple "$path" || true
}

submit_notarize "$NOTARIZE_PATH"
echo "signed: $TARGET"
