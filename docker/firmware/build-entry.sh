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
SECURE_BOOT_KEY_FILE="${SECURE_BOOT_KEY_FILE:-/keys/secure-boot-v2-rsa.pem}"

if [[ ! -f "$KEY_FILE" ]]; then
  echo "signing key not found at $KEY_FILE (mount it read-only)" >&2
  exit 1
fi
if [[ "$FW_PROFILE" == "production" && ! -f "$SECURE_BOOT_KEY_FILE" ]]; then
  echo "secure boot v2 key not found at $SECURE_BOOT_KEY_FILE (mount it read-only for production releases)" >&2
  exit 1
fi

export NOCKSTER_UPDATE_PUBKEY_SHA256_HEX

if [[ "$FW_PROFILE" == "production" ]]; then
  # Production OTA images must be signed for ESP Secure Boot v2 before the
  # update manifest is signed, so the manifest hash covers the bootable image.
  make signed-update-secure-boot-v2 \
    FW_PROFILE="$FW_PROFILE" \
    NOCKSTER_RELEASE_VERSION="$RELEASE_VERSION" \
    UPDATE_SIGNING_KEY_FILE="$KEY_FILE" \
    SECURE_BOOT_KEY_FILE="$SECURE_BOOT_KEY_FILE" \
    ALLOW_UNSIGNED_PRODUCTION=1
else
  # Dev/chip-security OTA bundles are useful for reversible update testing on
  # non-secure-booted boards.
  make signed-update \
    FW_PROFILE="$FW_PROFILE" \
    NOCKSTER_RELEASE_VERSION="$RELEASE_VERSION" \
    UPDATE_SIGNING_KEY_FILE="$KEY_FILE" \
    ALLOW_UNSIGNED_PRODUCTION=1
fi

# OTA release index with relative asset URLs (published to R2 at
# bin.aeroe.io/nockster/updates/ by the firmware-release workflow).
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
