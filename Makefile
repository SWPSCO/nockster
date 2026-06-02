SHELL := /bin/zsh
.SHELLFLAGS := -c
TARGET_ESP := xtensa-esp32s3-none-elf
FW_BINARY := target/$(TARGET_ESP)/release/nockster-fw
PARTITION_TABLE ?= partitions.csv
NOCKSTER_APP_SLOT_SIZE_BYTES ?= 3145728
ESP_CHIP ?= esp32s3
ESP_FLASH_SIZE ?= 16mb
WASM_TOOLCHAIN := nightly
WASM_TARGET := wasm32-unknown-unknown
FLASH_PORT ?=
WIPE_PORT ?=
FW_PROFILE ?=
FW_FEATURES ?=
NOCKSTER_RELEASE_VERSION ?= 0
ALLOW_UNSIGNED_PRODUCTION ?=
PROVISION_PORT ?=
HMAC_KEY_FILE ?=
SECURE_BOOT_KEY_FILE ?=
SECURE_BOOT_IMAGE ?=
SECURE_BOOT_SIGNED_IMAGE ?=
SECURE_BOOT_DIGEST_BLOCK ?= BLOCK_KEY0
FLASH_ENCRYPTION_KEY_FILE ?=
FLASH_ENCRYPTION_KEY_BLOCK ?= BLOCK_KEY4
FLASH_CRYPT_CNT_VALUE ?= 0x7
NVS_PARTITION_ENCRYPTION_VALIDATED ?= 0
LOCAL_UPDATE_SIGNING_KEY_FILE ?= .secrets/update-signing.key
UPDATE_SIGNING_KEY_FILE ?= $(LOCAL_UPDATE_SIGNING_KEY_FILE)
NOCKSTER_UPDATE_PUBKEY_SHA256_HEX ?= 5aa46209222080a2ce107e25d427c3d9ada6cb77be25d7d2a3df8959b7fa2602
UPDATE_ARTIFACT_DIR ?= target/update
UPDATE_BUNDLE ?= $(UPDATE_ARTIFACT_DIR)/nockster-fw.update.json
UPDATE_FIRMWARE ?= $(UPDATE_ARTIFACT_DIR)/nockster-fw.bin
UPDATE_INDEX ?= latest.json
UPDATE_BUNDLE_URL ?=
UPDATE_FIRMWARE_URL ?=
UPDATE_WEB_DIR ?= web/public/updates
UPDATE_WEB_INDEX ?= $(UPDATE_WEB_DIR)/latest.json
RUN_EFUSE_SUMMARY ?= 0
RELEASE_PREFLIGHT_STRICT ?=
NOCKSTER_CLI ?=
CONFIRM_IRREVERSIBLE ?=
PROVISION_STAGE ?= production
VALIDATE_STAGE ?= smoke
VALIDATE_PORT ?= hid
VALIDATE_BAUD ?= 115200
VALIDATE_DRY_RUN ?= 0
EFFECTIVE_FW_PROFILE := $(if $(strip $(FW_PROFILE)),$(strip $(FW_PROFILE)),$(if $(findstring chip-security,$(FW_FEATURES)),chip-security,dev))
ifneq ($(filter $(EFFECTIVE_FW_PROFILE),dev chip-security production),$(EFFECTIVE_FW_PROFILE))
$(error unsupported FW_PROFILE "$(EFFECTIVE_FW_PROFILE)" (expected dev, chip-security, or production))
endif
ifeq ($(EFFECTIVE_FW_PROFILE),dev)
FW_PROFILE_FEATURES :=
else ifeq ($(EFFECTIVE_FW_PROFILE),chip-security)
FW_PROFILE_FEATURES := chip-security
else ifeq ($(EFFECTIVE_FW_PROFILE),production)
FW_PROFILE_FEATURES := chip-security
endif
FW_EFFECTIVE_FEATURES := $(if $(strip $(FW_FEATURES)),$(strip $(FW_FEATURES)),$(FW_PROFILE_FEATURES))
FW_FEATURE_ARGS := $(if $(strip $(FW_EFFECTIVE_FEATURES)),--features "$(FW_EFFECTIVE_FEATURES)",)
.PHONY: all build flash test clean fw fw-dev fw-chip-security fw-production check-update-trust signed-update update-firmware-image cli core wasm js js-test web tauri tauri-dev tauri-build validate-device-state provision-plan provision-summary release-preflight generate-update-signing-key update-pubkey update-index update-web-assets generate-hmac-up-key provision-hmac-up generate-secure-boot-v2-key release-sign-secure-boot-v2 provision-secure-boot-v2-digest generate-flash-encryption-key provision-flash-encryption-key provision-flash-encryption-enable provision-lockdown-jtag provision-lockdown-download provision-lockdown-direct-boot provision-lockdown-rom-print provision-power-glitch-protection

