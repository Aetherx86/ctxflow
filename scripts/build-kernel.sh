#!/usr/bin/env bash
# Builds kernel.wasm for every host. Uses the rustup target when installed. With Homebrew Rust,
# which ships no wasm32 std, it rebuilds core and alloc from the bundled source and links with lld.
set -euo pipefail
cd "$(dirname "$0")/../kernel"
T=wasm32-unknown-unknown
if rustup target list --installed 2>/dev/null | grep -q "$T"; then
  cargo build --release --target "$T" --lib
else
  LLD="$(brew --prefix lld 2>/dev/null || true)/bin"
  [ -x "$LLD/wasm-ld" ] || LLD="/opt/homebrew/opt/lld@22/bin"
  PATH="$LLD:$PATH" RUSTC_BOOTSTRAP=1 CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER=wasm-ld \
    cargo build --release --target "$T" --lib -Zbuild-std=core,alloc
fi
W="target/$T/release/ctxflow_kernel.wasm"
cp "$W" ../src/ctxflow/kernel.wasm
mkdir -p ../hosts/js/dist && cp "$W" ../hosts/js/dist/kernel.wasm
wc -c "$W"
