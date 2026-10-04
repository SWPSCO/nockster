#!/usr/bin/env bash
set -euo pipefail

image="${1:-}"
max_bytes="${2:-${NOCKSTER_APP_SLOT_SIZE_BYTES:-3145728}}"
label="${3:-firmware image}"

if [[ -z "${image}" ]]; then
  echo "usage: $0 /path/to/app-image.bin [max-bytes] [label]" >&2
  exit 2
fi

if [[ ! -f "${image}" ]]; then
  echo "${label}: missing file: ${image}" >&2
  exit 2
fi

if [[ ! "${max_bytes}" =~ ^[0-9]+$ ]]; then
  echo "${label}: max bytes must be numeric, got ${max_bytes}" >&2
  exit 2
fi

size="$(wc -c <"${image}" | tr -d '[:space:]')"
if [[ ! "${size}" =~ ^[0-9]+$ ]]; then
  echo "${label}: could not determine file size: ${image}" >&2
  exit 2
fi

if (( size < 24 )); then
  echo "${label}: too small to be an ESP app image (${size} bytes)" >&2
  exit 2
fi

if (( size > max_bytes )); then
  echo "${label}: ${size} bytes exceeds configured app slot size ${max_bytes}" >&2
  exit 2
fi

magic="$(od -An -tx1 -N1 "${image}" | tr -d '[:space:]')"
if [[ "${magic}" != "e9" ]]; then
  echo "${label}: first byte is 0x${magic:-??}, expected ESP image magic 0xe9" >&2
  exit 2
fi

segments="$(od -An -tu1 -j1 -N1 "${image}" | tr -d '[:space:]')"
if [[ ! "${segments}" =~ ^[0-9]+$ ]] || (( segments == 0 || segments > 32 )); then
  echo "${label}: suspicious segment count ${segments:-?}" >&2
  exit 2
fi

sha256="unavailable"
if command -v sha256sum >/dev/null 2>&1; then
  sha256="$(sha256sum "${image}" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
  sha256="$(shasum -a 256 "${image}" | awk '{print $1}')"
fi

echo "${label}: ok"
echo "  path: ${image}"
echo "  size: ${size}"
echo "  sha256: ${sha256}"
echo "  esp_image_magic: 0x${magic}"
echo "  segment_count: ${segments}"
