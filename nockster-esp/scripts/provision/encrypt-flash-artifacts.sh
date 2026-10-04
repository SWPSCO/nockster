#!/usr/bin/env bash
set -euo pipefail

key_file="${1:-}"
bootloader_image="${2:-}"
partition_table_bin="${3:-}"
signed_app_image="${4:-}"
out_bootloader="${5:-target/secure-boot-v2/encrypted/bootloader.enc.bin}"
out_partition_table="${6:-target/secure-boot-v2/encrypted/partition-table.enc.bin}"
out_signed_app="${7:-target/secure-boot-v2/encrypted/nockster-fw.factory.signed.enc.bin}"
out_otadata="${8:-target/secure-boot-v2/encrypted/otadata.blank.enc.bin}"

espsecure_cmd="${ESPSECURE:-espsecure}"
script_dir="$(cd "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"

bootloader_offset="${FLASH_ENCRYPTION_BOOTLOADER_OFFSET:-0x0}"
partition_table_offset="${FLASH_ENCRYPTION_PARTITION_TABLE_OFFSET:-0x8000}"
app_offset="${FLASH_ENCRYPTION_APP_OFFSET:-0x10000}"
otadata_offset="${FLASH_ENCRYPTION_OTADATA_OFFSET:-0x310000}"
otadata_size="${FLASH_ENCRYPTION_OTADATA_SIZE:-8192}"

if [[ -z "${key_file}" || -z "${bootloader_image}" || -z "${partition_table_bin}" || -z "${signed_app_image}" ]]; then
  echo "usage: $0 /path/to/flash-encryption-key.bin signed-bootloader.bin partition-table.bin signed-app.bin [out-bootloader.enc] [out-partition-table.enc] [out-app.enc] [out-otadata.enc]" >&2
  exit 2
fi

bash "${script_dir}/check-secret-output-path.sh" "${key_file}" "flash encryption key file"
bash "${script_dir}/check-flash-encryption-key.sh" "${key_file}" "flash encryption key file"

for input in "${bootloader_image}" "${partition_table_bin}" "${signed_app_image}"; do
  if [[ ! -f "${input}" ]]; then
    echo "missing flash encryption input artifact: ${input}" >&2
    exit 2
  fi
done

if ! command -v "${espsecure_cmd}" >/dev/null 2>&1; then
  echo "missing espsecure command: ${espsecure_cmd}" >&2
  echo "install Espressif tooling or set ESPSECURE=/path/to/espsecure" >&2
  exit 2
fi

mkdir -p -- \
  "$(dirname -- "${out_bootloader}")" \
  "$(dirname -- "${out_partition_table}")" \
  "$(dirname -- "${out_signed_app}")" \
  "$(dirname -- "${out_otadata}")"

tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/nockster-flash-enc.XXXXXX")"
cleanup() {
  rm -rf -- "${tmpdir}"
}
trap cleanup EXIT

blank_otadata="${tmpdir}/otadata.blank.bin"
head -c "${otadata_size}" /dev/zero | tr '\000' '\377' >"${blank_otadata}"

encrypt_one() {
  local address="$1"
  local input="$2"
  local output="$3"
  local tmp_output="${tmpdir}/$(basename -- "${output}").tmp"

  echo "Encrypting ${input} for flash address ${address} -> ${output}"
  "${espsecure_cmd}" encrypt-flash-data \
    --aes-xts \
    --keyfile "${key_file}" \
    --address "${address}" \
    --output "${tmp_output}" \
    "${input}"
  mv -- "${tmp_output}" "${output}"
}

encrypt_one "${bootloader_offset}" "${bootloader_image}" "${out_bootloader}"
encrypt_one "${partition_table_offset}" "${partition_table_bin}" "${out_partition_table}"
encrypt_one "${app_offset}" "${signed_app_image}" "${out_signed_app}"
encrypt_one "${otadata_offset}" "${blank_otadata}" "${out_otadata}"

if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "${out_bootloader}" "${out_partition_table}" "${out_signed_app}" "${out_otadata}"
fi

echo "wrote encrypted bootloader: ${out_bootloader}"
echo "wrote encrypted partition table: ${out_partition_table}"
echo "wrote encrypted signed app: ${out_signed_app}"
echo "wrote encrypted blank otadata: ${out_otadata}"
