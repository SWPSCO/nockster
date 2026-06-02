#!/usr/bin/env bash
set -euo pipefail

port="${1:-}"
espefuse_cmd="${ESPEFUSE:-espefuse}"
if [[ -z "${port}" ]]; then
  echo "usage: $0 /dev/ttyACM0" >&2
  exit 2
fi

if ! command -v "${espefuse_cmd}" >/dev/null 2>&1; then
  echo "missing espefuse command: ${espefuse_cmd}" >&2
  echo "install Espressif tooling or set ESPEFUSE=/path/to/espefuse" >&2
  exit 2
fi

exec "${espefuse_cmd}" --chip esp32s3 --port "${port}" summary
