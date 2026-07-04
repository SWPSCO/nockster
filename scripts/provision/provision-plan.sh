#!/usr/bin/env bash
set -euo pipefail

stage="${PROVISION_STAGE:-${1:-production}}"
port="${PROVISION_PORT:-/dev/ttyACM0}"
secret_dir="${NOCKSTER_SECRET_DIR:-../nockster-secrets}"
hmac_key_file="${HMAC_KEY_FILE:-${secret_dir}/hmac-up.bin}"
secure_boot_key_file="${SECURE_BOOT_KEY_FILE:-${secret_dir}/secure-boot-v2-rsa.pem}"
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
secure_boot_image="${SECURE_BOOT_IMAGE:-target/secure-boot-v2/nockster-fw.factory.bin}"
secure_boot_signed_image="${SECURE_BOOT_SIGNED_IMAGE:-target/secure-boot-v2/nockster-fw.factory.signed.bin}"
secure_boot_digest_block="${SECURE_BOOT_DIGEST_BLOCK:-BLOCK_KEY0}"
secure_boot_digest_purpose="${SECURE_BOOT_DIGEST_PURPOSE:-}"
secure_boot_bootloader_image="${SECURE_BOOT_BOOTLOADER_IMAGE:-target/secure-boot-v2/bootloader.bin}"
secure_boot_partition_table_bin="${SECURE_BOOT_PARTITION_TABLE_BIN:-target/secure-boot-v2/partition-table.bin}"
flash_key_block="${FLASH_ENCRYPTION_KEY_BLOCK:-BLOCK_KEY4}"
flash_crypt_cnt="${FLASH_CRYPT_CNT_VALUE:-0x7}"
flash_enc_bootloader_image="${FLASH_ENCRYPTION_BOOTLOADER_IMAGE:-target/secure-boot-v2/encrypted/bootloader.enc.bin}"
flash_enc_partition_table_bin="${FLASH_ENCRYPTION_PARTITION_TABLE_BIN:-target/secure-boot-v2/encrypted/partition-table.enc.bin}"
flash_enc_signed_image="${FLASH_ENCRYPTION_SIGNED_IMAGE:-target/secure-boot-v2/encrypted/nockster-fw.factory.signed.enc.bin}"
flash_enc_otadata_bin="${FLASH_ENCRYPTION_OTADATA_BIN:-target/secure-boot-v2/encrypted/otadata.blank.enc.bin}"

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
  if [[ "${stage}" == "flash-encryption" || "${stage}" == "lockdown" ]]; then
    note "This stage assumes the board is already secure-booted. Do not use ordinary make flash."
    cmd "nockster-cli security --port hid --expect-chip-security --expect-secure-boot"
    cmd "make provision-summary PROVISION_PORT=${port}"
    return
  fi
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
  note "After wiping and initializing storage, validate schema v2."
  cmd "nockster-cli security --port hid --expect-nvs-v2"
}

secure_boot_plan() {
  local digest_cmd

  step "2. Secure boot v2"
  note "Generate the update release signing key outside this repo and record its trust hash."
  cmd "make generate-update-signing-key UPDATE_SIGNING_KEY_FILE=${update_key_file}"
  cmd "make update-pubkey UPDATE_SIGNING_KEY_FILE=${update_key_file}"
  note "Generate the ESP32-S3 secure-boot signing key outside this repo. It must be RSA3072."
  cmd "make generate-secure-boot-v2-key SECURE_BOOT_KEY_FILE=${secure_boot_key_file}"
  note "Build the production app image with a nonzero release counter and pinned update trust hash."
  cmd "make update-firmware-image FW_PROFILE=production ALLOW_UNSIGNED_PRODUCTION=1 NOCKSTER_RELEASE_VERSION=${release_version} NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=${trust_hash} UPDATE_FIRMWARE=${secure_boot_image}"
  cmd "make release-sign-secure-boot-v2 SECURE_BOOT_KEY_FILE=${secure_boot_key_file} SECURE_BOOT_IMAGE=${secure_boot_image} SECURE_BOOT_SIGNED_IMAGE=${secure_boot_signed_image}"
  cmd "make release-build-secure-boot-v2-bootloader SECURE_BOOT_KEY_FILE=${secure_boot_key_file} SECURE_BOOT_BOOTLOADER_IMAGE=${secure_boot_bootloader_image} SECURE_BOOT_PARTITION_TABLE_BIN=${secure_boot_partition_table_bin}"
  note "Put the board in serial bootloader/download mode, then flash the signed bootloader and signed app without resetting."
  cmd "make flash-secure-boot-v2 FLASH_PORT=${port} SECURE_BOOT_KEY_FILE=${secure_boot_key_file} SECURE_BOOT_SIGNED_IMAGE=${secure_boot_signed_image} SECURE_BOOT_BOOTLOADER_IMAGE=${secure_boot_bootloader_image} SECURE_BOOT_PARTITION_TABLE_BIN=${secure_boot_partition_table_bin}"
  note "Burn the RSA public-key digest and then SECURE_BOOT_EN before the first normal reset."
  cmd "make provision-summary PROVISION_PORT=${port}"
  digest_cmd="make provision-secure-boot-v2-digest PROVISION_PORT=${port} SECURE_BOOT_KEY_FILE=${secure_boot_key_file} SECURE_BOOT_DIGEST_BLOCK=${secure_boot_digest_block}"
  if [[ -n "${secure_boot_digest_purpose}" ]]; then
    digest_cmd="${digest_cmd} SECURE_BOOT_DIGEST_PURPOSE=${secure_boot_digest_purpose}"
  fi
  cmd "${digest_cmd} CONFIRM_IRREVERSIBLE=burn-secure-boot-v2"
  cmd "make provision-secure-boot-v2-enable PROVISION_PORT=${port} CONFIRM_IRREVERSIBLE=enable-secure-boot-v2"
  cmd "espflash reset --port ${port}"
  cmd "nockster-cli security --port hid --expect-chip-security --expect-secure-boot"
}

