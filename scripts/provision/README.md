# Provisioning Scripts

These scripts are deliberately separate from `make flash` and normal firmware
builds. They are for future production/security provisioning work and can make
a board difficult or impossible to recover if used incorrectly.

Safe status check:

```sh
make provision-summary PROVISION_PORT=/dev/ttyACM0
```

Dry-run provisioning checklist:

```sh
make provision-plan PROVISION_STAGE=production PROVISION_PORT=/dev/ttyACM0
```

This prints the ordered commands for HMAC_UP, secure boot v2, flash encryption,
strict release preflight, and final lockdown validation. It does not invoke
`espflash`, `espsecure`, `espefuse`, or `nockster-cli`.

Fresh-board one-shot provisioning:

```sh
make flash-prod-e2e
```

`flash-prod-e2e` is the single-confirmation shortcut for a fresh sacrificial
board. It defaults to `/dev/ttyACM0`, `../nockster-secrets/`, release version
`1` when `NOCKSTER_RELEASE_VERSION=0`, secure-boot digest slot `BLOCK_KEY0`,
and flash-encryption key slot `BLOCK_KEY4`. It generates missing key files
outside the repo, builds/signs/encrypts a fresh production image set under
`target/prod-e2e/<time>/`, prints the current eFuse summary, then asks once for
`FLASH-PROD-E2E`.

After that one prompt it burns HMAC_UP, flashes the signed secure-boot image,
burns the secure-boot digest, burns the flash-encryption key, enables secure
boot, flashes only encrypted artifacts, enables flash encryption, burns the
flash-time Unix timestamp into `BLOCK_USR_DATA`, and write-protects that eFuse
block before pausing for a manual normal reboot and HID validation. The
timestamp is the device's USB serial and survives every firmware update. A
production build refuses wallet initialization when this serial or the HMAC_UP
pepper is missing. At the reboot pause,
power-cycle the board or press EN/RESET, do not hold BOOT/download, wait for
HID to re-enumerate, then press Enter. It refuses to start the burn phase if
the selected key slots, `BLOCK_USR_DATA`, `SECURE_BOOT_EN`, or
`SPI_BOOT_CRYPT_CNT` are already set. Use `PROD_E2E_DRY_RUN=1` to print the
command order. Final lockdown and
power-glitch fuses remain separate targets.

Non-destructive device validation:

```sh
make validate-device-state VALIDATE_STAGE=smoke VALIDATE_PORT=hid
make validate-device-state VALIDATE_STAGE=production VALIDATE_PORT=hid
```

The validation wrapper runs scriptable `nockster-cli` status and expectation
checks for smoke, HMAC_UP/NVS-v2, OTA readiness, secure boot, flash encryption,
production lockdown, and power-glitch protection. It does not write eFuses,
seed the device, change PINs, or start update streams. Use
`VALIDATE_DRY_RUN=1` to print the commands first.

Non-destructive release preflight:

```sh
make release-preflight
```

By default this does not read or write eFuses. It checks the selected firmware
profile, release counter, update trust-anchor format, partition-table layout,
local secret-file hygiene for any key paths you provide, and whether
secret-looking paths have accidentally become tracked by git. When both
`UPDATE_SIGNING_KEY_FILE` and
`NOCKSTER_UPDATE_PUBKEY_SHA256_HEX` are provided, it derives the key's public
hash and requires it to match the configured firmware trust anchor. Include a
read-only chip status check only when you ask for it:

```sh
make release-preflight RUN_EFUSE_SUMMARY=1 PROVISION_PORT=/dev/ttyACM0
```

If Espressif tools are outside `PATH`, set `ESPSECURE=/path/to/espsecure` or
`ESPEFUSE=/path/to/espefuse`; preflight checks that configured command before
reporting it usable. Missing Espressif tools are warnings in the default
non-strict pass and failures in strict/production preflight.

