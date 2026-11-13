#!/usr/bin/env bash
# Build the game to web/game.wasm.
# Normal machines: rustup target add wasm32-unknown-unknown && ./build.sh
# Offline machines: set WASM_SYSROOT to a sysroot with core/alloc/compiler_builtins built for wasm32.
set -euo pipefail
cd "$(dirname "$0")"
if [ -n "${WASM_SYSROOT:-}" ]; then
  rustc --edition 2021 --crate-type cdylib --crate-name power_grid --target wasm32-unknown-unknown \
    -C opt-level=3 -C panic=abort -C lto=fat -C codegen-units=1 -C strip=symbols \
    --sysroot "$WASM_SYSROOT" -o web/game.wasm src/lib.rs
else
  cargo build --release --target wasm32-unknown-unknown
  cp target/wasm32-unknown-unknown/release/power_grid.wasm web/game.wasm
fi
ls -l web/game.wasm