flash_encryption_plan() {
  step "3. Flash encryption"
  note "Generate the flash-encryption key outside this repo."
  cmd "make generate-flash-encryption-key FLASH_ENCRYPTION_KEY_FILE=${flash_key_file}"
  note "Rebuild the signed secure-boot bootloader with flash-encryption release-mode support."
  cmd "make release-build-secure-boot-v2-bootloader SECURE_BOOT_KEY_FILE=${secure_boot_key_file} SECURE_BOOT_BOOTLOADER_FLASH_ENCRYPTION=1 SECURE_BOOT_BOOTLOADER_IMAGE=${secure_boot_bootloader_image} SECURE_BOOT_PARTITION_TABLE_BIN=${secure_boot_partition_table_bin}"
  note "Host-encrypt the signed bootloader, partition table, signed factory app, and blank otadata at their flash offsets."
  cmd "make release-encrypt-flash-v2-artifacts FLASH_ENCRYPTION_KEY_FILE=${flash_key_file} SECURE_BOOT_KEY_FILE=${secure_boot_key_file} SECURE_BOOT_BOOTLOADER_IMAGE=${secure_boot_bootloader_image} SECURE_BOOT_PARTITION_TABLE_BIN=${secure_boot_partition_table_bin} SECURE_BOOT_SIGNED_IMAGE=${secure_boot_signed_image} FLASH_ENCRYPTION_BOOTLOADER_IMAGE=${flash_enc_bootloader_image} FLASH_ENCRYPTION_PARTITION_TABLE_BIN=${flash_enc_partition_table_bin} FLASH_ENCRYPTION_SIGNED_IMAGE=${flash_enc_signed_image} FLASH_ENCRYPTION_OTADATA_BIN=${flash_enc_otadata_bin}"
  note "Put the board in serial bootloader/download mode. Burn the key, flash only ciphertext, then enable encryption before reset."
  cmd "make provision-summary PROVISION_PORT=${port}"
  cmd "make provision-flash-encryption-key PROVISION_PORT=${port} FLASH_ENCRYPTION_KEY_FILE=${flash_key_file} FLASH_ENCRYPTION_KEY_BLOCK=${flash_key_block} CONFIRM_IRREVERSIBLE=burn-flash-encryption-key"
  cmd "make flash-encrypted-secure-boot-v2 FLASH_PORT=${port} FLASH_ENCRYPTION_BOOTLOADER_IMAGE=${flash_enc_bootloader_image} FLASH_ENCRYPTION_PARTITION_TABLE_BIN=${flash_enc_partition_table_bin} FLASH_ENCRYPTION_SIGNED_IMAGE=${flash_enc_signed_image} FLASH_ENCRYPTION_OTADATA_BIN=${flash_enc_otadata_bin}"
  cmd "make provision-flash-encryption-enable PROVISION_PORT=${port} FLASH_CRYPT_CNT_VALUE=${flash_crypt_cnt} CONFIRM_IRREVERSIBLE=enable-flash-encryption"
  cmd "espflash reset --port ${port}"
  cmd "nockster-cli security --port hid --expect-chip-security --expect-secure-boot --expect-flash-encryption"
}

release_preflight_plan() {
  step "4. Strict release preflight"
  cmd "make release-preflight FW_PROFILE=production RELEASE_PREFLIGHT_STRICT=1 NOCKSTER_RELEASE_VERSION=${release_version} NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=${trust_hash} HMAC_KEY_FILE=${hmac_key_file} UPDATE_SIGNING_KEY_FILE=${update_key_file} SECURE_BOOT_KEY_FILE=${secure_boot_key_file} FLASH_ENCRYPTION_KEY_FILE=${flash_key_file} UPDATE_BUNDLE=${update_bundle} UPDATE_FIRMWARE=${update_firmware}"
  note "Generate the browser updater index from the verified signed bundle and firmware."
  cmd "make update-index UPDATE_BUNDLE=${update_bundle} UPDATE_FIRMWARE=${update_firmware} UPDATE_INDEX=${update_index}"
  note "Validate the browser updater index immediately before publishing."
  cmd "make release-preflight FW_PROFILE=production RELEASE_PREFLIGHT_STRICT=1 NOCKSTER_RELEASE_VERSION=${release_version} NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=${trust_hash} HMAC_KEY_FILE=${hmac_key_file} UPDATE_SIGNING_KEY_FILE=${update_key_file} SECURE_BOOT_KEY_FILE=${secure_boot_key_file} FLASH_ENCRYPTION_KEY_FILE=${flash_key_file} UPDATE_BUNDLE=${update_bundle} UPDATE_FIRMWARE=${update_firmware} UPDATE_INDEX=${update_index}"
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
