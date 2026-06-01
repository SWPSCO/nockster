#!/usr/bin/env bash
set -euo pipefail

stage="${PROVISION_STAGE:-${1:-production}}"
port="${PROVISION_PORT:-/dev/ttyACM0}"
secret_dir="${NOCKSTER_SECRET_DIR:-../nockster-secrets}"
hmac_key_file="${HMAC_KEY_FILE:-${secret_dir}/hmac-up.bin}"
secure_boot_key_file="${SECURE_BOOT_KEY_FILE:-${secret_dir}/secure-boot-v2.pem}"
flash_key_file="${FLASH_ENCRYPTION_KEY_FILE:-${secret_dir}/flash-encryption-key.bin}"
update_key_file="${UPDATE_SIGNING_KEY_FILE:-${secret_dir}/release-signing-key.hex}"
update_bundle="${UPDATE_BUNDLE:-nockster-fw.update.json}"
update_firmware="${UPDATE_FIRMWARE:-target/xtensa-esp32s3-none-elf/release/nockster-fw.bin}"
update_index="${UPDATE_INDEX:-latest.json}"
configured_release_version="${NOCKSTER_RELEASE_VERSION:-}"
if [[ -z "${configured_release_version}" || "${configured_release_version}" == "0" ]]; then
  release_version="<release-version>"
else
  release_version="${configured_release_version}"
fi
trust_hash="${NOCKSTER_UPDATE_PUBKEY_SHA256_HEX:-<sha256-of-compressed-release-pubkey>}"
secure_boot_image="${SECURE_BOOT_IMAGE:-${update_firmware}}"
secure_boot_signed_image="${SECURE_BOOT_SIGNED_IMAGE:-${update_firmware%.bin}.signed.bin}"
secure_boot_digest_block="${SECURE_BOOT_DIGEST_BLOCK:-BLOCK_KEY0}"
flash_key_block="${FLASH_ENCRYPTION_KEY_BLOCK:-BLOCK_KEY4}"
flash_crypt_cnt="${FLASH_CRYPT_CNT_VALUE:-0x7}"

usage() {
  cat <<'USAGE'
usage: make provision-plan [PROVISION_STAGE=production] [PROVISION_PORT=/dev/ttyACM0]

Stages:
  hmac-up           HMAC_UP key provisioning and NVS v2 migration checklist
  secure-boot      secure boot v2 signing/provisioning checklist
  flash-encryption flash encryption key/enable checklist
  lockdown         production lockdown validation checklist
  production       full ordered production checklist

This script prints commands only. It does not read or write eFuses.
USAGE
}

step() {
  printf '\n%s\n' "$1"
  printf '%s\n' "------------------------------------------------------------"
}

cmd() {
  printf '  %s\n' "$*"
}

note() {
  printf '  # %s\n' "$*"
}

common_header() {
  cat <<EOF
Nockster provisioning plan
stage: ${stage}
port: ${port}

This is a dry-run checklist. It prints the explicit commands to run later and
does not invoke espflash, espsecure, espefuse, or nockster-cli.
EOF
}

baseline_status() {
  step "0. Baseline status build"
  note "Use a chip-security build to read eFuse status without writing fuses."
  cmd "make fw-chip-security"
  cmd "make flash FLASH_PORT=${port} FW_PROFILE=chip-security"
  cmd "nockster-cli security --port hid --expect-chip-security"
  cmd "make provision-summary PROVISION_PORT=${port}"
}

hmac_up_plan() {
  step "1. HMAC_UP storage pepper"
  note "Generate the key outside this repo; this creates a local secret file only."
  cmd "make generate-hmac-up-key HMAC_KEY_FILE=${hmac_key_file}"
  note "Review eFuse state before the irreversible write."
  cmd "make provision-summary PROVISION_PORT=${port}"
  cmd "make provision-hmac-up PROVISION_PORT=${port} HMAC_KEY_FILE=${hmac_key_file} CONFIRM_IRREVERSIBLE=burn-hmac-up"
  cmd "nockster-cli security --port hid --expect-chip-security --expect-hmac-up --expect-hmac-up-read-protected"
  note "After initializing or unlocking existing storage, migrate schema v1 to v2 explicitly."
  cmd "nockster-cli security --port hid --migrate-nvs-v2 --current-pin <pin> --expect-nvs-v2"
}

secure_boot_plan() {
  step "2. Secure boot v2"
  note "Generate the update release signing key outside this repo and record its trust hash."
  cmd "make generate-update-signing-key UPDATE_SIGNING_KEY_FILE=${update_key_file}"
  cmd "make update-pubkey UPDATE_SIGNING_KEY_FILE=${update_key_file}"
  note "Generate the secure-boot signing key outside this repo."
  cmd "make generate-secure-boot-v2-key SECURE_BOOT_KEY_FILE=${secure_boot_key_file}"
  note "Build a release image with a nonzero release counter and pinned update trust hash."
  cmd "make fw FW_PROFILE=production ALLOW_UNSIGNED_PRODUCTION=1 NOCKSTER_RELEASE_VERSION=${release_version} NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=${trust_hash}"
  cmd "make release-sign-secure-boot-v2 SECURE_BOOT_KEY_FILE=${secure_boot_key_file} SECURE_BOOT_IMAGE=${secure_boot_image} SECURE_BOOT_SIGNED_IMAGE=${secure_boot_signed_image}"
  note "Burn the digest only after sacrificial-board rejection tests pass."
  cmd "make provision-summary PROVISION_PORT=${port}"
  cmd "make provision-secure-boot-v2-digest PROVISION_PORT=${port} SECURE_BOOT_KEY_FILE=${secure_boot_key_file} SECURE_BOOT_DIGEST_BLOCK=${secure_boot_digest_block} CONFIRM_IRREVERSIBLE=burn-secure-boot-v2"
  cmd "nockster-cli security --port hid --expect-chip-security --expect-secure-boot"
}

