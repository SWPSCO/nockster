#!/usr/bin/env bash
set -euo pipefail

failures=0
warnings=0

ok() {
  printf 'OK   %s\n' "$*"
}

info() {
  printf 'INFO %s\n' "$*"
}

warn() {
  warnings=$((warnings + 1))
  printf 'WARN %s\n' "$*" >&2
}

fail() {
  failures=$((failures + 1))
  printf 'FAIL %s\n' "$*" >&2
}

is_true() {
  case "${1:-}" in
    1 | true | TRUE | yes | YES | on | ON) return 0 ;;
    *) return 1 ;;
  esac
}

absolute_path() {
  local path="$1"
  local dir
  local base
  dir="$(dirname -- "${path}")"
  base="$(basename -- "${path}")"
  if [[ ! -d "${dir}" ]]; then
    return 1
  fi
  (cd "${dir}" && printf '%s/%s\n' "$(pwd -P)" "${base}")
}

file_mode() {
  local path="$1"
  stat -c '%a' "${path}" 2>/dev/null || stat -f '%Lp' "${path}" 2>/dev/null || true
}

check_hex64() {
  local value="$1"
  local label="$2"
  local required="$3"

  if [[ -z "${value}" ]]; then
    if [[ "${required}" == "required" ]]; then
      fail "${label} is required"
    else
      warn "${label} is not configured"
    fi
    return
  fi

  if [[ "${value}" =~ ^[0-9a-fA-F]{64}$ ]]; then
    ok "${label} is a 32-byte hex value"
  else
    fail "${label} must be exactly 64 hex characters"
  fi
}

check_release_version() {
  local value="$1"
  local required="$2"

  if [[ -z "${value}" ]]; then
    if [[ "${required}" == "required" ]]; then
      fail "NOCKSTER_RELEASE_VERSION is required"
    else
      warn "NOCKSTER_RELEASE_VERSION is not set"
    fi
    return
  fi

  if [[ ! "${value}" =~ ^[0-9]+$ ]]; then
    fail "NOCKSTER_RELEASE_VERSION must be numeric"
    return
  fi

  if [[ "${value}" == "0" ]]; then
    if [[ "${required}" == "required" ]]; then
      fail "NOCKSTER_RELEASE_VERSION must be greater than zero for strict release preflight"
    else
      warn "NOCKSTER_RELEASE_VERSION is 0; signed updates from this build require a higher bundle counter"
    fi
  else
    ok "NOCKSTER_RELEASE_VERSION=${value}"
  fi
}

