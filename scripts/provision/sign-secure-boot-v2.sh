#!/usr/bin/env bash
set -euo pipefail

key_file="${1:-}"
input_image="${2:-}"
output_image="${3:-}"
espsecure_cmd="${ESPSECURE:-espsecure}"
script_dir="$(cd "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"

if [[ -z "${key_file}" || -z "${input_image}" || -z "${output_image}" ]]; then
  echo "usage: $0 /path/to/secure-boot-v2.pem /path/to/unsigned-app.bin /path/to/signed-app.bin" >&2
  exit 2
fi

bash "${script_dir}/check-secret-output-path.sh" "${key_file}" "secure boot v2 key file"

if [[ ! -f "${key_file}" ]]; then
  echo "missing secure boot signing key: ${key_file}" >&2
  exit 2
fi

if [[ ! -f "${input_image}" ]]; then
  echo "missing unsigned app image: ${input_image}" >&2
  exit 2
fi

if [[ "${input_image}" == "${output_image}" ]]; then
  echo "refusing to sign in place; choose a distinct output path" >&2
  exit 2
fi

if [[ -e "${output_image}" ]]; then
  echo "refusing to overwrite existing signed image: ${output_image}" >&2
  exit 2
fi

out_dir="$(dirname -- "${output_image}")"
if [[ ! -d "${out_dir}" ]]; then
  echo "signed image output directory does not exist: ${out_dir}" >&2
  exit 2
fi

"${script_dir}/check-release-image.sh" "${input_image}" "${NOCKSTER_APP_SLOT_SIZE_BYTES:-3145728}" "secure boot input image"

echo "Signing ${input_image} for ESP32-S3 secure boot v2."
echo "The signing key is secret and must stay outside the repo."
"${espsecure_cmd}" sign-data \
  --version 2 \
  --keyfile "${key_file}" \
  --output "${output_image}" \
  "${input_image}"

"${espsecure_cmd}" verify-signature \
  --version 2 \
  --keyfile "${key_file}" \
  "${output_image}"

"${espsecure_cmd}" signature-info-v2 "${output_image}"
echo "wrote secure-boot-signed image: ${output_image}"