flash_encryption_plan() {
  step "3. Flash encryption"
  note "Generate the flash-encryption key outside this repo."
  cmd "make generate-flash-encryption-key FLASH_ENCRYPTION_KEY_FILE=${flash_key_file}"
  note "Burn the key and enable encryption as separate reviewed actions."
  cmd "make provision-summary PROVISION_PORT=${port}"
  cmd "make provision-flash-encryption-key PROVISION_PORT=${port} FLASH_ENCRYPTION_KEY_FILE=${flash_key_file} FLASH_ENCRYPTION_KEY_BLOCK=${flash_key_block} CONFIRM_IRREVERSIBLE=burn-flash-encryption-key"
  cmd "make provision-flash-encryption-enable PROVISION_PORT=${port} FLASH_CRYPT_CNT_VALUE=${flash_crypt_cnt} CONFIRM_IRREVERSIBLE=enable-flash-encryption"
  cmd "nockster-cli security --port hid --expect-chip-security --expect-flash-encryption"
}

release_preflight_plan() {
  step "4. Strict release preflight"
  cmd "make release-preflight FW_PROFILE=production RELEASE_PREFLIGHT_STRICT=1 NOCKSTER_AUTO_MIGRATE_NVS_V2=0 NOCKSTER_RELEASE_VERSION=${release_version} NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=${trust_hash} HMAC_KEY_FILE=${hmac_key_file} UPDATE_SIGNING_KEY_FILE=${update_key_file} SECURE_BOOT_KEY_FILE=${secure_boot_key_file} FLASH_ENCRYPTION_KEY_FILE=${flash_key_file} UPDATE_BUNDLE=${update_bundle} UPDATE_FIRMWARE=${update_firmware}"
  note "Generate the browser updater index from the verified signed bundle and firmware."
  cmd "make update-index UPDATE_BUNDLE=${update_bundle} UPDATE_FIRMWARE=${update_firmware} UPDATE_INDEX=${update_index}"
  note "Validate the browser updater index immediately before publishing."
  cmd "make release-preflight FW_PROFILE=production RELEASE_PREFLIGHT_STRICT=1 NOCKSTER_AUTO_MIGRATE_NVS_V2=0 NOCKSTER_RELEASE_VERSION=${release_version} NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=${trust_hash} HMAC_KEY_FILE=${hmac_key_file} UPDATE_SIGNING_KEY_FILE=${update_key_file} SECURE_BOOT_KEY_FILE=${secure_boot_key_file} FLASH_ENCRYPTION_KEY_FILE=${flash_key_file} UPDATE_BUNDLE=${update_bundle} UPDATE_FIRMWARE=${update_firmware} UPDATE_INDEX=${update_index}"
  note "Publish ${update_index}, ${update_bundle}, and ${update_firmware} under the web updater release path."
}

lockdown_plan() {
  step "5. Production lockdown"
  note "Do this only after secure boot, flash encryption, OTA recovery, and sacrificial-board tests pass."
  note "There is intentionally no one-shot lockdown target; run each guarded action after review."
  cmd "make provision-lockdown-jtag PROVISION_PORT=${port} CONFIRM_IRREVERSIBLE=disable-jtag"
  cmd "make provision-lockdown-download PROVISION_PORT=${port} CONFIRM_IRREVERSIBLE=disable-download-mode"
  cmd "make provision-lockdown-direct-boot PROVISION_PORT=${port} CONFIRM_IRREVERSIBLE=disable-direct-boot"
  cmd "make provision-lockdown-rom-print PROVISION_PORT=${port} CONFIRM_IRREVERSIBLE=disable-rom-print"
  note "After intentionally setting production-only fuses, validate with:"
  cmd "nockster-cli security --port hid --expect-production-lockdown"
  note "Power-glitch protection remains a separate board-specific validation:"
  cmd "make provision-power-glitch-protection PROVISION_PORT=${port} CONFIRM_IRREVERSIBLE=enable-power-glitch"
  cmd "nockster-cli security --port hid --expect-power-glitch-protection"
}

common_header

case "${stage}" in
  -h|--help|help)
    usage
    ;;
  hmac-up)
    baseline_status
    hmac_up_plan
    ;;
  secure-boot)
    baseline_status
    secure_boot_plan
    ;;
  flash-encryption)
    baseline_status
    flash_encryption_plan
    ;;
  lockdown)
    baseline_status
    lockdown_plan
    ;;
  production|all)
    baseline_status
    hmac_up_plan
    secure_boot_plan
    flash_encryption_plan
    release_preflight_plan
    lockdown_plan
    ;;
  *)
    echo "unsupported PROVISION_STAGE: ${stage}" >&2
    usage >&2
    exit 2
    ;;
esac
