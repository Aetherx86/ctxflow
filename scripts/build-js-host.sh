#!/usr/bin/env bash
# Bundles the JS host with sentinel-kit's obfuscator, unmodified. The kit builds whatever sits in
# <its root>/packages, so the host is staged next to a copy of the tool.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
KIT="${SENTINEL_KIT:-$HOME/Documents/sentinel-kit}"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/tools" "$STAGE/packages/host"
cp "$KIT/tools/obfuscate.mjs" "$STAGE/tools/"
cp -R "$ROOT/hosts/js/src" "$STAGE/packages/host/src"
(cd "$STAGE" && node --no-warnings tools/obfuscate.mjs)
mkdir -p "$ROOT/hosts/js/dist"
cp "$STAGE/packages/host/dist/index.mjs" "$ROOT/hosts/js/dist/index.mjs"
wc -c "$ROOT/hosts/js/dist/index.mjs" "$ROOT/hosts/js/dist/kernel.wasm"
