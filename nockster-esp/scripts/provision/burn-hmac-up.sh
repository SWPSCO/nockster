#!/usr/bin/env bash
set -euo pipefail

port="${1:-}"
key_file="${2:-}"
espefuse_cmd="${ESPEFUSE:-espefuse}"
script_dir="$(cd "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"

if [[ -z "${port}" || -z "${key_file}" ]]; then
  echo "usage: $0 /dev/ttyACM0 /path/to/hmac-up.bin" >&2
  exit 2
fi

bash "${script_dir}/check-secret-output-path.sh" "${key_file}" "HMAC_UP key file"

if [[ ! -f "${key_file}" ]]; then
  echo "missing HMAC key file: ${key_file}" >&2
  exit 2
fi

size="$(wc -c <"${key_file}" | tr -d '[:space:]')"
if [[ "${size}" != "32" ]]; then
  echo "HMAC_UP key file must be exactly 32 bytes, got ${size}" >&2
  exit 2
fi

if ! command -v "${espefuse_cmd}" >/dev/null 2>&1; then
  echo "missing espefuse command: ${espefuse_cmd}" >&2
  echo "install Espressif tooling or set ESPEFUSE=/path/to/espefuse" >&2
  exit 2
fi

echo "About to burn HMAC_UP key material into BLOCK_KEY5 on ${port}."
echo "This is irreversible. The key file should be secret and must not be committed."
echo "Current eFuse summary:"
"${espefuse_cmd}" --chip esp32s3 --port "${port}" summary

read -r -p "Type BURN-HMAC-UP to continue: " confirmation
if [[ "${confirmation}" != "BURN-HMAC-UP" ]]; then
  echo "aborted"
  exit 1
fi

exec "${espefuse_cmd}" --chip esp32s3 --port "${port}" burn-key BLOCK_KEY5 "${key_file}" HMAC_UP
