#!/usr/bin/env bash
set -euo pipefail

key_file="${1:-}"
label="${2:-secure boot v2 key file}"

if [[ -z "${key_file}" ]]; then
  echo "usage: $0 /path/to/secure-boot-v2-rsa.pem [label]" >&2
  exit 2
fi

if [[ ! -f "${key_file}" ]]; then
  echo "missing ${label}: ${key_file}" >&2
  exit 2
fi
if ! command -v openssl >/dev/null 2>&1; then
  echo "missing openssl command" >&2
  exit 2
fi

if ! public_key_header="$(openssl pkey -in "${key_file}" -noout -text_pub 2>/dev/null | sed -n '1p')"; then
  echo "${label} is not a readable PEM private key: ${key_file}" >&2
  exit 2
fi

if [[ "${public_key_header}" != "Public-Key: (3072 bit)" ]]; then
  echo "ESP32-S3 secure boot v2 requires an RSA3072 signing key." >&2
  echo "Got: ${public_key_header:-<unknown key type>}" >&2
  exit 2
fi

echo "${label} is RSA3072 for ESP32-S3 secure boot v2"
