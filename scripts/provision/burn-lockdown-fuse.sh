#!/usr/bin/env bash
set -euo pipefail

port="${1:-}"
scope="${2:-}"
espefuse_cmd="${ESPEFUSE:-espefuse}"

usage() {
  cat <<'USAGE' >&2
usage: burn-lockdown-fuse.sh /dev/ttyACM0 <scope>

Scopes:
  jtag          burn JTAG/USB-serial-JTAG disable fuses
  download      burn ROM download-mode disable fuses
  direct-boot   burn direct boot disable fuse
  rom-print     burn ROM USB-serial-JTAG print disable fuse
  power-glitch  enable power-glitch protection

Every scope is irreversible. Run provision-summary first.
USAGE
}

if [[ -z "${port}" || -z "${scope}" ]]; then
  usage
  exit 2
fi

if ! command -v "${espefuse_cmd}" >/dev/null 2>&1; then
  echo "missing espefuse command: ${espefuse_cmd}" >&2
  echo "install Espressif tooling or set ESPEFUSE=/path/to/espefuse" >&2
  exit 2
fi

case "${scope}" in
  jtag)
    description="disable pad JTAG, USB JTAG, software JTAG, and USB-serial-JTAG on ${port}"
    confirmation_text="DISABLE-JTAG"
    ;;
  download)
    description="disable ROM download mode, USB-serial-JTAG download, and USB-OTG download on ${port}"
    confirmation_text="DISABLE-DOWNLOAD-MODE"
    ;;
  direct-boot)
    description="disable direct boot on ${port}"
    confirmation_text="DISABLE-DIRECT-BOOT"
    ;;
  rom-print)
    description="disable ROM USB-serial-JTAG printing on ${port}"
    confirmation_text="DISABLE-ROM-PRINT"
    ;;
  power-glitch)
    description="enable power-glitch protection on ${port}"
    confirmation_text="ENABLE-POWER-GLITCH"
    ;;
  *)
    echo "unsupported lockdown scope: ${scope}" >&2
    usage
    exit 2
    ;;
esac

echo "About to ${description}."
echo "This is irreversible and can make the board difficult or impossible to recover over USB-C."
echo "Do this only after secure boot, flash encryption, OTA recovery, and sacrificial-board tests pass."
echo "Current eFuse summary:"
"${espefuse_cmd}" --chip esp32s3 --port "${port}" summary

read -r -p "Type ${confirmation_text} to continue: " confirmation
if [[ "${confirmation}" != "${confirmation_text}" ]]; then
  echo "aborted"
  exit 1
fi

burn_efuse() {
  printf '\nBurning eFuse:'
  printf ' %q' "$@"
  printf '\n'
  "${espefuse_cmd}" --chip esp32s3 --port "${port}" burn-efuse "$@"
}

case "${scope}" in
  jtag)
    burn_efuse DIS_PAD_JTAG
    burn_efuse DIS_USB_JTAG
    burn_efuse SOFT_DIS_JTAG 0x7
    burn_efuse DIS_USB_SERIAL_JTAG
    ;;
  download)
    burn_efuse DIS_DOWNLOAD_MODE
    burn_efuse DIS_USB_SERIAL_JTAG_DOWNLOAD_MODE
    burn_efuse DIS_USB_OTG_DOWNLOAD_MODE
    ;;
  direct-boot)
    burn_efuse DIS_DIRECT_BOOT
    ;;
  rom-print)
    burn_efuse DIS_USB_SERIAL_JTAG_ROM_PRINT
    ;;
  power-glitch)
    burn_efuse POWERGLITCH_EN
    ;;
esac
