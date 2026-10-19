#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
BUILD="$ROOT/build"
mkdir -p "$BUILD"
cd "$BUILD"

if [[ -z "${CMAKE_PREFIX_PATH:-}" ]]; then
  export CMAKE_PREFIX_PATH="/opt/homebrew/opt/qt:/opt/homebrew/opt/qt@6:/usr/local/opt/qt:/usr/local/opt/qt@6"
fi

cmake .. -DCMAKE_BUILD_TYPE=Release
cmake --build . --parallel
echo "Built: $BUILD/keel-light-ui"
