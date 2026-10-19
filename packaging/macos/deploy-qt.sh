#!/usr/bin/env bash
# Run macdeployqt on a Qt UI binary inside an .app or standalone.
# Usage: deploy-qt.sh path/to/App.app/Contents/MacOS/keel-*-ui
set -euo pipefail

UI_BIN="${1:?usage: deploy-qt.sh path/to/qt-ui-binary}"

MACDEPLOYQT="${MACDEPLOYQT:-}"
if [[ -z "$MACDEPLOYQT" ]]; then
  for c in \
    "$(brew --prefix qt@6 2>/dev/null)/bin/macdeployqt" \
    "$(brew --prefix qt 2>/dev/null)/bin/macdeployqt" \
    /opt/homebrew/opt/qt@6/bin/macdeployqt \
    /opt/homebrew/opt/qt/bin/macdeployqt \
    /usr/local/opt/qt@6/bin/macdeployqt; do
    if [[ -x "$c" ]]; then
      MACDEPLOYQT="$c"
      break
    fi
  done
fi

if [[ -z "$MACDEPLOYQT" || ! -x "$MACDEPLOYQT" ]]; then
  echo "macdeployqt not found; set MACDEPLOYQT or install Qt 6" >&2
  exit 1
fi

# If binary is inside .app, deploy the app; else deploy adjacent
case "$UI_BIN" in
  *.app/Contents/MacOS/*)
    APP="${UI_BIN%.app/Contents/MacOS/*}.app"
    "$MACDEPLOYQT" "$APP" -always-overwrite
    ;;
  *)
    "$MACDEPLOYQT" "$UI_BIN" -always-overwrite
    ;;
esac

echo "deployed Qt deps for $UI_BIN"
