#!/bin/zsh
. $HOME/export-esp.sh
fuser -k /dev/ttyACM0 2>/dev/null || true
cargo +esp build -p siger-fw --release --target xtensa-esp32s3-none-elf
espflash flash --port /dev/ttyACM0 target/xtensa-esp32s3-none-elf/release/siger-fw