For a stricter production pass, provide the local secret paths from outside the
repo plus the public update artifacts:

```sh
make signed-update-secure-boot-v2 \
  FW_PROFILE=production \
  ALLOW_UNSIGNED_PRODUCTION=1 \
  NOCKSTER_RELEASE_VERSION=1 \
  NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=<sha256-of-compressed-release-pubkey> \
  UPDATE_SIGNING_KEY_FILE=../nockster-secrets/release-signing-key.hex \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2-rsa.pem
```

```sh
make release-preflight \
  FW_PROFILE=production \
  RELEASE_PREFLIGHT_STRICT=1 \
  NOCKSTER_RELEASE_VERSION=1 \
  NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=<sha256-of-compressed-release-pubkey> \
  HMAC_KEY_FILE=../nockster-secrets/hmac-up.bin \
  UPDATE_SIGNING_KEY_FILE=../nockster-secrets/release-signing-key.hex \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2-rsa.pem \
  FLASH_ENCRYPTION_KEY_FILE=../nockster-secrets/flash-encryption-key.bin \
  UPDATE_BUNDLE=target/update/nockster-fw.update.json \
  UPDATE_FIRMWARE=target/update/nockster-fw.bin
```

It verifies the signed update bundle against the configured trust anchor and
reports bundle-verification failures without running any provisioning action.
If `UPDATE_INDEX` is provided, preflight also regenerates the expected browser
latest-release index from `UPDATE_BUNDLE` and `UPDATE_FIRMWARE` and compares it
to that file. Pass `UPDATE_BUNDLE_URL` and `UPDATE_FIRMWARE_URL` too when the
published index should contain hosted artifact URLs instead of default file
names.
The checked partition table must keep the custom `nvs` partition unflagged for
partition-level encryption unless raw NVS read/write testing has passed on this
board and `NVS_PARTITION_ENCRYPTION_VALIDATED=1` is set.

After the bundle verifies, generate the browser updater index from the same
public artifacts. Relative artifact URLs are preferred when the index, bundle,
and firmware are published in the same `/updates/` directory; explicit absolute
artifact URLs must be HTTPS, except localhost testing, so the generated index
matches the browser updater's fetch policy.

```sh
make update-index \
  UPDATE_BUNDLE=nockster-fw.update.json \
  UPDATE_FIRMWARE=target/xtensa-esp32s3-none-elf/release/nockster-fw.bin \
  UPDATE_INDEX=latest.json
```

Before publishing, validate that generated index against the signed bundle and
firmware:

```sh
make release-preflight \
  UPDATE_BUNDLE=nockster-fw.update.json \
  UPDATE_FIRMWARE=target/xtensa-esp32s3-none-elf/release/nockster-fw.bin \
  UPDATE_INDEX=latest.json
```

Publish `latest.json`, the bundle JSON, and the firmware image under the web
updater's configured release path. Keep `latest.json` mutable, but publish the
bundle and firmware under versioned names referenced by the index. The target
calls `nockster-cli update index`, which hashes the firmware against the signed
manifest before writing the index.
End users should consume that hosted index through the browser updater's
`update firmware` button; these CLI and Make commands are release-operator
tooling, not the normal upgrade path. The intended release UX is a hosted page:
plug in the device, click `update firmware`, approve the browser prompt, and
let the device validate the signed firmware before activating it.

For hardware-validation runs that need to exercise the same non-destructive
reboot request used by the hosted updater:

```sh
make validate-device-state VALIDATE_STAGE=reboot VALIDATE_PORT=hid
```

HMAC_UP key provisioning guard:

```sh
make generate-hmac-up-key HMAC_KEY_FILE=../nockster-secrets/hmac-up.bin
make provision-summary PROVISION_PORT=/dev/ttyACM0
make provision-hmac-up \
  PROVISION_PORT=/dev/ttyACM0 \
  HMAC_KEY_FILE=../nockster-secrets/hmac-up.bin \
  CONFIRM_IRREVERSIBLE=burn-hmac-up
```

