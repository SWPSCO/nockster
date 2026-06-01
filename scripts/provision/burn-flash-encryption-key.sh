#!/usr/bin/env bash
set -euo pipefail

port="${1:-}"
key_file="${2:-}"
block="${3:-BLOCK_KEY4}"
espefuse_cmd="${ESPEFUSE:-espefuse}"
script_dir="$(cd "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"

if [[ -z "${port}" || -z "${key_file}" ]]; then
  echo "usage: $0 /dev/ttyACM0 /path/to/flash-encryption-key.bin [BLOCK_KEY4]" >&2
  exit 2
fi

bash "${script_dir}/check-secret-output-path.sh" "${key_file}" "flash encryption key file"

if [[ ! -f "${key_file}" ]]; then
  echo "missing flash encryption key file: ${key_file}" >&2
  exit 2
fi

size="$(wc -c <"${key_file}" | tr -d '[:space:]')"
if [[ "${size}" != "32" ]]; then
  echo "flash encryption key file must be exactly 32 bytes, got ${size}" >&2
  exit 2
fi

case "${block}" in
  BLOCK_KEY0|BLOCK_KEY1|BLOCK_KEY2|BLOCK_KEY3|BLOCK_KEY4|BLOCK_KEY5) ;;
  *)
    echo "unsupported flash encryption key block: ${block}" >&2
    echo "expected one of BLOCK_KEY0..BLOCK_KEY5" >&2
    exit 2
    ;;
esac

if ! command -v "${espefuse_cmd}" >/dev/null 2>&1; then
  echo "missing espefuse command: ${espefuse_cmd}" >&2
  echo "install Espressif tooling or set ESPEFUSE=/path/to/espefuse" >&2
  exit 2
fi

echo "About to burn an XTS_AES_128_KEY flash encryption key into ${block} on ${port}."
echo "This is irreversible. Keep this key file secret and outside the repo."
echo "Do this only after confirming ${block} is unused in the current eFuse summary."
echo "Current eFuse summary:"
"${espefuse_cmd}" --chip esp32s3 --port "${port}" summary

read -r -p "Type BURN-FLASH-ENCRYPTION-KEY to continue: " confirmation
if [[ "${confirmation}" != "BURN-FLASH-ENCRYPTION-KEY" ]]; then
  echo "aborted"
  exit 1
fi

exec "${espefuse_cmd}" --chip esp32s3 --port "${port}" burn-key \
  "${block}" "${key_file}" XTS_AES_128_KEY
