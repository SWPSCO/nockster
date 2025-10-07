SHELL := /bin/zsh
.SHELLFLAGS := -c
TARGET_ESP := xtensa-esp32s3-none-elf
FW_BINARY := target/$(TARGET_ESP)/release/siger-fw
WASM_TOOLCHAIN := nightly
WASM_TARGET := wasm32-unknown-unknown

.PHONY: all build flash test clean fw cli core wasm js web

all: build

build: fw cli web

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
	espflash flash --port /dev/$$DEV --partition-table partitions.csv $(FW_BINARY)

wipe: fw
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
	espflash flash --port /dev/$$DEV --partition-table partitions.csv --erase-parts nvs $(FW_BINARY)

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
	cargo +nightly run -p siger-cli --bin siger-cli --target $$TARGET -- test --port $$DEVICE

cli:
	@if [[ "$$OSTYPE" == "darwin"* ]]; then \
		cargo build -p siger-cli --release --target aarch64-apple-darwin; \
	else \
		cargo build -p siger-cli --release --target x86_64-unknown-linux-gnu; \
	fi

core:
	@cargo build -p siger-core --release

wasm-setup:
	@rustup toolchain install $(WASM_TOOLCHAIN)
	@rustup +$(WASM_TOOLCHAIN) target add $(WASM_TARGET)
	@cargo +$(WASM_TOOLCHAIN) install wasm-pack --locked >/dev/null 2>&1 || true

wasm: wasm-setup
	@RUSTUP_TOOLCHAIN=$(WASM_TOOLCHAIN) \
	wasm-pack build crates/siger-wasm --target web --out-dir pkg

js:
	@cd siger-js; \
	npm install; \
	npm run build

web: wasm js
	@cd web; \
	npm install; \
	npm run build

serve: web
	@cd web; \
	npm run dev

clean:
	@cargo clean
	@rm -rf crates/siger-wasm/pkg
	@echo "Cleaned build artifacts and WASM pkg"

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
	@echo "  make flash      - Build and flash firmware (preserves keys)"
	@echo "    make fw         - Build firmware"
	@echo "    make wipe       - Build and flash firmware (erases keys)"
	@echo "  make test       - Run CLI tests against device"
	@echo "  make cli        - Build siger-cli tool"
	@echo "  make serve      - Build and serve web UI and dependencies"
	@echo "    make wasm       - Build WASM package for web"
	@echo "    make js         - Build siger-js lib for web"
	@echo "    make web        - Build demo UI for web"
	@echo "  make core       - Build siger-core library"
	@echo "  make monitor    - Open serial monitor"
	@echo "  make disconnect - Disconnect USB device"
	@echo "  make fmt        - Format code"
	@echo "  make clean      - Clean all build artifacts"
