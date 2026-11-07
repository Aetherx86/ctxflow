#!/usr/bin/env bash
# Builds kernel.wasm for every host. Uses the rustup target when installed. With Homebrew Rust,
# which ships no wasm32 std, it rebuilds core and alloc from the bundled source and links with lld.
set -euo pipefail
cd "$(dirname "$0")/.."
T=wasm32-unknown-unknown
# Size is the point of this binary. These apply to everything in it, including a rebuilt core and alloc.
export CARGO_PROFILE_RELEASE_OPT_LEVEL=z
export CARGO_PROFILE_RELEASE_STRIP=symbols
# wasm-ld keeps the name section unless told otherwise, and it would publish every internal function name.
LINK=(-- -C link-arg=--strip-all)
if rustup target list --installed 2>/dev/null | grep -q "$T"; then
  cargo rustc -p ctxflow-kernel --release --target "$T" --lib --crate-type cdylib "${LINK[@]}"
else
  LLD="$(brew --prefix lld 2>/dev/null || true)/bin"
  [ -x "$LLD/wasm-ld" ] || LLD="/opt/homebrew/opt/lld@22/bin"
  PATH="$LLD:$PATH" RUSTC_BOOTSTRAP=1 CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_LINKER=wasm-ld \
    cargo rustc -p ctxflow-kernel --release --target "$T" --lib --crate-type cdylib -Zbuild-std=core,alloc "${LINK[@]}"
fi
W="target/$T/release/ctxflow_kernel.wasm"
mkdir -p hosts/js/dist && cp "$W" hosts/js/dist/kernel.wasm
wc -c "$W"
