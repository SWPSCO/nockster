#!/usr/bin/env bash
set -euo pipefail

port="${1:-}"
crypt_cnt="${2:-0x7}"
espefuse_cmd="${ESPEFUSE:-espefuse}"

if [[ -z "${port}" ]]; then
  echo "usage: $0 /dev/ttyACM0 [0x7]" >&2
  exit 2
fi

case "${crypt_cnt}" in
  1|3|7|0x1|0x3|0x7) ;;
  *)
    echo "unsupported SPI_BOOT_CRYPT_CNT value: ${crypt_cnt}" >&2
    echo "expected 0x1, 0x3, or 0x7; 0x7 is the production default" >&2
    exit 2
    ;;
esac

if ! command -v "${espefuse_cmd}" >/dev/null 2>&1; then
  echo "missing espefuse command: ${espefuse_cmd}" >&2
  echo "install Espressif tooling or set ESPEFUSE=/path/to/espefuse" >&2
  exit 2
fi

echo "About to enable ESP32-S3 flash encryption on ${port} by burning SPI_BOOT_CRYPT_CNT=${crypt_cnt}."
echo "This is irreversible. Do this only after secure boot, encrypted-image flashing, and recovery tests pass on a sacrificial board."
echo "Current eFuse summary:"
"${espefuse_cmd}" --chip esp32s3 --port "${port}" summary

read -r -p "Type ENABLE-FLASH-ENCRYPTION to continue: " confirmation
if [[ "${confirmation}" != "ENABLE-FLASH-ENCRYPTION" ]]; then
  echo "aborted"
  exit 1
fi

exec "${espefuse_cmd}" --chip esp32s3 --port "${port}" burn-efuse \
  SPI_BOOT_CRYPT_CNT "${crypt_cnt}"
