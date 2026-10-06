#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../nockster-esp"
export RUSTUP_TOOLCHAIN=nightly-2026-06-06
wasm-pack build crates/nockster-wasm --release --target web --out-dir pkg --locked