`generate-hmac-up-key` only creates a local 32-byte key file, refuses to write
inside the repo, refuses to overwrite an existing file, and sets restrictive
permissions. It does not touch eFuses.

`provision-hmac-up` prints the current eFuse summary and asks for an additional
interactive confirmation before it calls `espefuse burn-key`. Set
`ESPEFUSE=/path/to/espefuse` if the Espressif tool is not on `PATH`.

After provisioning, a firmware built with `FW_PROFILE=chip-security` can use
the HMAC_UP peripheral output for NVS schema-v2 first initialization and
PIN-change rewrites. Default dev builds do not read this eFuse state and keep
using schema v1.

Use the CLI expectation flags after provisioning and after first
initialization/PIN-change migration:

```sh
nockster-cli security --port hid \
  --expect-chip-security \
  --expect-hmac-up \
  --expect-hmac-up-read-protected \
  --expect-nvs-v2
```

Secure boot v2 release signing:

```sh
make generate-secure-boot-v2-key \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2-rsa.pem

make update-firmware-image \
  FW_PROFILE=production ALLOW_UNSIGNED_PRODUCTION=1 \
  NOCKSTER_RELEASE_VERSION=<n> \
  NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=<sha256-of-release-pubkey> \
  UPDATE_FIRMWARE=target/secure-boot-v2/nockster-fw.factory.bin

make release-sign-secure-boot-v2 \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2-rsa.pem \
  SECURE_BOOT_IMAGE=target/secure-boot-v2/nockster-fw.factory.bin \
  SECURE_BOOT_SIGNED_IMAGE=target/secure-boot-v2/nockster-fw.factory.signed.bin

make release-build-secure-boot-v2-bootloader \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2-rsa.pem
```

`generate-secure-boot-v2-key` only creates a local secure-boot signing key,
refuses to write inside the repo, refuses to overwrite an existing file, and
sets restrictive permissions. For ESP32-S3 it creates an RSA3072 key. It does
not touch eFuses.
`release-sign-secure-boot-v2` refuses in-place signing, refuses to overwrite an
existing signed output, and first checks that the input looks like an ESP app
image that fits the configured app slot.
`release-build-secure-boot-v2-bootloader` uses ESP-IDF to build a signed
secure-boot-enabled second-stage bootloader with OTA rollback enabled, plus a
matching partition-table binary.

Put the board in serial bootloader/download mode and flash the signed
bootloader plus signed app without a normal reset:

```sh
make flash-secure-boot-v2 \
  FLASH_PORT=/dev/ttyACM0 \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2-rsa.pem \
  SECURE_BOOT_SIGNED_IMAGE=target/secure-boot-v2/nockster-fw.factory.signed.bin
```

`flash-secure-boot-v2` writes the signed secure-boot bootloader at `0x0`, the
partition table at `0x8000`, writes blank `otadata` at `0x310000`, writes the
signed app at `0x10000`, and leaves the chip in serial bootloader mode. Burn
the digest and `SECURE_BOOT_EN` before the first normal reset.

Secure boot digest provisioning is irreversible and intentionally separate from
release signing:

```sh
make provision-summary PROVISION_PORT=/dev/ttyACM0
make provision-secure-boot-v2-digest \
  PROVISION_PORT=/dev/ttyACM0 \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2-rsa.pem \
  CONFIRM_IRREVERSIBLE=burn-secure-boot-v2

make provision-secure-boot-v2-enable \
  PROVISION_PORT=/dev/ttyACM0 \
  CONFIRM_IRREVERSIBLE=enable-secure-boot-v2
```

`provision-secure-boot-v2-digest` prints the current eFuse summary and asks for
an additional interactive confirmation before it calls
`espefuse burn-key-digest`.
`provision-secure-boot-v2-enable` separately burns `SECURE_BOOT_EN`; run it
only after the signed bootloader/app and matching RSA digest are in place.

