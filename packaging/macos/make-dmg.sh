#!/usr/bin/env bash
# Create a compressed DMG from an .app (and optional extra files).
# Usage: make-dmg.sh --app Foo.app --volume "Keel Foo" --out Foo.dmg [--extra path]
set -euo pipefail

APP=""
VOLUME=""
OUT=""
EXTRAS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app) APP="$2"; shift 2 ;;
    --volume) VOLUME="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --extra) EXTRAS+=("$2"); shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 1 ;;
  esac
done

if [[ -z "$APP" || -z "$VOLUME" || -z "$OUT" ]]; then
  echo "usage: $0 --app App.app --volume NAME --out out.dmg" >&2
  exit 1
fi

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
for extra in "${EXTRAS[@]+"${EXTRAS[@]}"}"; do
  cp -R "$extra" "$STAGE/"
done

mkdir -p "$(dirname "$OUT")"
rm -f "$OUT"
hdiutil create -volname "$VOLUME" -srcfolder "$STAGE" -ov -format UDZO "$OUT"
echo "$OUT"