check_outside_repo() {
  local path="$1"
  local label="$2"
  local abs

  if ! abs="$(absolute_path "${path}")"; then
    warn "could not resolve ${label} path: ${path}"
    return
  fi

  case "${abs}" in
    "${repo_root}" | "${repo_root}"/*)
      fail "${label} must live outside the repo: ${abs}"
      ;;
    *)
      ok "${label} is outside the repo"
      ;;
  esac
}

check_secret_permissions() {
  local path="$1"
  local label="$2"
  local mode
  local mode3
  local perm

  mode="$(file_mode "${path}")"
  if [[ -z "${mode}" ]]; then
    warn "could not inspect permissions for ${label}: ${path}"
    return
  fi

  mode3="${mode: -3}"
  if [[ ! "${mode3}" =~ ^[0-7]{3}$ ]]; then
    warn "unexpected permission mode for ${label}: ${mode}"
    return
  fi

  perm=$((8#${mode3}))
  if (( (perm & 077) != 0 )); then
    fail "${label} permissions are too broad (${mode3}); use 0600 or 0400"
  else
    ok "${label} permissions are restricted (${mode3})"
  fi
}

check_hmac_key_file() {
  local path="$1"
  local required="$2"
  local size

  if [[ -z "${path}" ]]; then
    if [[ "${required}" == "required" ]]; then
      fail "HMAC_KEY_FILE is required"
    else
      warn "HMAC_KEY_FILE is not set; skipping HMAC_UP key-file checks"
    fi
    return
  fi

  if [[ ! -f "${path}" ]]; then
    fail "missing HMAC_UP key file: ${path}"
    return
  fi

  check_outside_repo "${path}" "HMAC_UP key file"
  check_secret_permissions "${path}" "HMAC_UP key file"

  size="$(wc -c <"${path}" | tr -d '[:space:]')"
  if [[ "${size}" == "32" ]]; then
    ok "HMAC_UP key file is 32 bytes"
  else
    fail "HMAC_UP key file must be exactly 32 bytes, got ${size}"
  fi
}

check_update_signing_key_file() {
  local path="$1"
  local required="$2"
  local raw_size
  local hex_size

  if [[ -z "${path}" ]]; then
    if [[ "${required}" == "required" ]]; then
      fail "UPDATE_SIGNING_KEY_FILE is required"
    else
      warn "UPDATE_SIGNING_KEY_FILE is not set; skipping release signing key checks"
    fi
    return
  fi

  if [[ ! -f "${path}" ]]; then
    fail "missing release signing key file: ${path}"
    return
  fi

  check_outside_repo "${path}" "release signing key file"
  check_secret_permissions "${path}" "release signing key file"

  raw_size="$(wc -c <"${path}" | tr -d '[:space:]')"
  hex_size="$(tr -d '[:space:]' <"${path}" | wc -c | tr -d '[:space:]')"
  if [[ "${raw_size}" == "32" ]]; then
    ok "release signing key file is raw 32-byte format"
  elif [[ "${hex_size}" == "64" ]] && tr -d '[:space:]' <"${path}" | LC_ALL=C grep -Eq '^[0-9a-fA-F]{64}$'; then
    ok "release signing key file is hex 32-byte format"
  else
    fail "release signing key file must be raw 32 bytes or 64 hex characters"
  fi
}

check_update_signing_key_trust_hash() {
  local path="$1"
  local trusted_hash="$2"
  local required="$3"
  local cli
  local output
  local derived_hash
  local derived_hash_lower
  local trusted_hash_lower

  if [[ -z "${path}" || -z "${trusted_hash}" ]]; then
    return
  fi

  if [[ ! -f "${path}" ]]; then
    return
  fi

  if ! cli="$(resolve_cli "${NOCKSTER_CLI:-}")"; then
    if [[ "${required}" == "required" ]]; then
      fail "nockster-cli is required to validate UPDATE_SIGNING_KEY_FILE against NOCKSTER_UPDATE_PUBKEY_SHA256_HEX"
    else
      warn "nockster-cli is unavailable; skipping release signing key trust-hash check"
    fi
    return
  fi

  if ! output="$("${cli}" update pubkey --signing-key-file "${path}" 2>&1)"; then
    fail "could not derive release signing key public hash"
    printf '%s\n' "${output}" >&2
    return
  fi

  derived_hash="$(printf '%s\n' "${output}" | awk '/^trusted_pubkey_sha256:/ {print $2; exit}')"
  if [[ -z "${derived_hash}" ]]; then
    fail "could not parse trusted_pubkey_sha256 from nockster-cli update pubkey output"
    printf '%s\n' "${output}" >&2
    return
  fi

  derived_hash_lower="$(printf '%s' "${derived_hash}" | tr '[:upper:]' '[:lower:]')"
  trusted_hash_lower="$(printf '%s' "${trusted_hash}" | tr '[:upper:]' '[:lower:]')"
  if [[ "${derived_hash_lower}" == "${trusted_hash_lower}" ]]; then
    ok "release signing key matches NOCKSTER_UPDATE_PUBKEY_SHA256_HEX"
  else
    fail "release signing key does not match NOCKSTER_UPDATE_PUBKEY_SHA256_HEX"
    printf 'derived:    %s\nconfigured: %s\n' "${derived_hash}" "${trusted_hash}" >&2
  fi
}

check_secure_boot_key_file() {
  local path="$1"
  local required="$2"

  if [[ -z "${path}" ]]; then
    if [[ "${required}" == "required" ]]; then
      fail "SECURE_BOOT_KEY_FILE is required"
    else
      warn "SECURE_BOOT_KEY_FILE is not set; skipping secure boot key checks"
    fi
    return
  fi

  if [[ ! -f "${path}" ]]; then
    fail "missing secure boot signing key file: ${path}"
    return
  fi

  check_outside_repo "${path}" "secure boot signing key file"
  check_secret_permissions "${path}" "secure boot signing key file"

  if LC_ALL=C grep -q 'PRIVATE KEY' "${path}"; then
    ok "secure boot signing key looks like a PEM private key"
  else
    warn "secure boot signing key does not look like a PEM private key"
  fi
}

check_flash_encryption_key_file() {
  local path="$1"
  local required="$2"
  local size

  if [[ -z "${path}" ]]; then
    if [[ "${required}" == "required" ]]; then
      fail "FLASH_ENCRYPTION_KEY_FILE is required"
    else
      warn "FLASH_ENCRYPTION_KEY_FILE is not set; skipping flash encryption key checks"
    fi
    return
  fi

  if [[ ! -f "${path}" ]]; then
    fail "missing flash encryption key file: ${path}"
    return
  fi

  check_outside_repo "${path}" "flash encryption key file"
  check_secret_permissions "${path}" "flash encryption key file"

  size="$(wc -c <"${path}" | tr -d '[:space:]')"
  if [[ "${size}" == "32" ]]; then
    ok "flash encryption key file is 32 bytes"
  else
    fail "flash encryption key file must be exactly 32 bytes, got ${size}"
  fi
}

partition_row() {
  local table="$1"
  local wanted="$2"

  awk -F, -v wanted="${wanted}" '
    function trim(value) {
      gsub(/^[ \t]+|[ \t]+$/, "", value)
      return value
    }
    /^[ \t]*#/ || /^[ \t]*$/ { next }
    {
      for (i = 1; i <= 6; i++) {
        field[i] = trim($i)
      }
      if (field[1] == wanted) {
        printf "%s\t%s\t%s\t%s\t%s\t%s\n", field[1], field[2], field[3], field[4], field[5], field[6]
        found = 1
        exit
      }
    }
    END {
      if (!found) {
        exit 1
      }
    }
  ' "${table}"
}

partition_size_bytes() {
  local value="$1"
  local compact
  local suffix
  local number
  local multiplier=1

  compact="$(printf '%s' "${value}" | tr -d '[:space:]')"
  suffix="${compact: -1}"
  case "${suffix}" in
    K|k)
      multiplier=1024
      number="${compact%?}"
      ;;
    M|m)
      multiplier=$((1024 * 1024))
      number="${compact%?}"
      ;;
    *)
      number="${compact}"
      ;;
  esac

  if [[ "${number}" =~ ^0[xX][0-9a-fA-F]+$ ]]; then
    printf '%d\n' "$((number * multiplier))"
  elif [[ "${number}" =~ ^[0-9]+$ ]]; then
    printf '%d\n' "$((10#${number} * multiplier))"
  else
    return 1
  fi
}

check_partition() {
  local table="$1"
  local name="$2"
  local expected_type="$3"
  local expected_subtype="$4"
  local row
  local part_name
  local part_type
  local part_subtype
  local part_offset
  local part_size
  local part_flags

  if ! row="$(partition_row "${table}" "${name}")"; then
    fail "partition table is missing required partition: ${name}"
    return 1
  fi

  IFS=$'\t' read -r part_name part_type part_subtype part_offset part_size part_flags <<<"${row}"
  if [[ "${part_type}" != "${expected_type}" || "${part_subtype}" != "${expected_subtype}" ]]; then
    fail "partition ${name} has type/subtype ${part_type}/${part_subtype}, expected ${expected_type}/${expected_subtype}"
    return 1
  fi

  ok "partition ${name} is ${part_type}/${part_subtype} at ${part_offset:-unknown} size ${part_size:-unknown}"
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
    "${part_name}" "${part_type}" "${part_subtype}" "${part_offset}" "${part_size}" "${part_flags}"
}

check_partition_table() {
  local table="$1"
  local required="$2"
  local nvs_row
  local factory_row
  local ota0_row
  local ota1_row
  local _name
  local _type
  local _subtype
  local offset
  local size
  local flags
  local factory_size
  local ota0_size
  local ota1_size
  local app_slot_limit="${NOCKSTER_APP_SLOT_SIZE_BYTES:-3145728}"
  local flags_lower

  if [[ -z "${table}" ]]; then
    table="${repo_root}/partitions.csv"
  elif [[ "${table}" != /* ]]; then
    table="${repo_root}/${table}"
  fi

  if [[ ! -f "${table}" ]]; then
    if [[ "${required}" == "required" ]]; then
      fail "partition table is required but missing: ${table}"
    else
      warn "partition table is missing; skipping partition-layout checks: ${table}"
    fi
    return
  fi

  info "checking partition table: ${table}"

  if ! nvs_row="$(check_partition "${table}" nvs data nvs)"; then
    return
  fi
  IFS=$'\t' read -r _name _type _subtype offset size flags <<<"$(printf '%s\n' "${nvs_row}" | tail -n 1)"
  flags_lower="$(printf '%s' "${flags}" | tr '[:upper:]' '[:lower:]')"
  if [[ "${flags_lower}" == *encrypted* ]]; then
    if is_true "${NVS_PARTITION_ENCRYPTION_VALIDATED:-0}"; then
      warn "NVS partition is marked encrypted; assuming raw NVS I/O was validated on this board"
    else
      fail "NVS partition is marked encrypted before validation; set NVS_PARTITION_ENCRYPTION_VALIDATED=1 only after raw NVS read/write testing"
    fi
  else
    ok "NVS partition is not marked with flash-encryption flags"
  fi

  if ! factory_row="$(check_partition "${table}" factory app factory)"; then
    return
  fi
  if ! check_partition "${table}" otadata data ota >/dev/null; then
    return
  fi
  if ! ota0_row="$(check_partition "${table}" ota_0 app ota_0)"; then
    return
  fi
  if ! ota1_row="$(check_partition "${table}" ota_1 app ota_1)"; then
    return
  fi

  IFS=$'\t' read -r _name _type _subtype offset size flags <<<"$(printf '%s\n' "${factory_row}" | tail -n 1)"
  if [[ "${offset}" != "0x10000" ]]; then
    fail "factory app partition offset is ${offset}, expected 0x10000 for serial flashing"
  else
    ok "factory app partition starts at 0x10000"
  fi
  if ! factory_size="$(partition_size_bytes "${size}")"; then
    fail "factory app partition size is invalid: ${size}"
    factory_size=0
  fi

  IFS=$'\t' read -r _name _type _subtype _offset size _flags <<<"$(printf '%s\n' "${ota0_row}" | tail -n 1)"
  if ! ota0_size="$(partition_size_bytes "${size}")"; then
    fail "ota_0 app partition size is invalid: ${size}"
    ota0_size=0
  fi
  IFS=$'\t' read -r _name _type _subtype _offset size _flags <<<"$(printf '%s\n' "${ota1_row}" | tail -n 1)"
  if ! ota1_size="$(partition_size_bytes "${size}")"; then
    fail "ota_1 app partition size is invalid: ${size}"
    ota1_size=0
  fi

  if (( ota0_size > 0 && ota1_size > 0 && ota0_size != ota1_size )); then
    fail "OTA app partition sizes differ: ota_0=${ota0_size}, ota_1=${ota1_size}"
  elif (( ota0_size > 0 && ota1_size > 0 )); then
    ok "OTA app partition sizes match (${ota0_size} bytes)"
  fi

  if [[ ! "${app_slot_limit}" =~ ^[0-9]+$ ]]; then
    fail "NOCKSTER_APP_SLOT_SIZE_BYTES must be numeric, got ${app_slot_limit}"
  elif (( ota0_size > 0 && app_slot_limit > ota0_size )); then
    fail "NOCKSTER_APP_SLOT_SIZE_BYTES=${app_slot_limit} exceeds ota_0 partition size ${ota0_size}"
  elif (( app_slot_limit > 0 )); then
    ok "configured app image limit is ${app_slot_limit} bytes"
  fi

  if (( factory_size > 0 && ota0_size > 0 && factory_size != ota0_size )); then
    warn "factory app partition size (${factory_size}) differs from OTA slot size (${ota0_size})"
  fi
}

check_release_image_file() {
  local path="$1"
  local label="$2"
  local required="$3"
  local checker="${repo_root}/scripts/provision/check-release-image.sh"
  local output

  if [[ -z "${path}" ]]; then
    if [[ "${required}" == "required" ]]; then
      fail "${label} is required"
    else
      warn "${label} is not set; skipping ESP app image checks"
    fi
    return
  fi

  if [[ ! -f "${path}" ]]; then
    fail "missing ${label}: ${path}"
    return
  fi

  if [[ ! -x "${checker}" ]]; then
    fail "missing release image checker: ${checker}"
    return
  fi

  if output="$("${checker}" "${path}" "${NOCKSTER_APP_SLOT_SIZE_BYTES:-3145728}" "${label}" 2>&1)"; then
    ok "${label} looks like an ESP app image and fits the app slot"
  else
    fail "${label} failed ESP app image sanity checks"
    printf '%s\n' "${output}" >&2
  fi
}

check_secure_boot_image_paths() {
  local input_image="$1"
  local signed_image="$2"

  if [[ -z "${input_image}" && -z "${signed_image}" ]]; then
    return
  fi

  if [[ -z "${input_image}" || -z "${signed_image}" ]]; then
    fail "SECURE_BOOT_IMAGE and SECURE_BOOT_SIGNED_IMAGE must be provided together"
    return
  fi

  if [[ "${input_image}" == "${signed_image}" ]]; then
    fail "SECURE_BOOT_SIGNED_IMAGE must be distinct from SECURE_BOOT_IMAGE"
  else
    ok "secure boot signed image output is distinct from input"
  fi

  local signed_dir
  signed_dir="$(dirname -- "${signed_image}")"
  if [[ -d "${signed_dir}" ]]; then
    ok "secure boot signed image output directory exists"
  else
    fail "secure boot signed image output directory does not exist: ${signed_dir}"
  fi
}

check_tracked_secret_paths() {
  local tracked

  if ! command -v git >/dev/null 2>&1; then
    warn "git is unavailable; skipping tracked secret path scan"
    return
  fi

  tracked="$(
    git ls-files -- \
      '*.seed' \
      '*.pem' \
      '*.key' \
      '*.wallet' \
      '*.psnt' \
      'release-signing-key*.hex' \
      'update-signing-key*.hex' \
      '*secure-boot*.pem' \
      'flash-encryption-key*.bin' \
      '.envrc.secret' \
      2>/dev/null || true
  )"

  if [[ -n "${tracked}" ]]; then
    fail "tracked secret-looking paths found:"
    printf '%s\n' "${tracked}" >&2
  else
    ok "no tracked secret-looking paths"
  fi
}

resolve_cli() {
  local configured="$1"
  if [[ -n "${configured}" ]]; then
    printf '%s\n' "${configured}"
    return
  fi
  if [[ -x target/x86_64-unknown-linux-gnu/release/nockster-cli ]]; then
    printf '%s\n' "target/x86_64-unknown-linux-gnu/release/nockster-cli"
    return
  fi
  if [[ -x target/release/nockster-cli ]]; then
    printf '%s\n' "target/release/nockster-cli"
    return
  fi
  if command -v nockster-cli >/dev/null 2>&1; then
    command -v nockster-cli
    return
  fi
  return 1
}

check_update_bundle() {
  local bundle="$1"
  local firmware="$2"
  local trusted_hash="$3"
  local required="$4"
  local cli

  if [[ -z "${bundle}" && -z "${firmware}" ]]; then
    if [[ "${required}" == "required" ]]; then
      fail "UPDATE_BUNDLE and UPDATE_FIRMWARE are required for strict release preflight"
    else
      warn "UPDATE_BUNDLE/UPDATE_FIRMWARE are not set; skipping signed update bundle verification"
    fi
    return
  fi

  if [[ -z "${bundle}" || -z "${firmware}" ]]; then
    fail "UPDATE_BUNDLE and UPDATE_FIRMWARE must be provided together"
    return
  fi

  if [[ ! -f "${bundle}" ]]; then
    fail "missing update bundle: ${bundle}"
    return
  fi

  if [[ ! -f "${firmware}" ]]; then
    fail "missing firmware image: ${firmware}"
    return
  fi

  if [[ -z "${trusted_hash}" ]]; then
    fail "NOCKSTER_UPDATE_PUBKEY_SHA256_HEX is required to verify the update bundle"
    return
  fi

  if ! cli="$(resolve_cli "${NOCKSTER_CLI:-}")"; then
    fail "nockster-cli is required to verify UPDATE_BUNDLE"
    return
  fi

  info "verifying signed update bundle with ${cli}"
  local output
  if output="$(
    "${cli}" update verify \
      --bundle "${bundle}" \
      --firmware "${firmware}" \
      --trusted-pubkey-sha256 "${trusted_hash}" \
      2>&1
  )"; then
    ok "signed update bundle verifies against configured trust anchor"
  else
    fail "signed update bundle failed verification against configured trust anchor"
    printf '%s\n' "${output}" >&2
  fi
}

check_update_index() {
  local index="$1"
  local bundle="$2"
  local firmware="$3"
  local cli
  local tmp
  local output
  local args

  if [[ -z "${index}" ]]; then
    info "UPDATE_INDEX is not set; skipping browser release-index publication check"
    return
  fi

  if [[ -z "${bundle}" || -z "${firmware}" ]]; then
    fail "UPDATE_INDEX validation requires UPDATE_BUNDLE and UPDATE_FIRMWARE"
    return
  fi

  if [[ ! -f "${index}" ]]; then
    fail "missing browser release index: ${index}"
    return
  fi

  if [[ ! -f "${bundle}" ]]; then
    fail "missing update bundle for release-index validation: ${bundle}"
    return
  fi

  if [[ ! -f "${firmware}" ]]; then
    fail "missing firmware image for release-index validation: ${firmware}"
    return
  fi

  if ! cli="$(resolve_cli "${NOCKSTER_CLI:-}")"; then
    fail "nockster-cli is required to validate UPDATE_INDEX"
    return
  fi

  if ! tmp="$(mktemp "${TMPDIR:-/tmp}/nockster-release-index.XXXXXX.json")"; then
    fail "could not create temporary release-index file"
    return
  fi

  args=(update index --bundle "${bundle}" --firmware "${firmware}" --out "${tmp}")
  if [[ -n "${UPDATE_BUNDLE_URL:-}" ]]; then
    args+=(--bundle-url "${UPDATE_BUNDLE_URL}")
  fi
  if [[ -n "${UPDATE_FIRMWARE_URL:-}" ]]; then
    args+=(--firmware-url "${UPDATE_FIRMWARE_URL}")
  fi

  info "validating browser release index with ${cli}"
  if ! output="$("${cli}" "${args[@]}" 2>&1)"; then
    fail "could not regenerate expected browser release index"
    printf '%s\n' "${output}" >&2
    rm -f "${tmp}"
    return
  fi

  if cmp -s "${tmp}" "${index}"; then
    ok "browser release index matches signed bundle and firmware"
    rm -f "${tmp}"
    return
  fi

  fail "browser release index does not match signed bundle, firmware, or configured artifact URLs"
  if command -v diff >/dev/null 2>&1; then
    diff -u "${tmp}" "${index}" >&2 || true
  else
    printf 'expected index was generated at: %s\n' "${tmp}" >&2
    tmp=""
  fi
  if [[ -n "${tmp}" ]]; then
    rm -f "${tmp}"
  fi
}

check_tools() {
  local required="$1"
  local espsecure_cmd="${ESPSECURE:-espsecure}"
  local espefuse_cmd="${ESPEFUSE:-espefuse}"

  if command -v "${espsecure_cmd}" >/dev/null 2>&1; then
    ok "espsecure is available: ${espsecure_cmd}"
  elif [[ "${required}" == "required" ]]; then
    fail "espsecure is required for strict/production release preflight but is unavailable: ${espsecure_cmd}"
  else
    warn "espsecure is unavailable (${espsecure_cmd}); secure boot signing helpers will not run"
  fi

  if command -v "${espefuse_cmd}" >/dev/null 2>&1; then
    ok "espefuse is available: ${espefuse_cmd}"
  elif [[ "${required}" == "required" ]]; then
    fail "espefuse is required for strict/production release preflight but is unavailable: ${espefuse_cmd}"
  else
    warn "espefuse is unavailable (${espefuse_cmd}); eFuse summary/provisioning helpers will not run"
  fi
}

maybe_run_efuse_summary() {
  local port="$1"
  local espefuse_cmd="${ESPEFUSE:-espefuse}"

  if ! is_true "${RUN_EFUSE_SUMMARY:-0}"; then
    info "skipping eFuse summary; set RUN_EFUSE_SUMMARY=1 PROVISION_PORT=/dev/ttyACM0 for read-only chip status"
    return
  fi

  if [[ -z "${port}" ]]; then
    fail "RUN_EFUSE_SUMMARY=1 requires PROVISION_PORT=/dev/ttyACM0"
    return
  fi

  if ! command -v "${espefuse_cmd}" >/dev/null 2>&1; then
    fail "cannot run eFuse summary; missing ${espefuse_cmd}"
    return
  fi

  info "running read-only eFuse summary on ${port}"
  "${espefuse_cmd}" --chip esp32s3 --port "${port}" summary
  ok "read-only eFuse summary completed"
}

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd -P)"
profile="${FW_PROFILE:-dev}"
release_version="${NOCKSTER_RELEASE_VERSION:-0}"
trusted_hash="${NOCKSTER_UPDATE_PUBKEY_SHA256_HEX:-}"

strict_required="optional"
if is_true "${RELEASE_PREFLIGHT_STRICT:-0}" || [[ "${profile}" == "production" ]]; then
  strict_required="required"
fi

info "repo: ${repo_root}"
info "profile: ${profile}"
info "strict checks: ${strict_required}"

case "${profile}" in
  dev | chip-security | production)
    ok "FW_PROFILE is supported"
    ;;
  *)
    fail "unsupported FW_PROFILE: ${profile}"
  ;;
esac

check_release_version "${release_version}" "${strict_required}"
check_hex64 "${trusted_hash}" "NOCKSTER_UPDATE_PUBKEY_SHA256_HEX" "${strict_required}"
check_tracked_secret_paths
check_tools "${strict_required}"
check_partition_table "${PARTITION_TABLE:-partitions.csv}" "${strict_required}"

check_hmac_key_file "${HMAC_KEY_FILE:-}" "${strict_required}"
check_update_signing_key_file "${UPDATE_SIGNING_KEY_FILE:-}" "${strict_required}"
check_update_signing_key_trust_hash "${UPDATE_SIGNING_KEY_FILE:-}" "${trusted_hash}" "${strict_required}"
check_secure_boot_key_file "${SECURE_BOOT_KEY_FILE:-}" "${strict_required}"
check_flash_encryption_key_file "${FLASH_ENCRYPTION_KEY_FILE:-}" "${strict_required}"
check_release_image_file "${UPDATE_FIRMWARE:-}" "UPDATE_FIRMWARE" "${strict_required}"
if [[ -n "${SECURE_BOOT_IMAGE:-}" ]]; then
  check_release_image_file "${SECURE_BOOT_IMAGE}" "SECURE_BOOT_IMAGE" "required"
fi
check_secure_boot_image_paths "${SECURE_BOOT_IMAGE:-}" "${SECURE_BOOT_SIGNED_IMAGE:-}"
check_update_bundle "${UPDATE_BUNDLE:-}" "${UPDATE_FIRMWARE:-}" "${trusted_hash}" "${strict_required}"
check_update_index "${UPDATE_INDEX:-}" "${UPDATE_BUNDLE:-}" "${UPDATE_FIRMWARE:-}"
maybe_run_efuse_summary "${PROVISION_PORT:-}"

if (( failures > 0 )); then
  printf 'release preflight failed: %d failure(s), %d warning(s)\n' "${failures}" "${warnings}" >&2
  exit 1
fi

printf 'release preflight passed: %d warning(s)\n' "${warnings}"
