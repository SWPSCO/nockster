#!/usr/bin/env bash
set -euo pipefail
export RUSTUP_TOOLCHAIN=nightly-2026-04-03
cd "$(dirname "$0")/../packages/wallet-engine"
wasm-pack build --release --target web --out-dir ../wallet/src/pkg --locked
