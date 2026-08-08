#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd "${script_dir}/../.." && pwd -P)"

make_cmd="${MAKE:-make}"
espflash_cmd="${ESPFLASH:-espflash}"
espefuse_cmd="${ESPEFUSE:-espefuse}"
secret_dir="${NOCKSTER_SECRET_DIR:-../nockster-secrets}"

is_true() {
  case "${1:-}" in
    1 | true | TRUE | yes | YES | on | ON) return 0 ;;
    *) return 1 ;;
  esac
}

dry_run="${PROD_E2E_DRY_RUN:-0}"

run_cmd() {
  printf '+'
  printf ' %q' "$@"
  printf '\n'
  if ! is_true "${dry_run}"; then
    "$@"
  fi
}

run_make() {
  run_cmd "${make_cmd}" "$@"
}

fail() {
  echo "flash-prod-e2e: $*" >&2
  exit 2
}

normalize_port() {
  local port="$1"
  if [[ -z "${port}" ]]; then
    fail "set PROVISION_PORT=/dev/ttyACM0 or FLASH_PORT=/dev/ttyACM0"
  fi
  if [[ "${port}" == /dev/* ]]; then
    printf '%s\n' "${port}"
  else
    printf '/dev/%s\n' "${port}"
  fi
}

block_index() {
  case "$1" in
    BLOCK_KEY0) printf '0\n' ;;
    BLOCK_KEY1) printf '1\n' ;;
    BLOCK_KEY2) printf '2\n' ;;
    BLOCK_KEY3) printf '3\n' ;;
    BLOCK_KEY4) printf '4\n' ;;
    BLOCK_KEY5) printf '5\n' ;;
    *) fail "unsupported key block: $1" ;;
  esac
}

default_digest_purpose() {
  case "$1" in
    BLOCK_KEY0) printf 'SECURE_BOOT_DIGEST0\n' ;;
    BLOCK_KEY1) printf 'SECURE_BOOT_DIGEST1\n' ;;
    BLOCK_KEY2) printf 'SECURE_BOOT_DIGEST2\n' ;;
    *) fail "set SECURE_BOOT_DIGEST_PURPOSE=SECURE_BOOT_DIGEST0|1|2 when using $1" ;;
  esac
}

check_hex64() {
  local value="$1"
  local label="$2"
  if [[ ! "${value}" =~ ^[0-9a-fA-F]{64}$ ]]; then
    fail "${label} must be exactly 64 hex characters"
  fi
}

check_crypt_cnt() {
  case "$1" in
    1 | 3 | 7 | 0x1 | 0x3 | 0x7) ;;
    *)
      fail "unsupported FLASH_CRYPT_CNT_VALUE=$1; expected 0x1, 0x3, or 0x7"
      ;;
  esac
}

require_command() {
  local cmd="$1"
  if ! command -v "${cmd}" >/dev/null 2>&1; then
    fail "missing command: ${cmd}"
  fi
}

ensure_hmac_key() {
  local path="$1"
  local size
  if [[ ! -f "${path}" ]]; then
    run_make generate-hmac-up-key HMAC_KEY_FILE="${path}"
    if is_true "${dry_run}"; then
      return
    fi
  fi
  bash "${script_dir}/check-secret-output-path.sh" "${path}" "HMAC_UP key file"
  if [[ ! -f "${path}" ]]; then
    fail "missing HMAC_UP key file: ${path}"
  fi
  size="$(wc -c <"${path}" | tr -d '[:space:]')"
  if [[ "${size}" != "32" ]]; then
    fail "HMAC_UP key file must be exactly 32 bytes, got ${size}"
  fi
}

ensure_secure_boot_key() {
  local path="$1"
  if [[ ! -f "${path}" ]]; then
    run_make generate-secure-boot-v2-key SECURE_BOOT_KEY_FILE="${path}"
    if is_true "${dry_run}"; then
      return
    fi
  fi
  bash "${script_dir}/check-secret-output-path.sh" "${path}" "secure boot v2 key file"
  bash "${script_dir}/check-secure-boot-v2-key.sh" "${path}" "secure boot v2 key file"
}

ensure_flash_encryption_key() {
  local path="$1"
  if [[ ! -f "${path}" ]]; then
    run_make generate-flash-encryption-key FLASH_ENCRYPTION_KEY_FILE="${path}"
    if is_true "${dry_run}"; then
      return
    fi
  fi
  bash "${script_dir}/check-secret-output-path.sh" "${path}" "flash encryption key file"
  bash "${script_dir}/check-flash-encryption-key.sh" "${path}" "flash encryption key file"
}

require_summary_regex() {
  local regex="$1"
  local message="$2"
  local summary_file="$3"
  if ! grep -Eq "${regex}" "${summary_file}"; then
    fail "${message}"
  fi
}

user_data_summary_line() {
  awk '/BLOCK_USR_DATA \(BLOCK3\)/ { getline; print; exit }' "$1"
}

check_user_data_empty() {
  local line
  local compact
  line="$(user_data_summary_line "$1")"
  compact="$(printf '%s' "${line}" | tr -d '[:space:]')"
  if [[ "${compact}" != "=$(printf '00%.0s' {1..32})R/W" ]]; then
    fail "BLOCK_USR_DATA is not empty and writable; refusing to alter the device serial"
  fi
}

check_device_serial_summary() {
  local summary_file="$1"
  local serial_file="$2"
  local line
  local actual_hex
  local expected_hex
  local status
  line="$(user_data_summary_line "${summary_file}")"
  actual_hex="$(printf '%s\n' "${line}" | awk '{ for (i = 2; i <= 33; i++) printf "%s", $i }')"
  status="$(printf '%s\n' "${line}" | awk '{ print $34 }')"
  expected_hex="$(od -An -tx1 -N8 "${serial_file}" | tr -d '[:space:]')"
  if [[ "${actual_hex:0:16}" != "${expected_hex}" ]] \
    || [[ "${actual_hex:16}" != "$(printf '00%.0s' {1..24})" ]]; then
    fail "BLOCK_USR_DATA does not contain the expected device serial"
  fi
  if [[ "${status}" != "R/-" ]]; then
    fail "BLOCK_USR_DATA is not write-protected"
  fi
}

check_fresh_board_summary() {
  local summary_file="$1"
  local secure_idx
  local flash_idx
  local hmac_idx

  if is_true "${dry_run}"; then
    echo "+ ${espefuse_cmd} --chip esp32s3 --port ${port} summary > ${summary_file}"
    return
  fi

  "${espefuse_cmd}" --chip esp32s3 --port "${port}" summary | tee "${summary_file}"

  check_user_data_empty "${summary_file}"

  secure_idx="$(block_index "${secure_boot_digest_block}")"
  flash_idx="$(block_index "${flash_key_block}")"
  hmac_idx="$(block_index "${hmac_block}")"

  require_summary_regex 'SECURE_BOOT_EN.*=.*False' \
    "SECURE_BOOT_EN is already burned; flash-prod-e2e is fresh-board-only" \
    "${summary_file}"
  require_summary_regex 'SPI_BOOT_CRYPT_CNT.*=.*Disable' \
    "SPI_BOOT_CRYPT_CNT is already enabled; flash-prod-e2e is fresh-board-only" \
    "${summary_file}"
  require_summary_regex "KEY_PURPOSE_${secure_idx}.*=.*USER" \
    "${secure_boot_digest_block} is not empty/USER in eFuse summary" \
    "${summary_file}"
  require_summary_regex "KEY_PURPOSE_${flash_idx}.*=.*USER" \
    "${flash_key_block} is not empty/USER in eFuse summary" \
    "${summary_file}"
  if grep -Eq "KEY_PURPOSE_${hmac_idx}.*=.*HMAC_UP" "${summary_file}"; then
    hmac_already_burned=1
    echo "${hmac_block} is already HMAC_UP; continuing without reburning HMAC_UP"
  elif grep -Eq "KEY_PURPOSE_${hmac_idx}.*=.*USER" "${summary_file}"; then
    :
  else
    fail "${hmac_block} is not empty/USER or HMAC_UP in eFuse summary"
  fi
}

resolve_cli() {
  if [[ -n "${NOCKSTER_CLI:-}" ]]; then
    printf '%s\n' "${NOCKSTER_CLI}"
    return
  fi
  if [[ -x "${repo_root}/target/x86_64-unknown-linux-gnu/release/nockster-cli" ]]; then
    printf '%s\n' "${repo_root}/target/x86_64-unknown-linux-gnu/release/nockster-cli"
    return
  fi
  if [[ -x "${repo_root}/target/release/nockster-cli" ]]; then
    printf '%s\n' "${repo_root}/target/release/nockster-cli"
    return
  fi
  if command -v nockster-cli >/dev/null 2>&1; then
    command -v nockster-cli
    return
  fi
  return 1
}

wait_for_manual_normal_boot() {
  cat <<EOF

Manual normal reboot required.

Power-cycle the board or press EN/RESET now. Let it boot normally; do not hold
BOOT/download. Wait for the HID device to re-enumerate before continuing.
EOF
  if is_true "${dry_run}"; then
    echo "+ wait for operator to manually reboot into normal HID mode"
    return
  fi
  read -r -p "Press Enter after the board has rebooted into normal/HID mode: " _
}

port="$(normalize_port "${PROVISION_PORT:-${FLASH_PORT:-/dev/ttyACM0}}")"
validate_port="${PROD_E2E_VALIDATE_PORT:-hid}"

if [[ ! -e "${port}" ]] && ! is_true "${dry_run}"; then
  fail "serial port does not exist: ${port}"
fi

release_version="${NOCKSTER_RELEASE_VERSION:-}"
if [[ -z "${release_version}" || "${release_version}" == "0" ]]; then
  release_version="${PROD_E2E_RELEASE_VERSION:-1}"
fi
if [[ ! "${release_version}" =~ ^[0-9]+$ || "${release_version}" == "0" ]]; then
  fail "NOCKSTER_RELEASE_VERSION must be a nonzero integer"
fi
if (( release_version >= 4294967295 )); then
  fail "NOCKSTER_RELEASE_VERSION must be below 4294967295 so the lockdown OTA can use the next version"
fi
ota_release_version=$((release_version + 1))

trust_hash="${NOCKSTER_UPDATE_PUBKEY_SHA256_HEX:-}"
check_hex64 "${trust_hash}" "NOCKSTER_UPDATE_PUBKEY_SHA256_HEX"

hmac_key_file="${HMAC_KEY_FILE:-${secret_dir}/hmac-up.bin}"
secure_boot_key_file="${SECURE_BOOT_KEY_FILE:-${secret_dir}/secure-boot-v2-rsa.pem}"
flash_key_file="${FLASH_ENCRYPTION_KEY_FILE:-${secret_dir}/flash-encryption-key.bin}"
update_signing_key_file="${UPDATE_SIGNING_KEY_FILE:-.secrets/update-signing.key}"

secure_boot_digest_block="${SECURE_BOOT_DIGEST_BLOCK:-BLOCK_KEY0}"
secure_boot_digest_purpose="${SECURE_BOOT_DIGEST_PURPOSE:-}"
if [[ -z "${secure_boot_digest_purpose}" ]]; then
  secure_boot_digest_purpose="$(default_digest_purpose "${secure_boot_digest_block}")"
fi

flash_key_block="${FLASH_ENCRYPTION_KEY_BLOCK:-BLOCK_KEY4}"
flash_crypt_cnt="${FLASH_CRYPT_CNT_VALUE:-0x7}"
check_crypt_cnt "${flash_crypt_cnt}"

hmac_block="BLOCK_KEY5"
hmac_already_burned=0
if [[ "${secure_boot_digest_block}" == "${hmac_block}" ]]; then
  fail "SECURE_BOOT_DIGEST_BLOCK cannot use ${hmac_block}; it is reserved for HMAC_UP"
fi
if [[ "${flash_key_block}" == "${hmac_block}" ]]; then
  fail "FLASH_ENCRYPTION_KEY_BLOCK cannot use ${hmac_block}; it is reserved for HMAC_UP"
fi
if [[ "${secure_boot_digest_block}" == "${flash_key_block}" ]]; then
  fail "SECURE_BOOT_DIGEST_BLOCK and FLASH_ENCRYPTION_KEY_BLOCK must be different"
fi

timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
artifact_dir="${PROD_E2E_ARTIFACT_DIR:-target/prod-e2e/${timestamp}}"
secure_boot_image="${SECURE_BOOT_IMAGE:-${artifact_dir}/nockster-fw.factory.bin}"
secure_boot_signed_image="${SECURE_BOOT_SIGNED_IMAGE:-${artifact_dir}/nockster-fw.factory.signed.bin}"
secure_boot_bootloader_image="${SECURE_BOOT_BOOTLOADER_IMAGE:-${artifact_dir}/bootloader.bin}"
secure_boot_partition_table_bin="${SECURE_BOOT_PARTITION_TABLE_BIN:-${artifact_dir}/partition-table.bin}"
flash_enc_bootloader_image="${FLASH_ENCRYPTION_BOOTLOADER_IMAGE:-${artifact_dir}/encrypted/bootloader.enc.bin}"
flash_enc_partition_table_bin="${FLASH_ENCRYPTION_PARTITION_TABLE_BIN:-${artifact_dir}/encrypted/partition-table.enc.bin}"
flash_enc_signed_image="${FLASH_ENCRYPTION_SIGNED_IMAGE:-${artifact_dir}/encrypted/nockster-fw.factory.signed.enc.bin}"
flash_enc_otadata_bin="${FLASH_ENCRYPTION_OTADATA_BIN:-${artifact_dir}/encrypted/otadata.blank.enc.bin}"
device_serial_bin="${DEVICE_SERIAL_BIN:-${artifact_dir}/device-serial.bin}"
ota_unsigned_image="${artifact_dir}/ota/nockster-fw.unsigned.bin"
ota_signed_image="${artifact_dir}/ota/nockster-fw.bin"
ota_bundle="${artifact_dir}/ota/nockster-fw.update.json"

mkdir -p -- \
  "${artifact_dir}" \
  "$(dirname -- "${secure_boot_image}")" \
  "$(dirname -- "${secure_boot_signed_image}")" \
  "$(dirname -- "${secure_boot_bootloader_image}")" \
  "$(dirname -- "${secure_boot_partition_table_bin}")" \
  "$(dirname -- "${flash_enc_bootloader_image}")" \
  "$(dirname -- "${flash_enc_partition_table_bin}")" \
  "$(dirname -- "${flash_enc_signed_image}")" \
  "$(dirname -- "${flash_enc_otadata_bin}")" \
  "$(dirname -- "${device_serial_bin}")" \
  "$(dirname -- "${ota_unsigned_image}")"

require_command "${espflash_cmd}"
require_command "${espefuse_cmd}"
require_command python3

echo "flash-prod-e2e production flow"
echo "port: ${port}"
echo "artifacts: ${artifact_dir}"
echo "factory release: ${release_version}"
echo "lockdown OTA release: ${ota_release_version}"

check_fresh_board_summary "${artifact_dir}/efuse-summary.initial.txt"

ensure_hmac_key "${hmac_key_file}"
ensure_secure_boot_key "${secure_boot_key_file}"
ensure_flash_encryption_key "${flash_key_file}"

run_make update-firmware-image \
  FW_PROFILE=production \
  ALLOW_UNSIGNED_PRODUCTION=1 \
  NOCKSTER_RELEASE_VERSION="${release_version}" \
  NOCKSTER_UPDATE_PUBKEY_SHA256_HEX="${trust_hash}" \
  UPDATE_FIRMWARE="${secure_boot_image}"

run_make release-sign-secure-boot-v2 \
  SECURE_BOOT_KEY_FILE="${secure_boot_key_file}" \
  SECURE_BOOT_IMAGE="${secure_boot_image}" \
  SECURE_BOOT_SIGNED_IMAGE="${secure_boot_signed_image}"

run_make release-build-secure-boot-v2-bootloader \
  SECURE_BOOT_KEY_FILE="${secure_boot_key_file}" \
  SECURE_BOOT_BOOTLOADER_FLASH_ENCRYPTION=1 \
  SECURE_BOOT_BOOTLOADER_IMAGE="${secure_boot_bootloader_image}" \
  SECURE_BOOT_PARTITION_TABLE_BIN="${secure_boot_partition_table_bin}"

run_make release-encrypt-flash-v2-artifacts \
  FLASH_ENCRYPTION_KEY_FILE="${flash_key_file}" \
  SECURE_BOOT_KEY_FILE="${secure_boot_key_file}" \
  SECURE_BOOT_BOOTLOADER_IMAGE="${secure_boot_bootloader_image}" \
  SECURE_BOOT_PARTITION_TABLE_BIN="${secure_boot_partition_table_bin}" \
  SECURE_BOOT_SIGNED_IMAGE="${secure_boot_signed_image}" \
  FLASH_ENCRYPTION_BOOTLOADER_IMAGE="${flash_enc_bootloader_image}" \
  FLASH_ENCRYPTION_PARTITION_TABLE_BIN="${flash_enc_partition_table_bin}" \
  FLASH_ENCRYPTION_SIGNED_IMAGE="${flash_enc_signed_image}" \
  FLASH_ENCRYPTION_OTADATA_BIN="${flash_enc_otadata_bin}"

# Build the exact OTA that will complete production lockdown before asking for
# permission to burn anything. check-update-trust verifies that its signing key
# matches the trust hash compiled into both production images.
run_make signed-update-secure-boot-v2 \
  FW_PROFILE=production \
  ALLOW_UNSIGNED_PRODUCTION=1 \
  NOCKSTER_RELEASE_VERSION="${ota_release_version}" \
  NOCKSTER_UPDATE_PUBKEY_SHA256_HEX="${trust_hash}" \
  UPDATE_SIGNING_KEY_FILE="${update_signing_key_file}" \
  SECURE_BOOT_KEY_FILE="${secure_boot_key_file}" \
  UPDATE_UNSIGNED_FIRMWARE="${ota_unsigned_image}" \
  UPDATE_FIRMWARE="${ota_signed_image}" \
  UPDATE_BUNDLE="${ota_bundle}"

check_fresh_board_summary "${artifact_dir}/efuse-summary.before-burn.txt"

if [[ -e "${device_serial_bin}" || -e "${artifact_dir}/device-serial.txt" ]]; then
  fail "device serial artifact already exists; choose a fresh PROD_E2E_ARTIFACT_DIR"
fi

if [[ "${hmac_already_burned}" == "1" ]]; then
  hmac_action="skip HMAC_UP burn; ${hmac_block} is already HMAC_UP"
else
  hmac_action="burn HMAC_UP into ${hmac_block} from ${hmac_key_file}"
fi

cat <<EOF

About to irreversibly provision ${port}.

This single confirmation will:
  - ${hmac_action}
  - flash signed plaintext secure-boot artifacts without reset
  - burn ${secure_boot_digest_purpose} into ${secure_boot_digest_block}
  - burn XTS_AES_128_KEY into ${flash_key_block} from ${flash_key_file}
  - burn SECURE_BOOT_EN
  - flash encrypted signed production artifacts without reset
  - burn SPI_BOOT_CRYPT_CNT=${flash_crypt_cnt}
  - burn the flash-time Unix timestamp into BLOCK_USR_DATA and write-protect it
  - pause for one manual normal boot and validate the factory image over ${validate_port}
  - install signed production OTA release ${ota_release_version} and reboot automatically
  - burn and verify JTAG, download-mode, direct-boot, ROM-print, and glitch-protection lockdown
  - require the complete production security report before declaring success

Do not interrupt the board after confirming.
EOF

if is_true "${dry_run}"; then
  echo "dry-run: skipping interactive confirmation and irreversible writes"
else
  read -r -p "Type FLASH-PROD-E2E to continue: " confirmation
  if [[ "${confirmation}" != "FLASH-PROD-E2E" ]]; then
    echo "aborted"
    exit 1
  fi
fi

# Capture the instant the production flash begins. This value becomes the
# device's immutable USB serial once BLOCK_USR_DATA is write-protected below.
flash_unix_timestamp="${DEVICE_SERIAL_UNIX_TIMESTAMP:-$(date -u +%s)}"
if [[ ! "${flash_unix_timestamp}" =~ ^[0-9]+$ ]] \
  || (( flash_unix_timestamp < 1577836800 || flash_unix_timestamp > 4294967295 )); then
  fail "DEVICE_SERIAL_UNIX_TIMESTAMP must be a Unix timestamp between 1577836800 and 4294967295"
fi
python3 - "${device_serial_bin}" "${flash_unix_timestamp}" <<'PY'
import pathlib
import struct
import sys

pathlib.Path(sys.argv[1]).write_bytes(struct.pack("<Q24x", int(sys.argv[2])))
PY
printf '%s\n' "${flash_unix_timestamp}" >"${artifact_dir}/device-serial.txt"
echo "device serial (flash Unix timestamp): ${flash_unix_timestamp}"

if [[ "${hmac_already_burned}" == "1" ]]; then
  echo "+ skip HMAC_UP burn; ${hmac_block} is already HMAC_UP"
else
  run_cmd "${espefuse_cmd}" --chip esp32s3 --port "${port}" --do-not-confirm \
    burn-key "${hmac_block}" "${hmac_key_file}" HMAC_UP
fi

run_make flash-secure-boot-v2 \
  FLASH_PORT="${port}" \
  SECURE_BOOT_KEY_FILE="${secure_boot_key_file}" \
  SECURE_BOOT_SIGNED_IMAGE="${secure_boot_signed_image}" \
  SECURE_BOOT_BOOTLOADER_IMAGE="${secure_boot_bootloader_image}" \
  SECURE_BOOT_PARTITION_TABLE_BIN="${secure_boot_partition_table_bin}"

run_cmd "${espefuse_cmd}" --chip esp32s3 --port "${port}" --before no-reset --do-not-confirm \
  burn-key-digest "${secure_boot_digest_block}" "${secure_boot_key_file}" "${secure_boot_digest_purpose}"

run_cmd "${espefuse_cmd}" --chip esp32s3 --port "${port}" --before no-reset --do-not-confirm \
  burn-key "${flash_key_block}" "${flash_key_file}" XTS_AES_128_KEY

run_cmd "${espefuse_cmd}" --chip esp32s3 --port "${port}" --before no-reset --do-not-confirm \
  burn-efuse SECURE_BOOT_EN 1

run_make flash-encrypted-secure-boot-v2 \
  FLASH_PORT="${port}" \
  FLASH_ENCRYPTION_BOOTLOADER_IMAGE="${flash_enc_bootloader_image}" \
  FLASH_ENCRYPTION_PARTITION_TABLE_BIN="${flash_enc_partition_table_bin}" \
  FLASH_ENCRYPTION_SIGNED_IMAGE="${flash_enc_signed_image}" \
  FLASH_ENCRYPTION_OTADATA_BIN="${flash_enc_otadata_bin}"

run_cmd "${espefuse_cmd}" --chip esp32s3 --port "${port}" --before no-reset --do-not-confirm \
  burn-efuse SPI_BOOT_CRYPT_CNT "${flash_crypt_cnt}"

run_cmd "${espefuse_cmd}" --chip esp32s3 --port "${port}" --before no-reset --do-not-confirm \
  burn-block-data BLOCK_USR_DATA "${device_serial_bin}"

run_cmd "${espefuse_cmd}" --chip esp32s3 --port "${port}" --before no-reset --do-not-confirm \
  write-protect-efuse BLOCK_USR_DATA

if is_true "${dry_run}"; then
  echo "+ ${espefuse_cmd} --chip esp32s3 --port ${port} --before no-reset summary > ${artifact_dir}/efuse-summary.serial.txt"
else
  "${espefuse_cmd}" --chip esp32s3 --port "${port}" --before no-reset summary \
    | tee "${artifact_dir}/efuse-summary.serial.txt"
  check_device_serial_summary "${artifact_dir}/efuse-summary.serial.txt" "${device_serial_bin}"
fi

wait_for_manual_normal_boot

if cli="$(resolve_cli)"; then
  if is_true "${dry_run}"; then
    echo "+ ${cli} security --port ${validate_port} --expect-chip-security --expect-hmac-up --expect-hmac-up-read-protected --expect-secure-boot --expect-flash-encryption"
    echo "+ ${cli} list-ports | verify HID serial ${flash_unix_timestamp}"
    echo "+ ${cli} update device-install --port ${validate_port} --bundle ${ota_bundle} --firmware ${ota_signed_image} --chunk-size 256 --reboot"
    echo "+ ${cli} security --port ${validate_port}"
    echo "+ ${cli} list-ports | verify HID serial ${flash_unix_timestamp}"
  else
    factory_validated=0
    for attempt in $(seq 1 12); do
      if "${cli}" security \
        --port "${validate_port}" \
        --expect-chip-security \
        --expect-hmac-up \
        --expect-hmac-up-read-protected \
        --expect-secure-boot \
        --expect-flash-encryption; then
        ports_output="$(NO_COLOR=1 "${cli}" list-ports)"
        if printf '%s\n' "${ports_output}" \
          | grep -Eq "hid:303a:2001.*${flash_unix_timestamp}([[:space:]]|$)"; then
          factory_validated=1
          break
        fi
        echo "security checks passed but HID serial ${flash_unix_timestamp} was not found" >&2
      fi
      echo "waiting for HID validation (${attempt}/12)..."
      sleep 2
    done
    if [[ "${factory_validated}" != "1" ]]; then
      fail "factory image did not pass pre-lockdown validation over ${validate_port}"
    fi

    echo "factory image verified; installing signed lockdown OTA release ${ota_release_version}"
    "${cli}" update device-install \
      --port "${validate_port}" \
      --bundle "${ota_bundle}" \
      --firmware "${ota_signed_image}" \
      --chunk-size 256 \
      --reboot

    echo "waiting for the locked HID device to return; if it does not, power-cycle it normally without holding BOOT"
    lockdown_validated=0
    for attempt in $(seq 1 30); do
      if "${cli}" security --port "${validate_port}"; then
        ports_output="$(NO_COLOR=1 "${cli}" list-ports)"
        if printf '%s\n' "${ports_output}" \
          | grep -Eq "hid:303a:2001.*${flash_unix_timestamp}([[:space:]]|$)"; then
          lockdown_validated=1
          break
        fi
        echo "lockdown checks passed but HID serial ${flash_unix_timestamp} was not found" >&2
      fi
      echo "waiting for locked production OTA (${attempt}/30)..."
      sleep 2
    done
    if [[ "${lockdown_validated}" != "1" ]]; then
      fail "OTA image did not pass complete production lockdown validation over ${validate_port}"
    fi

    echo "flash-prod-e2e complete; OTA release ${ota_release_version}, full lockdown, and immutable serial ${flash_unix_timestamp} verified"
    exit 0
  fi
else
  fail "nockster-cli not found; refusing to skip post-provision HID and serial validation"
fi

echo "flash-prod-e2e dry-run complete; no device or eFuses were changed"
