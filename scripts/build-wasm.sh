#!/usr/bin/env bash
set -euo pipefail
export RUSTUP_TOOLCHAIN=nightly-2025-02-14
cd "$(dirname "$0")/../packages/wallet-engine"
wasm-pack build --release --target web --out-dir ../wallet/src/pkg --locked
