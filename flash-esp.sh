#!/bin/zsh
. $HOME/export-esp.sh
if [[ "$OSTYPE" == "darwin"* ]]; then
  DEV="cu.usbmodem1101"
else
  DEV="ttyACM0"
fi
fuser -k /dev/ttyACM0 2>/dev/null || true
cargo +esp build -p siger-fw --release --target xtensa-esp32s3-none-elf
espflash flash --port /dev/$DEV target/xtensa-esp32s3-none-elf/release/siger-fw

