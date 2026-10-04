#!/usr/bin/env bash
set -euo pipefail

port="${1:-}"
key_file="${2:-}"
block="${3:-BLOCK_KEY0}"
purpose="${4:-}"
espefuse_cmd="${ESPEFUSE:-espefuse}"
script_dir="$(cd "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"

if [[ -z "${port}" || -z "${key_file}" ]]; then
  echo "usage: $0 /dev/ttyACM0 /path/to/secure-boot-v2-rsa.pem [BLOCK_KEY0] [SECURE_BOOT_DIGEST0]" >&2
  exit 2
fi

bash "${script_dir}/check-secret-output-path.sh" "${key_file}" "secure boot v2 key file"

if [[ ! -f "${key_file}" ]]; then
  echo "missing secure boot signing key: ${key_file}" >&2
  exit 2
fi
bash "${script_dir}/check-secure-boot-v2-key.sh" "${key_file}" "secure boot v2 key file"

if ! command -v "${espefuse_cmd}" >/dev/null 2>&1; then
  echo "missing espefuse command: ${espefuse_cmd}" >&2
  echo "install Espressif tooling or set ESPEFUSE=/path/to/espefuse" >&2
  exit 2
fi

case "${block}" in
  BLOCK_KEY0|BLOCK_KEY1|BLOCK_KEY2|BLOCK_KEY3|BLOCK_KEY4|BLOCK_KEY5) ;;
  *)
    echo "unsupported secure boot digest block: ${block}" >&2
    echo "expected one of BLOCK_KEY0..BLOCK_KEY5" >&2
    exit 2
    ;;
esac

if [[ -z "${purpose}" ]]; then
  case "${block}" in
    BLOCK_KEY0) purpose="SECURE_BOOT_DIGEST0" ;;
    BLOCK_KEY1) purpose="SECURE_BOOT_DIGEST1" ;;
    BLOCK_KEY2) purpose="SECURE_BOOT_DIGEST2" ;;
    *)
      echo "Set SECURE_BOOT_DIGEST_PURPOSE=SECURE_BOOT_DIGEST0|1|2 when using ${block}" >&2
      exit 2
      ;;
  esac
fi

case "${purpose}" in
  SECURE_BOOT_DIGEST0|SECURE_BOOT_DIGEST1|SECURE_BOOT_DIGEST2) ;;
  *)
    echo "unsupported secure boot digest purpose: ${purpose}" >&2
    echo "expected SECURE_BOOT_DIGEST0, SECURE_BOOT_DIGEST1, or SECURE_BOOT_DIGEST2" >&2
    exit 2
    ;;
esac

echo "About to burn the secure boot v2 digest for ${key_file} into ${block} as ${purpose} on ${port}."
echo "This is irreversible. Do this only on a production/provisioning board."
echo "Current eFuse summary:"
"${espefuse_cmd}" --chip esp32s3 --port "${port}" summary

read -r -p "Type BURN-SECURE-BOOT-V2 to continue: " confirmation
if [[ "${confirmation}" != "BURN-SECURE-BOOT-V2" ]]; then
  echo "aborted"
  exit 1
fi

exec "${espefuse_cmd}" --chip esp32s3 --port "${port}" burn-key-digest \
  "${block}" "${key_file}" "${purpose}"