Flash encryption provisioning guards:

```sh
make generate-flash-encryption-key \
  FLASH_ENCRYPTION_KEY_FILE=../nockster-secrets/flash-encryption-key.bin

make release-build-secure-boot-v2-bootloader \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2-rsa.pem \
  SECURE_BOOT_BOOTLOADER_FLASH_ENCRYPTION=1

make release-encrypt-flash-v2-artifacts \
  FLASH_ENCRYPTION_KEY_FILE=../nockster-secrets/flash-encryption-key.bin \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2-rsa.pem \
  SECURE_BOOT_SIGNED_IMAGE=target/secure-boot-v2/nockster-fw.factory.signed.bin

make provision-summary PROVISION_PORT=/dev/ttyACM0

make provision-flash-encryption-key \
  PROVISION_PORT=/dev/ttyACM0 \
  FLASH_ENCRYPTION_KEY_FILE=../nockster-secrets/flash-encryption-key.bin \
  FLASH_ENCRYPTION_KEY_BLOCK=BLOCK_KEY4 \
  CONFIRM_IRREVERSIBLE=burn-flash-encryption-key

make flash-encrypted-secure-boot-v2 \
  FLASH_PORT=/dev/ttyACM0

make provision-flash-encryption-enable \
  PROVISION_PORT=/dev/ttyACM0 \
  CONFIRM_IRREVERSIBLE=enable-flash-encryption
```

`generate-flash-encryption-key` only creates a local 32-byte key file, refuses
to write inside the repo, refuses to overwrite an existing file, and sets
restrictive permissions. It does not touch eFuses.

`release-encrypt-flash-v2-artifacts` host-encrypts the signed secure-boot
bootloader at `0x0`, partition table at `0x8000`, signed factory app at
`0x10000`, and blank `otadata` at `0x310000` using ESP32-S3 AES-XTS flash
encryption. `flash-encrypted-secure-boot-v2` writes only those ciphertext
artifacts and leaves the board in serial bootloader mode.

`provision-flash-encryption-key` burns the key with purpose `XTS_AES_128_KEY`
and prints the current eFuse summary first. `provision-flash-encryption-enable`
burns `SPI_BOOT_CRYPT_CNT` separately. Keep these separate until a
sacrificial-board run has proven the signed/encrypted image and recovery flow.
Do not reset between `flash-encrypted-secure-boot-v2` and
`provision-flash-encryption-enable`.

After reset, validate both secure boot and flash encryption:

```sh
nockster-cli security --port hid \
  --expect-chip-security --expect-secure-boot --expect-flash-encryption
```

Production lockdown guards:

```sh
make provision-lockdown-jtag \
  PROVISION_PORT=/dev/ttyACM0 \
  CONFIRM_IRREVERSIBLE=disable-jtag

make provision-lockdown-download \
  PROVISION_PORT=/dev/ttyACM0 \
  CONFIRM_IRREVERSIBLE=disable-download-mode

make provision-lockdown-direct-boot \
  PROVISION_PORT=/dev/ttyACM0 \
  CONFIRM_IRREVERSIBLE=disable-direct-boot

make provision-lockdown-rom-print \
  PROVISION_PORT=/dev/ttyACM0 \
  CONFIRM_IRREVERSIBLE=disable-rom-print
```

These targets print the current eFuse summary and require a second interactive
confirmation before invoking `espefuse burn-efuse`. They are intentionally
separate; there is no one-shot lockdown target. Run them only after secure
boot, flash encryption, OTA recovery, and sacrificial-board tests pass.

Power-glitch protection is also separate until this exact board has been tested
for false positives:

```sh
make provision-power-glitch-protection \
  PROVISION_PORT=/dev/ttyACM0 \
  CONFIRM_IRREVERSIBLE=enable-power-glitch
```
