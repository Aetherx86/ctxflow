#!/usr/bin/env bash
# Builds everything from source: kernel.wasm, the obfuscated JS host, and the web app that uses them.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
"$ROOT/scripts/build-kernel.sh"
"$ROOT/scripts/build-js-host.sh"
rm -rf "$ROOT/src/ctxflow/web"
cp -R "$ROOT/hosts/web" "$ROOT/src/ctxflow/web"
mkdir -p "$ROOT/src/ctxflow/web/engine"
cp "$ROOT/hosts/js/dist/index.mjs" "$ROOT/hosts/js/dist/kernel.wasm" "$ROOT/src/ctxflow/web/engine/"
echo "Static app: $ROOT/src/ctxflow/web (deploys as plain files, or run: ctxflow serve)"
