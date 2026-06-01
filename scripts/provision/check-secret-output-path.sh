#!/usr/bin/env bash
set -euo pipefail

path="${1:-}"
label="${2:-secret output file}"

if [[ -z "${path}" ]]; then
  echo "usage: $0 /path/to/secret-file [label]" >&2
  exit 2
fi

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd -P)"

case "${path}" in
  /*) target="${path}" ;;
  *) target="$(pwd -P)/${path}" ;;
esac

parent="$(dirname -- "${target}")"
base="$(basename -- "${target}")"
probe="${parent}"
suffix=""
while [[ ! -d "${probe}" && "${probe}" != "/" ]]; do
  suffix="/$(basename -- "${probe}")${suffix}"
  probe="$(dirname -- "${probe}")"
done

if [[ ! -d "${probe}" ]]; then
  echo "could not resolve ${label} parent path: ${parent}" >&2
  exit 2
fi

resolved_parent="$(cd "${probe}" && pwd -P)${suffix}"
resolved_target="${resolved_parent}/${base}"

case "${resolved_target}" in
  "${repo_root}" | "${repo_root}"/*)
    echo "${label} must live outside the repo: ${resolved_target}" >&2
    exit 2
    ;;
esac

echo "${label} path is outside the repo: ${resolved_target}"
