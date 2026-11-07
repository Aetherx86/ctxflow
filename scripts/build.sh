#!/usr/bin/env bash
# Builds everything from source: kernel.wasm, the obfuscated JS host, the web app and the binary.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
"$ROOT/scripts/build-kernel.sh"
"$ROOT/scripts/build-js-host.sh"
mkdir -p "$ROOT/dist/web/engine"
cp -R "$ROOT/hosts/web/." "$ROOT/dist/web/"
cp "$ROOT/hosts/js/dist/index.mjs" "$ROOT/hosts/js/dist/kernel.wasm" "$ROOT/dist/web/engine/"
cargo build --release --locked -p ctxflow --manifest-path "$ROOT/Cargo.toml"
echo "Binary: $ROOT/target/release/ctxflow"
echo "Static app: $ROOT/dist/web"
