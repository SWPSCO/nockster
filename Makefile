SHELL := /bin/zsh
.SHELLFLAGS := -c

TARGET_ESP := xtensa-esp32s3-none-elf
FW_BINARY := target/$(TARGET_ESP)/release/siger-fw

.PHONY: all build flash test clean fw cli core nicker

all: build

build: fw cli

fw:
	@. $(HOME)/export-esp.sh && \
	cargo +esp -Zbuild-std=core,alloc build -p siger-fw --release --target $(TARGET_ESP)

flash: fw
	@if [[ "$$OSTYPE" == "darwin"* ]]; then \
		DEV=$$(ls /dev/cu.usbmodem* | head -1 | xargs basename); \
		if [[ -z "$$DEV" ]]; then \
			echo "No USB serial device found"; \
			exit 1; \
		fi; \
	else \
		DEV="ttyACM0"; \
		fuser -k /dev/ttyACM0 2>/dev/null || true; \
	fi; \
	espflash flash --port /dev/$$DEV $(FW_BINARY)

test:
	@fuser -k /dev/ttyACM0 2>/dev/null || true; \
	if [[ "$$OSTYPE" == "darwin"* ]]; then \
		TARGET=aarch64-apple-darwin; \
		DEVICE=$$(ls /dev/tty.usbmodem* 2>/dev/null | head -1); \
		if [ -z "$$DEVICE" ]; then \
			DEVICE=$$(ls /dev/cu.usbmodem* 2>/dev/null | head -1); \
		fi; \
		if [ -z "$$DEVICE" ]; then \
			echo "No USB serial device found"; \
			exit 1; \
		fi; \
	else \
		TARGET=x86_64-unknown-linux-gnu; \
		DEVICE=/dev/ttyACM0; \
	fi; \
	cargo +nightly run -p siger-cli --target $$TARGET -- $$DEVICE

cli:
	@if [[ "$$OSTYPE" == "darwin"* ]]; then \
		cargo build -p siger-cli --release --target aarch64-apple-darwin; \
	else \
		cargo build -p siger-cli --release --target x86_64-unknown-linux-gnu; \
	fi

core:
	@cargo build -p siger-core --release

nicker:
	@cargo build -p nicker --release

clean:
	@cargo clean

monitor:
	@if [[ "$$OSTYPE" == "darwin"* ]]; then \
		DEV=$$(ls /dev/cu.usbmodem* | head -1 | xargs basename); \
		if [[ -z "$$DEV" ]]; then \
			echo "No USB serial device found"; \
			exit 1; \
		fi; \
	else \
		DEV="ttyACM0"; \
	fi; \
	espflash monitor --port /dev/$$DEV

check:
	@cargo check --workspace

clippy:
	@cargo clippy --workspace -- -D warnings

fmt:
	@cargo fmt --all

disconnect:
	@if [[ "$$OSTYPE" == "darwin"* ]]; then \
		DEV=$$(ls /dev/cu.usbmodem* | head -1 | xargs basename); \
		if [[ -z "$$DEV" ]]; then \
			echo "No USB serial device found"; \
			exit 1; \
		fi; \
	else \
		DEV=$$(ls /dev/ttyACM* | head -1 | xargs basename); \
		if [[ -z "$$DEV" ]]; then \
			echo "No USB serial device found"; \
			exit 1; \
		fi; \
	fi; \
	fuser -k /dev/$$DEV 2>/dev/null || true

help:
	@echo "Available targets:"
	@echo "  make fw      - Build ESP32-S3 firmware"
	@echo "  make flash   - Build and flash firmware to device"
	@echo "  make test    - Run CLI tests against device"
	@echo "  make cli     - Build CLI tool"
	@echo "  make core    - Build core library"
	@echo "  make nicker  - Build nicker crate"
	@echo "  make clean   - Clean all build artifacts"
	@echo "  make monitor - Open serial monitor"
	@echo "  make check   - Check all crates"
	@echo "  make clippy  - Run clippy lints"
	@echo "  disconnect   - Disconnect USB device"
	@echo "  make fmt     - Format code"