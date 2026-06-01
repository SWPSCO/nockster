#!/usr/bin/env bash
set -euo pipefail

port="${1:-}"
key_file="${2:-}"
block="${3:-BLOCK_KEY0}"
espefuse_cmd="${ESPEFUSE:-espefuse}"
script_dir="$(cd "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"

if [[ -z "${port}" || -z "${key_file}" ]]; then
  echo "usage: $0 /dev/ttyACM0 /path/to/secure-boot-v2.pem [BLOCK_KEY0]" >&2
  exit 2
fi

bash "${script_dir}/check-secret-output-path.sh" "${key_file}" "secure boot v2 key file"

if [[ ! -f "${key_file}" ]]; then
  echo "missing secure boot signing key: ${key_file}" >&2
  exit 2
fi

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

echo "About to burn the secure boot v2 digest for ${key_file} into ${block} on ${port}."
echo "This is irreversible. Do this only on a production/provisioning board."
echo "Current eFuse summary:"
"${espefuse_cmd}" --chip esp32s3 --port "${port}" summary

read -r -p "Type BURN-SECURE-BOOT-V2 to continue: " confirmation
if [[ "${confirmation}" != "BURN-SECURE-BOOT-V2" ]]; then
  echo "aborted"
  exit 1
fi

exec "${espefuse_cmd}" --chip esp32s3 --port "${port}" burn-key-digest \
  "${block}" "${key_file}" SECURE_BOOT_DIGEST0
