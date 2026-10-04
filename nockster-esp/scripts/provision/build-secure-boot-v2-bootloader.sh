#!/usr/bin/env bash
set -euo pipefail

key_file="${1:-}"
partition_table="${2:-partitions.csv}"
out_bootloader="${3:-target/secure-boot-v2/bootloader.bin}"
out_partition_table="${4:-target/secure-boot-v2/partition-table.bin}"
flash_encryption="${SECURE_BOOT_BOOTLOADER_FLASH_ENCRYPTION:-0}"

if [[ -z "${key_file}" ]]; then
  echo "usage: $0 /path/to/rsa3072-secure-boot.pem [partitions.csv] [out-bootloader.bin] [out-partition-table.bin]" >&2
  exit 2
fi

script_dir="$(cd "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd "${script_dir}/../.." && pwd -P)"

bash "${script_dir}/check-secret-output-path.sh" "${key_file}" "secure boot v2 key file"

if [[ ! -f "${key_file}" ]]; then
  echo "missing secure boot signing key: ${key_file}" >&2
  exit 2
fi
if [[ ! -f "${partition_table}" ]]; then
  echo "missing partition table: ${partition_table}" >&2
  exit 2
fi
bash "${script_dir}/check-secure-boot-v2-key.sh" "${key_file}" "secure boot v2 key file"

if ! command -v idf.py >/dev/null 2>&1; then
  if [[ -z "${IDF_PATH:-}" ]]; then
    if [[ -d "${HOME}/esp/esp-idf" ]]; then
      export IDF_PATH="${HOME}/esp/esp-idf"
    else
      echo "idf.py is not on PATH and IDF_PATH is not set" >&2
      exit 2
    fi
  fi
  # shellcheck disable=SC1091
  . "${IDF_PATH}/export.sh" >/dev/null
fi
if ! command -v espsecure >/dev/null 2>&1; then
  echo "missing espsecure command" >&2
  exit 2
fi

key_abs="$(realpath -- "${key_file}")"
partition_abs="$(realpath -- "${partition_table}")"
build_root="${SECURE_BOOT_BOOTLOADER_BUILD_DIR:-${repo_root}/target/secure-boot-v2/idf-bootloader}"
project_dir="${build_root}/project"

rm -rf -- "${project_dir}"
mkdir -p -- "${project_dir}/main" "$(dirname -- "${out_bootloader}")" "$(dirname -- "${out_partition_table}")"
cp -- "${partition_abs}" "${project_dir}/partitions.csv"

cat >"${project_dir}/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.16)
include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(nockster_secure_bootloader)
EOF

cat >"${project_dir}/main/CMakeLists.txt" <<'EOF'
idf_component_register(SRCS "app_main.c" INCLUDE_DIRS "")
EOF

cat >"${project_dir}/main/app_main.c" <<'EOF'
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

void app_main(void) {
    while (1) {
        vTaskDelay(pdMS_TO_TICKS(1000));
    }
}
EOF

cat >"${project_dir}/sdkconfig.defaults" <<EOF
CONFIG_IDF_TARGET="esp32s3"
CONFIG_PARTITION_TABLE_CUSTOM=y
CONFIG_PARTITION_TABLE_CUSTOM_FILENAME="partitions.csv"
CONFIG_PARTITION_TABLE_OFFSET=0x8000
CONFIG_ESPTOOLPY_FLASHSIZE_16MB=y
CONFIG_ESPTOOLPY_FLASHSIZE="16MB"
CONFIG_ESPTOOLPY_FLASHMODE_DIO=y
CONFIG_ESPTOOLPY_FLASHFREQ_40M=y
CONFIG_SECURE_BOOT=y
CONFIG_SECURE_BOOT_V2_ENABLED=y
CONFIG_SECURE_BOOT_BUILD_SIGNED_BINARIES=y
CONFIG_SECURE_BOOT_SIGNING_KEY="${key_abs}"
CONFIG_SECURE_BOOT_FLASH_BOOTLOADER_DEFAULT=y
CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y
CONFIG_BOOTLOADER_LOG_LEVEL_NONE=y
CONFIG_LOG_DEFAULT_LEVEL_NONE=y
EOF

case "${flash_encryption}" in
  1|true|TRUE|yes|YES|on|ON)
    cat >>"${project_dir}/sdkconfig.defaults" <<'EOF'
CONFIG_SECURE_FLASH_ENC_ENABLED=y
CONFIG_SECURE_FLASH_ENCRYPTION_AES128=y
CONFIG_SECURE_FLASH_ENCRYPTION_MODE_RELEASE=y
# CONFIG_SECURE_FLASH_ENCRYPTION_MODE_DEVELOPMENT is not set
CONFIG_SECURE_FLASH_CHECK_ENC_EN_IN_APP=y
EOF
    ;;
  0|false|FALSE|no|NO|off|OFF|"") ;;
  *)
    echo "unsupported SECURE_BOOT_BOOTLOADER_FLASH_ENCRYPTION=${flash_encryption}" >&2
    echo "expected 0 or 1" >&2
    exit 2
    ;;
esac

(
  cd "${project_dir}"
  rm -rf -- build sdkconfig
  idf.py set-target esp32s3
  idf.py bootloader
)

cp -- "${project_dir}/build/bootloader/bootloader.bin" "${out_bootloader}"
cp -- "${project_dir}/build/partition_table/partition-table.bin" "${out_partition_table}"

espsecure verify-signature --version 2 --keyfile "${key_abs}" "${out_bootloader}"

size="$(stat -c '%s' "${out_bootloader}" 2>/dev/null || stat -f '%z' "${out_bootloader}")"
if (( size > 32768 )); then
  echo "signed bootloader is too large for partition table offset 0x8000: ${size} bytes" >&2
  exit 1
fi

echo "wrote signed secure-boot bootloader: ${out_bootloader}"
echo "wrote partition table: ${out_partition_table}"
if [[ "${flash_encryption}" =~ ^(1|true|TRUE|yes|YES|on|ON)$ ]]; then
  echo "bootloader was built with flash-encryption release-mode support"
fi
