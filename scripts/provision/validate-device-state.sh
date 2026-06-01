#!/usr/bin/env bash
set -euo pipefail

stage="${VALIDATE_STAGE:-${1:-smoke}}"
port="${VALIDATE_PORT:-hid}"
baud="${VALIDATE_BAUD:-115200}"
dry_run="${VALIDATE_DRY_RUN:-${DRY_RUN:-0}}"
configured_cli="${NOCKSTER_CLI:-}"

is_true() {
  case "${1:-}" in
    1 | true | TRUE | yes | YES | on | ON) return 0 ;;
    *) return 1 ;;
  esac
}

usage() {
  cat <<'USAGE'
usage: make validate-device-state [VALIDATE_STAGE=smoke] [VALIDATE_PORT=hid]

Stages:
  smoke             non-destructive protocol/info/security/slot health smoke
  hmac-up           require chip-security, HMAC_UP, read-protection, and NVS v2
  update-ready      require idle update stream and OTA partition layout
  reboot            request a non-destructive firmware reboot
  secure-boot       require secure boot enabled
  flash-encryption  require flash encryption enabled
  lockdown          require production-lockdown fuses except power-glitch
  power-glitch      require power-glitch protection
  production        run hmac-up, secure-boot, flash-encryption, lockdown, and OTA checks

Set VALIDATE_DRY_RUN=1 to print commands without touching the device.
USAGE
}

resolve_cli() {
  if [[ -n "${configured_cli}" ]]; then
    printf '%s\n' "${configured_cli}"
    return
  fi
  if is_true "${dry_run}"; then
    printf '%s\n' "nockster-cli"
    return
  fi
  if [[ -x target/x86_64-unknown-linux-gnu/release/nockster-cli ]]; then
    printf '%s\n' "target/x86_64-unknown-linux-gnu/release/nockster-cli"
    return
  fi
  if [[ -x target/release/nockster-cli ]]; then
    printf '%s\n' "target/release/nockster-cli"
    return
  fi
  if command -v nockster-cli >/dev/null 2>&1; then
    command -v nockster-cli
    return
  fi
  echo "missing nockster-cli; set NOCKSTER_CLI=/path/to/nockster-cli or build the CLI" >&2
  exit 2
}

run_cmd() {
  printf '+'
  printf ' %q' "$@"
  printf '\n'
  if ! is_true "${dry_run}"; then
    "$@"
  fi
}

cli="$(resolve_cli)"

case "${stage}" in
  -h|--help|help)
    usage
    exit 0
    ;;
  smoke|hmac-up|update-ready|reboot|secure-boot|flash-encryption|lockdown|power-glitch|production) ;;
  *)
    echo "unsupported VALIDATE_STAGE: ${stage}" >&2
    usage >&2
    exit 2
    ;;
esac

echo "device validation stage: ${stage}"
echo "port: ${port}"
if is_true "${dry_run}"; then
  echo "dry-run: yes"
fi

validate_smoke() {
  run_cmd "${cli}" smoke --port "${port}" --baud "${baud}"
}

validate_hmac_up() {
  run_cmd "${cli}" security \
    --port "${port}" \
    --baud "${baud}" \
    --expect-chip-security \
    --expect-hmac-up \
    --expect-hmac-up-read-protected \
    --expect-nvs-v2
}

validate_update_ready() {
  run_cmd "${cli}" update status \
    --port "${port}" \
    --baud "${baud}" \
    --expect-idle \
    --expect-ota-ready \
    --require-boot-status
}

validate_reboot() {
  run_cmd "${cli}" reboot --port "${port}" --baud "${baud}"
}

validate_secure_boot() {
  run_cmd "${cli}" security \
    --port "${port}" \
    --baud "${baud}" \
    --expect-chip-security \
    --expect-secure-boot
}

validate_flash_encryption() {
  run_cmd "${cli}" security \
    --port "${port}" \
    --baud "${baud}" \
    --expect-chip-security \
    --expect-flash-encryption
}

validate_lockdown() {
  run_cmd "${cli}" security \
    --port "${port}" \
    --baud "${baud}" \
    --expect-chip-security \
    --expect-production-lockdown
}

validate_power_glitch() {
  run_cmd "${cli}" security \
    --port "${port}" \
    --baud "${baud}" \
    --expect-chip-security \
    --expect-power-glitch-protection
}

case "${stage}" in
  smoke)
    validate_smoke
    ;;
  hmac-up)
    validate_hmac_up
    ;;
  update-ready)
    validate_update_ready
    ;;
  reboot)
    validate_reboot
    ;;
  secure-boot)
    validate_secure_boot
    ;;
  flash-encryption)
    validate_flash_encryption
    ;;
  lockdown)
    validate_lockdown
    ;;
  power-glitch)
    validate_power_glitch
    ;;
  production)
    validate_hmac_up
    validate_secure_boot
    validate_flash_encryption
    validate_lockdown
    validate_update_ready
    ;;
esac

echo "device validation stage '${stage}' completed"
