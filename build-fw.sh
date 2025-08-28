#!/bin/zsh
. $HOME/export-esp.sh
# compile
cargo +esp -Z build-std=core,alloc build -p siger-fw --release --target xtensa-esp32s3-none-elf