all: build

build: fw cli web

fw:
	@. $(HOME)/export-esp.sh && \
	if [[ "$(EFFECTIVE_FW_PROFILE)" == "production" && "$(ALLOW_UNSIGNED_PRODUCTION)" != "1" ]]; then \
		echo "Refusing unsigned production firmware build."; \
		echo "Use FW_PROFILE=dev or FW_PROFILE=chip-security for test builds."; \
		echo "Set ALLOW_UNSIGNED_PRODUCTION=1 only for release-flow dry runs."; \
		exit 1; \
	fi; \
	if [[ -n "$(NOCKSTER_UPDATE_PUBKEY_SHA256_HEX)" ]]; then \
		NOCKSTER_BUILD_PROFILE="$(EFFECTIVE_FW_PROFILE)" \
		NOCKSTER_RELEASE_VERSION="$(NOCKSTER_RELEASE_VERSION)" \
		NOCKSTER_UPDATE_PUBKEY_SHA256_HEX="$(NOCKSTER_UPDATE_PUBKEY_SHA256_HEX)" \
		cargo +esp -Zbuild-std=core,alloc build -p nockster-fw --release --target $(TARGET_ESP) $(FW_FEATURE_ARGS); \
	else \
		unset NOCKSTER_UPDATE_PUBKEY_SHA256_HEX; \
		NOCKSTER_BUILD_PROFILE="$(EFFECTIVE_FW_PROFILE)" \
		NOCKSTER_RELEASE_VERSION="$(NOCKSTER_RELEASE_VERSION)" \
		cargo +esp -Zbuild-std=core,alloc build -p nockster-fw --release --target $(TARGET_ESP) $(FW_FEATURE_ARGS); \
	fi

fw-dev:
	@$(MAKE) fw FW_PROFILE=dev

fw-chip-security:
	@$(MAKE) fw FW_PROFILE=chip-security

fw-production:
	@echo "Production firmware is intentionally outside ordinary make flash/build."; \
	echo "Secure boot signing, flash encryption, and irreversible provisioning need explicit release scripts."; \
	echo "For dry-run metadata only: make fw FW_PROFILE=production ALLOW_UNSIGNED_PRODUCTION=1"; \
	exit 1

check-update-trust:
	@if [[ -z "$(NOCKSTER_UPDATE_PUBKEY_SHA256_HEX)" ]]; then \
		echo "signed updates require NOCKSTER_UPDATE_PUBKEY_SHA256_HEX to be compiled into firmware"; \
		exit 1; \
	fi

update-firmware-image: fw
	@set -e; \
	mkdir -p "$(dir $(UPDATE_FIRMWARE))"; \
	espflash save-image \
		--chip "$(ESP_CHIP)" \
		--flash-size "$(ESP_FLASH_SIZE)" \
		--partition-table "$(PARTITION_TABLE)" \
		--target-app-partition factory \
		"$(FW_BINARY)" \
		"$(UPDATE_FIRMWARE)"; \
	bash scripts/provision/check-release-image.sh \
		"$(UPDATE_FIRMWARE)" \
		"$(NOCKSTER_APP_SLOT_SIZE_BYTES)" \
		"update firmware image"

signed-update: check-update-trust update-firmware-image
	@set -e; \
	if [[ -z "$(UPDATE_SIGNING_KEY_FILE)" ]]; then \
		echo "Set UPDATE_SIGNING_KEY_FILE=/path/to/release-signing-key.hex"; \
		exit 1; \
	fi; \
	mkdir -p "$(dir $(UPDATE_BUNDLE))"; \
	args=(update sign \
		--firmware "$(UPDATE_FIRMWARE)" \
		--out "$(UPDATE_BUNDLE)" \
		--signing-key-file "$(UPDATE_SIGNING_KEY_FILE)" \
		--release-version "$(NOCKSTER_RELEASE_VERSION)" \
		--hardware-target esp32s3-touch-lcd-1.47 \
		--build-profile "$(EFFECTIVE_FW_PROFILE)"); \
	if [[ -n "$(NOCKSTER_CLI)" ]]; then \
		"$(NOCKSTER_CLI)" "$${args[@]}"; \
	else \
		cargo run -p nockster-cli --bin nockster-cli -- "$${args[@]}"; \
	fi; \
	verify_args=(update verify \
		--bundle "$(UPDATE_BUNDLE)" \
		--firmware "$(UPDATE_FIRMWARE)" \
		--trusted-pubkey-sha256 "$(NOCKSTER_UPDATE_PUBKEY_SHA256_HEX)"); \
	if [[ -n "$(NOCKSTER_CLI)" ]]; then \
		"$(NOCKSTER_CLI)" "$${verify_args[@]}"; \
	else \
		cargo run -p nockster-cli --bin nockster-cli -- "$${verify_args[@]}"; \
	fi

