#!/usr/bin/env bash
# Resolve workspace version from root Cargo.toml.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

keel_version() {
  sed -n '/^\[workspace.package\]/,/^\[/{s/^version = "\(.*\)"/\1/p;}' "$ROOT/Cargo.toml" | head -1
}

keel_tag_version() {
  # Strip leading v from tag ref (e.g. refs/tags/v0.1.0 or v0.1.0)
  local ref="${1:-}"
  ref="${ref#refs/tags/}"
  echo "${ref#v}"
}
