#!/usr/bin/env bash
set -euo pipefail

key_file="${1:-}"
espsecure_cmd="${ESPSECURE:-espsecure}"

if [[ -z "${key_file}" ]]; then
  echo "usage: $0 /path/to/secure-boot-v2.pem" >&2
  exit 2
fi

if [[ -e "${key_file}" ]]; then
  echo "refusing to overwrite existing secure boot v2 key file: ${key_file}" >&2
  exit 2
fi

script_dir="$(cd "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
bash "${script_dir}/check-secret-output-path.sh" "${key_file}" "secure boot v2 key file"

if ! command -v "${espsecure_cmd}" >/dev/null 2>&1; then
  echo "missing espsecure command: ${espsecure_cmd}" >&2
  echo "install Espressif tooling or set ESPSECURE=/path/to/espsecure" >&2
  exit 2
fi

parent="$(dirname -- "${key_file}")"
if [[ -n "${parent}" && "${parent}" != "." ]]; then
  existed=0
  if [[ -d "${parent}" ]]; then
    existed=1
  fi
  umask 077
  mkdir -p -- "${parent}"
  if [[ "${existed}" == "1" ]]; then
    mode="$(stat -c '%a' "${parent}" 2>/dev/null || stat -f '%Lp' "${parent}" 2>/dev/null || true)"
    if [[ -n "${mode}" && "${mode}" != "700" ]]; then
      echo "warning: ${parent} has mode ${mode}; keep secure boot keys outside shared directories" >&2
    fi
  else
    chmod 700 -- "${parent}"
  fi
fi

tmp="${key_file}.tmp.$$"
cleanup() {
  rm -f -- "${tmp}"
}
trap cleanup EXIT

umask 077
"${espsecure_cmd}" generate-signing-key \
  --version 2 \
  --scheme ecdsa256 \
  "${tmp}"
chmod 600 -- "${tmp}"

mv -n -- "${tmp}" "${key_file}"
if [[ -e "${tmp}" ]]; then
  echo "refusing to overwrite existing secure boot v2 key file: ${key_file}" >&2
  exit 2
fi
trap - EXIT
echo "wrote secure boot v2 signing key: ${key_file}"
echo "mode: 0600"