flash: fw
	@if [[ -n "$(FLASH_PORT)" ]]; then \
		DEV="$(FLASH_PORT)"; \
	elif [[ "$$OSTYPE" == "darwin"* ]]; then \
		DEV=$$(ls /dev/cu.usbmodem* | head -1 | xargs basename); \
		if [[ -z "$$DEV" ]]; then \
			echo "No USB serial device found"; \
			exit 1; \
		fi; \
	else \
		if ls /dev/ttyACM* >/dev/null 2>&1; then \
			DEV=$$(ls /dev/ttyACM* | head -1 | xargs basename); \
		elif ls /dev/ttyUSB* >/dev/null 2>&1; then \
			DEV=$$(ls /dev/ttyUSB* | head -1 | xargs basename); \
		elif ls /dev/hidraw* >/dev/null 2>&1; then \
			DEV="hid"; \
		else \
			echo "No USB serial or HID device found"; \
			exit 1; \
		fi; \
	fi; \
	if [[ "$$DEV" == hid* ]]; then \
		echo "Device is in HID mode; espflash needs a USB-serial/CDC port."; \
		echo "Put the device in bootloader mode and rerun with FLASH_PORT=/dev/ttyACM0."; \
		exit 1; \
	fi; \
	PORT_PATH="/dev/$$DEV"; \
	if [[ "$$DEV" == /dev/* ]]; then \
		PORT_PATH="$$DEV"; \
	fi; \
	if [[ "$$OSTYPE" != "darwin"* ]]; then \
		fuser -k "$$PORT_PATH" 2>/dev/null || true; \
	fi; \
	espflash erase-parts --port "$$PORT_PATH" --partition-table "$(PARTITION_TABLE)" --after no-reset otadata; \
	espflash flash --port "$$PORT_PATH" --partition-table "$(PARTITION_TABLE)" $(FW_BINARY); #\
	#pyserial-miniterm --dtr 0 --rts 0 /dev/$$DEV 115200

wipe: signed-update
	@if [[ -n "$(WIPE_PORT)" ]]; then \
		DEV="$(WIPE_PORT)"; \
	elif [[ "$$OSTYPE" == "darwin"* ]]; then \
		DEV=$$(ls /dev/cu.usbmodem* | head -1 | xargs basename); \
		if [[ -z "$$DEV" ]]; then \
			echo "No USB serial device found"; \
			exit 1; \
		fi; \
	else \
		if ls /dev/ttyACM* >/dev/null 2>&1; then \
			DEV=$$(ls /dev/ttyACM* | head -1 | xargs basename); \
		elif ls /dev/hidraw* >/dev/null 2>&1; then \
			DEV="hid"; \
		else \
			echo "No USB serial or HID device found"; \
			exit 1; \
		fi; \
	fi; \
	if [[ "$$DEV" == hid* ]]; then \
		echo "HID mode: signed update artifacts are built, but serial bootloader mode is required to flash this firmware while wiping NVS."; \
		cargo run -p nockster-cli --bin nockster-cli -- reset --port "$$DEV"; \
	else \
		PORT_PATH="/dev/$$DEV"; \
		if [[ "$$DEV" == /dev/* ]]; then \
			PORT_PATH="$$DEV"; \
		fi; \
		if [[ "$$OSTYPE" != "darwin"* ]]; then \
			fuser -k "$$PORT_PATH" 2>/dev/null || true; \
		fi; \
		espflash erase-parts --port "$$PORT_PATH" --partition-table "$(PARTITION_TABLE)" --after no-reset nvs otadata; \
		espflash flash --port "$$PORT_PATH" --partition-table "$(PARTITION_TABLE)" $(FW_BINARY); \
	fi

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
	cargo +nightly run -p nockster-cli --bin nockster-cli --target $$TARGET -- test --port $$DEVICE

cli:
	@if [[ "$$OSTYPE" == "darwin"* ]]; then \
		cargo build -p nockster-cli --release --target aarch64-apple-darwin; \
	else \
		cargo build -p nockster-cli --release --target x86_64-unknown-linux-gnu; \
	fi

core:
	@cargo build -p nockster-core --release

wasm-setup:
	@rustup toolchain install $(WASM_TOOLCHAIN)
	@rustup +$(WASM_TOOLCHAIN) target add $(WASM_TARGET)
	@cargo +$(WASM_TOOLCHAIN) install wasm-pack --locked >/dev/null 2>&1 || true

wasm: wasm-setup
	@RUSTUP_TOOLCHAIN=$(WASM_TOOLCHAIN) \
	wasm-pack build crates/nockster-wasm --target web --out-dir pkg

js:
	@cd nockster-js; \
	npm install; \
	npm run build

js-test:
	@cd nockster-js; \
	npm install; \
	npm test

web: wasm js
	@cd web; \
	npm install; \
	npm run build

serve: web
	@if [[ ! -f "web/public/updates/latest.json" ]]; then \
		echo "No local update index at web/public/updates/latest.json"; \
		echo "The web UI will still run, but update firmware will fail until you publish update assets."; \
		echo "Run: make update-web-assets UPDATE_BUNDLE=/path/to/nockster-fw.update.json UPDATE_FIRMWARE=/path/to/nockster-fw.bin"; \
	fi
	@cd web; \
	npm run dev

clean:
	@cargo clean
	@rm -rf crates/nockster-wasm/pkg
	@echo "Cleaned build artifacts and WASM pkg"

provision-summary:
	@if [[ -z "$(PROVISION_PORT)" ]]; then \
		echo "Set PROVISION_PORT=/dev/ttyACM0"; \
		exit 1; \
	fi; \
	bash scripts/provision/efuse-summary.sh "$(PROVISION_PORT)"

release-preflight:
	@FW_PROFILE="$(EFFECTIVE_FW_PROFILE)" \
	NOCKSTER_RELEASE_VERSION="$(NOCKSTER_RELEASE_VERSION)" \
	NOCKSTER_UPDATE_PUBKEY_SHA256_HEX="$(NOCKSTER_UPDATE_PUBKEY_SHA256_HEX)" \
	NOCKSTER_APP_SLOT_SIZE_BYTES="$(NOCKSTER_APP_SLOT_SIZE_BYTES)" \
	PARTITION_TABLE="$(PARTITION_TABLE)" \
	NVS_PARTITION_ENCRYPTION_VALIDATED="$(NVS_PARTITION_ENCRYPTION_VALIDATED)" \
	HMAC_KEY_FILE="$(HMAC_KEY_FILE)" \
	UPDATE_SIGNING_KEY_FILE="$(UPDATE_SIGNING_KEY_FILE)" \
	SECURE_BOOT_KEY_FILE="$(SECURE_BOOT_KEY_FILE)" \
	SECURE_BOOT_IMAGE="$(SECURE_BOOT_IMAGE)" \
	SECURE_BOOT_SIGNED_IMAGE="$(SECURE_BOOT_SIGNED_IMAGE)" \
	FLASH_ENCRYPTION_KEY_FILE="$(FLASH_ENCRYPTION_KEY_FILE)" \
	UPDATE_BUNDLE="$(UPDATE_BUNDLE)" \
	UPDATE_FIRMWARE="$(UPDATE_FIRMWARE)" \
	UPDATE_INDEX="$(if $(filter command line environment environment override,$(origin UPDATE_INDEX)),$(UPDATE_INDEX),)" \
	UPDATE_BUNDLE_URL="$(UPDATE_BUNDLE_URL)" \
	UPDATE_FIRMWARE_URL="$(UPDATE_FIRMWARE_URL)" \
	PROVISION_PORT="$(PROVISION_PORT)" \
	RUN_EFUSE_SUMMARY="$(RUN_EFUSE_SUMMARY)" \
	RELEASE_PREFLIGHT_STRICT="$(RELEASE_PREFLIGHT_STRICT)" \
	NOCKSTER_CLI="$(NOCKSTER_CLI)" \
	bash scripts/provision/release-preflight.sh

provision-plan:
	@PROVISION_STAGE="$(PROVISION_STAGE)" \
	PROVISION_PORT="$(PROVISION_PORT)" \
	NOCKSTER_RELEASE_VERSION="$(NOCKSTER_RELEASE_VERSION)" \
	NOCKSTER_UPDATE_PUBKEY_SHA256_HEX="$(NOCKSTER_UPDATE_PUBKEY_SHA256_HEX)" \
	HMAC_KEY_FILE="$(HMAC_KEY_FILE)" \
	UPDATE_SIGNING_KEY_FILE="$(UPDATE_SIGNING_KEY_FILE)" \
	SECURE_BOOT_KEY_FILE="$(SECURE_BOOT_KEY_FILE)" \
	SECURE_BOOT_IMAGE="$(SECURE_BOOT_IMAGE)" \
	SECURE_BOOT_SIGNED_IMAGE="$(SECURE_BOOT_SIGNED_IMAGE)" \
	SECURE_BOOT_DIGEST_BLOCK="$(SECURE_BOOT_DIGEST_BLOCK)" \
	FLASH_ENCRYPTION_KEY_FILE="$(FLASH_ENCRYPTION_KEY_FILE)" \
	FLASH_ENCRYPTION_KEY_BLOCK="$(FLASH_ENCRYPTION_KEY_BLOCK)" \
	FLASH_CRYPT_CNT_VALUE="$(FLASH_CRYPT_CNT_VALUE)" \
	UPDATE_BUNDLE="$(UPDATE_BUNDLE)" \
	UPDATE_FIRMWARE="$(UPDATE_FIRMWARE)" \
	bash scripts/provision/provision-plan.sh

validate-device-state:
	@VALIDATE_STAGE="$(VALIDATE_STAGE)" \
	VALIDATE_PORT="$(VALIDATE_PORT)" \
	VALIDATE_BAUD="$(VALIDATE_BAUD)" \
	VALIDATE_DRY_RUN="$(VALIDATE_DRY_RUN)" \
	NOCKSTER_CLI="$(NOCKSTER_CLI)" \
	bash scripts/provision/validate-device-state.sh

generate-update-signing-key:
	@if [[ -z "$(UPDATE_SIGNING_KEY_FILE)" ]]; then \
		echo "Set UPDATE_SIGNING_KEY_FILE=/path/to/release-signing-key.hex"; \
		exit 1; \
	fi; \
	bash scripts/provision/check-secret-output-path.sh "$(UPDATE_SIGNING_KEY_FILE)" "release signing key file"; \
	if [[ -n "$(NOCKSTER_CLI)" ]]; then \
		"$(NOCKSTER_CLI)" update keygen --out "$(UPDATE_SIGNING_KEY_FILE)"; \
	else \
		cargo run -p nockster-cli --bin nockster-cli -- update keygen --out "$(UPDATE_SIGNING_KEY_FILE)"; \
	fi

update-pubkey:
	@if [[ -z "$(UPDATE_SIGNING_KEY_FILE)" ]]; then \
		echo "Set UPDATE_SIGNING_KEY_FILE=/path/to/release-signing-key.hex"; \
		exit 1; \
	fi; \
	if [[ -n "$(NOCKSTER_CLI)" ]]; then \
		"$(NOCKSTER_CLI)" update pubkey --signing-key-file "$(UPDATE_SIGNING_KEY_FILE)"; \
	else \
		cargo run -p nockster-cli --bin nockster-cli -- update pubkey --signing-key-file "$(UPDATE_SIGNING_KEY_FILE)"; \
	fi

update-index:
	@if [[ -z "$(UPDATE_BUNDLE)" ]]; then \
		echo "Set UPDATE_BUNDLE=/path/to/nockster-fw.update.json"; \
		exit 1; \
	fi; \
	if [[ -z "$(UPDATE_FIRMWARE)" ]]; then \
		echo "Set UPDATE_FIRMWARE=/path/to/nockster-fw.bin"; \
		exit 1; \
	fi; \
	if [[ -z "$(UPDATE_INDEX)" ]]; then \
		echo "Set UPDATE_INDEX=/path/to/latest.json"; \
		exit 1; \
	fi; \
	args=(update index --bundle "$(UPDATE_BUNDLE)" --firmware "$(UPDATE_FIRMWARE)" --out "$(UPDATE_INDEX)"); \
	if [[ -n "$(UPDATE_BUNDLE_URL)" ]]; then \
		args+=(--bundle-url "$(UPDATE_BUNDLE_URL)"); \
	fi; \
	if [[ -n "$(UPDATE_FIRMWARE_URL)" ]]; then \
		args+=(--firmware-url "$(UPDATE_FIRMWARE_URL)"); \
	fi; \
	if [[ -n "$(NOCKSTER_CLI)" ]]; then \
		"$(NOCKSTER_CLI)" "$${args[@]}"; \
	else \
		cargo run -p nockster-cli --bin nockster-cli -- "$${args[@]}"; \
	fi

update-web-assets:
	@set -e; \
	if [[ -z "$(UPDATE_BUNDLE)" ]]; then \
		echo "Set UPDATE_BUNDLE=/path/to/nockster-fw.update.json"; \
		exit 1; \
	fi; \
	if [[ -z "$(UPDATE_FIRMWARE)" ]]; then \
		echo "Set UPDATE_FIRMWARE=/path/to/nockster-fw.bin"; \
		exit 1; \
	fi; \
	mkdir -p "$(UPDATE_WEB_DIR)"; \
	bundle_name="$$(basename "$(UPDATE_BUNDLE)")"; \
	firmware_name="$$(basename "$(UPDATE_FIRMWARE)")"; \
	$(MAKE) update-index \
		UPDATE_BUNDLE="$(UPDATE_BUNDLE)" \
		UPDATE_FIRMWARE="$(UPDATE_FIRMWARE)" \
		UPDATE_INDEX="$(UPDATE_WEB_INDEX)" \
		UPDATE_BUNDLE_URL="$${bundle_name}" \
		UPDATE_FIRMWARE_URL="$${firmware_name}" \
		NOCKSTER_CLI="$(NOCKSTER_CLI)"; \
	cp "$(UPDATE_BUNDLE)" "$(UPDATE_WEB_DIR)/$${bundle_name}"; \
	cp "$(UPDATE_FIRMWARE)" "$(UPDATE_WEB_DIR)/$${firmware_name}"; \
	echo "wrote local web update assets under $(UPDATE_WEB_DIR)"

generate-hmac-up-key:
	@if [[ -z "$(HMAC_KEY_FILE)" ]]; then \
		echo "Set HMAC_KEY_FILE=/path/to/hmac-up.bin"; \
		exit 1; \
	fi; \
	bash scripts/provision/generate-hmac-up-key.sh "$(HMAC_KEY_FILE)"

provision-hmac-up:
	@if [[ "$(CONFIRM_IRREVERSIBLE)" != "burn-hmac-up" ]]; then \
		echo "Refusing irreversible eFuse write."; \
		echo "Run provision-summary first, then rerun with CONFIRM_IRREVERSIBLE=burn-hmac-up."; \
		exit 1; \
	fi; \
	if [[ -z "$(PROVISION_PORT)" ]]; then \
		echo "Set PROVISION_PORT=/dev/ttyACM0"; \
		exit 1; \
	fi; \
	if [[ -z "$(HMAC_KEY_FILE)" ]]; then \
		echo "Set HMAC_KEY_FILE=/path/to/hmac-up.bin"; \
		exit 1; \
	fi; \
	bash scripts/provision/burn-hmac-up.sh "$(PROVISION_PORT)" "$(HMAC_KEY_FILE)"

generate-secure-boot-v2-key:
	@if [[ -z "$(SECURE_BOOT_KEY_FILE)" ]]; then \
		echo "Set SECURE_BOOT_KEY_FILE=/path/to/secure-boot-v2.pem"; \
		exit 1; \
	fi; \
	bash scripts/provision/generate-secure-boot-v2-key.sh "$(SECURE_BOOT_KEY_FILE)"

release-sign-secure-boot-v2:
	@if [[ -z "$(SECURE_BOOT_KEY_FILE)" ]]; then \
		echo "Set SECURE_BOOT_KEY_FILE=/path/to/secure-boot-v2.pem"; \
		exit 1; \
	fi; \
	if [[ -z "$(SECURE_BOOT_IMAGE)" ]]; then \
		echo "Set SECURE_BOOT_IMAGE=/path/to/unsigned-app-image.bin"; \
		exit 1; \
	fi; \
	if [[ -z "$(SECURE_BOOT_SIGNED_IMAGE)" ]]; then \
		echo "Set SECURE_BOOT_SIGNED_IMAGE=/path/to/signed-app-image.bin"; \
		exit 1; \
	fi; \
	bash scripts/provision/sign-secure-boot-v2.sh "$(SECURE_BOOT_KEY_FILE)" "$(SECURE_BOOT_IMAGE)" "$(SECURE_BOOT_SIGNED_IMAGE)"

provision-secure-boot-v2-digest:
	@if [[ "$(CONFIRM_IRREVERSIBLE)" != "burn-secure-boot-v2" ]]; then \
		echo "Refusing irreversible secure-boot eFuse write."; \
		echo "Run provision-summary first, then rerun with CONFIRM_IRREVERSIBLE=burn-secure-boot-v2."; \
		exit 1; \
	fi; \
	if [[ -z "$(PROVISION_PORT)" ]]; then \
		echo "Set PROVISION_PORT=/dev/ttyACM0"; \
		exit 1; \
	fi; \
	if [[ -z "$(SECURE_BOOT_KEY_FILE)" ]]; then \
		echo "Set SECURE_BOOT_KEY_FILE=/path/to/secure-boot-v2.pem"; \
		exit 1; \
	fi; \
	bash scripts/provision/burn-secure-boot-v2-digest.sh "$(PROVISION_PORT)" "$(SECURE_BOOT_KEY_FILE)" "$(SECURE_BOOT_DIGEST_BLOCK)"

generate-flash-encryption-key:
	@if [[ -z "$(FLASH_ENCRYPTION_KEY_FILE)" ]]; then \
		echo "Set FLASH_ENCRYPTION_KEY_FILE=/path/to/flash-encryption-key.bin"; \
		exit 1; \
	fi; \
	bash scripts/provision/generate-flash-encryption-key.sh "$(FLASH_ENCRYPTION_KEY_FILE)"

provision-flash-encryption-key:
	@if [[ "$(CONFIRM_IRREVERSIBLE)" != "burn-flash-encryption-key" ]]; then \
		echo "Refusing irreversible flash encryption key eFuse write."; \
		echo "Run provision-summary first, then rerun with CONFIRM_IRREVERSIBLE=burn-flash-encryption-key."; \
		exit 1; \
	fi; \
	if [[ -z "$(PROVISION_PORT)" ]]; then \
		echo "Set PROVISION_PORT=/dev/ttyACM0"; \
		exit 1; \
	fi; \
	if [[ -z "$(FLASH_ENCRYPTION_KEY_FILE)" ]]; then \
		echo "Set FLASH_ENCRYPTION_KEY_FILE=/path/to/flash-encryption-key.bin"; \
		exit 1; \
	fi; \
	bash scripts/provision/burn-flash-encryption-key.sh "$(PROVISION_PORT)" "$(FLASH_ENCRYPTION_KEY_FILE)" "$(FLASH_ENCRYPTION_KEY_BLOCK)"

provision-flash-encryption-enable:
	@if [[ "$(CONFIRM_IRREVERSIBLE)" != "enable-flash-encryption" ]]; then \
		echo "Refusing irreversible flash encryption enable eFuse write."; \
		echo "Run provision-summary first, then rerun with CONFIRM_IRREVERSIBLE=enable-flash-encryption."; \
		exit 1; \
	fi; \
	if [[ -z "$(PROVISION_PORT)" ]]; then \
		echo "Set PROVISION_PORT=/dev/ttyACM0"; \
		exit 1; \
	fi; \
	bash scripts/provision/enable-flash-encryption.sh "$(PROVISION_PORT)" "$(FLASH_CRYPT_CNT_VALUE)"

provision-lockdown-jtag:
	@if [[ "$(CONFIRM_IRREVERSIBLE)" != "disable-jtag" ]]; then \
		echo "Refusing irreversible JTAG lockdown eFuse writes."; \
		echo "Run provision-summary first, then rerun with CONFIRM_IRREVERSIBLE=disable-jtag."; \
		exit 1; \
	fi; \
	if [[ -z "$(PROVISION_PORT)" ]]; then \
		echo "Set PROVISION_PORT=/dev/ttyACM0"; \
		exit 1; \
	fi; \
	bash scripts/provision/burn-lockdown-fuse.sh "$(PROVISION_PORT)" jtag

provision-lockdown-download:
	@if [[ "$(CONFIRM_IRREVERSIBLE)" != "disable-download-mode" ]]; then \
		echo "Refusing irreversible download-mode lockdown eFuse writes."; \
		echo "Run provision-summary first, then rerun with CONFIRM_IRREVERSIBLE=disable-download-mode."; \
		exit 1; \
	fi; \
	if [[ -z "$(PROVISION_PORT)" ]]; then \
		echo "Set PROVISION_PORT=/dev/ttyACM0"; \
		exit 1; \
	fi; \
	bash scripts/provision/burn-lockdown-fuse.sh "$(PROVISION_PORT)" download

provision-lockdown-direct-boot:
	@if [[ "$(CONFIRM_IRREVERSIBLE)" != "disable-direct-boot" ]]; then \
		echo "Refusing irreversible direct-boot lockdown eFuse write."; \
		echo "Run provision-summary first, then rerun with CONFIRM_IRREVERSIBLE=disable-direct-boot."; \
		exit 1; \
	fi; \
	if [[ -z "$(PROVISION_PORT)" ]]; then \
		echo "Set PROVISION_PORT=/dev/ttyACM0"; \
		exit 1; \
	fi; \
	bash scripts/provision/burn-lockdown-fuse.sh "$(PROVISION_PORT)" direct-boot

provision-lockdown-rom-print:
	@if [[ "$(CONFIRM_IRREVERSIBLE)" != "disable-rom-print" ]]; then \
		echo "Refusing irreversible ROM-print lockdown eFuse write."; \
		echo "Run provision-summary first, then rerun with CONFIRM_IRREVERSIBLE=disable-rom-print."; \
		exit 1; \
	fi; \
	if [[ -z "$(PROVISION_PORT)" ]]; then \
		echo "Set PROVISION_PORT=/dev/ttyACM0"; \
		exit 1; \
	fi; \
	bash scripts/provision/burn-lockdown-fuse.sh "$(PROVISION_PORT)" rom-print

provision-power-glitch-protection:
	@if [[ "$(CONFIRM_IRREVERSIBLE)" != "enable-power-glitch" ]]; then \
		echo "Refusing irreversible power-glitch eFuse write."; \
		echo "Run provision-summary first, then rerun with CONFIRM_IRREVERSIBLE=enable-power-glitch."; \
		exit 1; \
	fi; \
	if [[ -z "$(PROVISION_PORT)" ]]; then \
		echo "Set PROVISION_PORT=/dev/ttyACM0"; \
		exit 1; \
	fi; \
	bash scripts/provision/burn-lockdown-fuse.sh "$(PROVISION_PORT)" power-glitch

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

tauri-dev: web
	@cd src-tauri && cargo tauri dev

tauri-build: web
	@cd src-tauri && cargo tauri build

tauri: tauri-build

help:
	@echo "Available targets:"
	@echo "  make flash      - Build and flash firmware (preserves keys; FLASH_PORT=/dev/ttyACM0)"
	@echo "    make fw         - Build firmware (FW_PROFILE=dev|chip-security|production)"
	@echo "    make fw-dev     - Build default dev firmware"
	@echo "    make fw-chip-security - Build firmware with read-only chip security status"
	@echo "    make fw-production - Refuse until signed/encrypted release flow is explicit"
	@echo "    make signed-update - Build OTA .bin and signed update bundle"
	@echo "    make wipe       - Build signed update artifacts; serial flashes+wipes, HID resets storage"
	@echo "    make provision-summary - Show eFuse summary (PROVISION_PORT=/dev/ttyACM0)"
	@echo "    make provision-plan - Print a non-destructive provisioning checklist"
	@echo "    make validate-device-state - Run scriptable device security/update checks"
	@echo "    make release-preflight - Non-destructive release/provisioning checks"
	@echo "    make generate-update-signing-key - Generate a local update release signing key"
	@echo "    make update-pubkey - Print update release public key and trust hash"
	@echo "    make update-index - Generate latest.json for browser firmware updates"
	@echo "    make update-web-assets - Publish local update files for make serve"
	@echo "    make generate-hmac-up-key - Generate a local 32-byte HMAC_UP key file"
	@echo "    make provision-hmac-up - Explicit HMAC_UP eFuse provisioning guard"
	@echo "    make generate-secure-boot-v2-key - Generate a local secure boot v2 signing key"
	@echo "    make release-sign-secure-boot-v2 - Sign a built app image for secure boot v2"
	@echo "    make provision-secure-boot-v2-digest - Explicit secure boot v2 digest burn guard"
	@echo "    make generate-flash-encryption-key - Generate a local flash encryption key"
	@echo "    make provision-flash-encryption-key - Explicit flash encryption key burn guard"
	@echo "    make provision-flash-encryption-enable - Explicit SPI_BOOT_CRYPT_CNT burn guard"
	@echo "    make provision-lockdown-jtag - Explicit production JTAG lockdown guard"
	@echo "    make provision-lockdown-download - Explicit production download-mode lockdown guard"
	@echo "    make provision-lockdown-direct-boot - Explicit production direct-boot lockdown guard"
	@echo "    make provision-lockdown-rom-print - Explicit production ROM-print lockdown guard"
	@echo "    make provision-power-glitch-protection - Explicit power-glitch protection guard"
	@echo "  make test       - Run CLI tests against device"
	@echo "  make cli        - Build nockster-cli tool"
	@echo "  make serve      - Build and serve web UI and dependencies"
	@echo "    make wasm       - Build WASM package for web"
	@echo "    make js         - Build nockster-js lib for web"
	@echo "    make js-test    - Run nockster-js protocol/update parser tests"
	@echo "    make web        - Build demo UI for web"
	@echo "  make tauri-dev  - Run Tauri desktop app in dev mode"
	@echo "  make tauri-build- Build Tauri desktop app for production"
	@echo "  make tauri      - Build Tauri desktop app (alias for tauri-build)"
	@echo "  make core       - Build nockster-core library"
	@echo "  make monitor    - Open serial monitor"
	@echo "  make disconnect - Disconnect USB device"
	@echo "  make fmt        - Format code"
	@echo "  make clean      - Clean all build artifacts"
