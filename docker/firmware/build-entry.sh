#!/usr/bin/env bash
# Build + sign a Nockster firmware release inside the container, then copy the
# artifacts to /out. The signing key is mounted read-only at /keys/signing.key
# (outside the repo root, as nockster-cli requires).
set -euo pipefail

# Authenticate cargo to private SWPSCO git deps (tx-types, nockchain, ...).
# GIT_AUTH_TOKEN is a scoped token with read access, injected at `docker run`.
if [[ -n "${GIT_AUTH_TOKEN:-}" ]]; then
  git config --global \
    url."https://x-access-token:${GIT_AUTH_TOKEN}@github.com/".insteadOf \
    "https://github.com/"
fi
export CARGO_NET_GIT_FETCH_WITH_CLI=true

source "$HOME/export-esp.sh"

: "${RELEASE_VERSION:?set RELEASE_VERSION (integer u32)}"
: "${FW_PROFILE:=production}"
: "${NOCKSTER_UPDATE_PUBKEY_SHA256_HEX:?set NOCKSTER_UPDATE_PUBKEY_SHA256_HEX (public trust anchor)}"
KEY_FILE="${UPDATE_SIGNING_KEY_FILE:-/keys/signing.key}"

if [[ ! -f "$KEY_FILE" ]]; then
  echo "signing key not found at $KEY_FILE (mount it read-only)" >&2
  exit 1
fi

export NOCKSTER_UPDATE_PUBKEY_SHA256_HEX

# Chains fw (--release) -> save-image -> sign -> verify. The production guard
# is bypassed here because this release-only path signs immediately afterward.
make signed-update \
  FW_PROFILE="$FW_PROFILE" \
  NOCKSTER_RELEASE_VERSION="$RELEASE_VERSION" \
  UPDATE_SIGNING_KEY_FILE="$KEY_FILE" \
  ALLOW_UNSIGNED_PRODUCTION=1

# OTA release index with relative asset URLs (served from my.nockster.com/updates/).
cargo run -q -p nockster-cli --bin nockster-cli -- update index \
  --bundle target/update/nockster-fw.update.json \
  --firmware target/update/nockster-fw.bin \
  --out target/update/latest.json

mkdir -p /out
cp target/update/nockster-fw.bin \
   target/update/nockster-fw.update.json \
   target/update/latest.json \
   /out/
echo "Artifacts written to /out:"
ls -l /out
