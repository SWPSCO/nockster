#!/usr/bin/env bash
set -euo pipefail

key_file="${1:-}"
label="${2:-flash encryption key file}"

if [[ -z "${key_file}" ]]; then
  echo "usage: $0 /path/to/flash-encryption-key.bin [label]" >&2
  exit 2
fi

if [[ ! -f "${key_file}" ]]; then
  echo "missing ${label}: ${key_file}" >&2
  exit 2
fi

size="$(wc -c <"${key_file}" | tr -d '[:space:]')"
if [[ "${size}" != "32" ]]; then
  echo "${label} must be exactly 32 bytes for ESP32-S3 XTS_AES_128_KEY, got ${size}" >&2
  exit 2
fi

echo "${label} is 32 bytes for ESP32-S3 flash encryption"
