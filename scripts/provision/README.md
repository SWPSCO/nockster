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
make release-preflight \
  FW_PROFILE=production \
  RELEASE_PREFLIGHT_STRICT=1 \
  NOCKSTER_RELEASE_VERSION=1 \
  NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=<sha256-of-compressed-release-pubkey> \
  HMAC_KEY_FILE=../nockster-secrets/hmac-up.bin \
  UPDATE_SIGNING_KEY_FILE=../nockster-secrets/release-signing-key.hex \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2.pem \
  FLASH_ENCRYPTION_KEY_FILE=../nockster-secrets/flash-encryption-key.bin \
  UPDATE_BUNDLE=nockster-fw.update.json \
  UPDATE_FIRMWARE=target/xtensa-esp32s3-none-elf/release/nockster-fw.bin
```

Strict/production preflight also requires `NOCKSTER_AUTO_MIGRATE_NVS_V2=0`.
Automatic NVS migration is for sacrificial chip-security test builds only.
It verifies the signed update bundle against the configured trust anchor and
reports bundle-verification failures without running any provisioning action.
The checked partition table must keep the custom `nvs` partition unflagged for
partition-level encryption unless raw NVS read/write testing has passed on this
board and `NVS_PARTITION_ENCRYPTION_VALIDATED=1` is set.

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
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2.pem

make release-sign-secure-boot-v2 \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2.pem \
  SECURE_BOOT_IMAGE=/path/to/unsigned-app-image.bin \
  SECURE_BOOT_SIGNED_IMAGE=/path/to/signed-app-image.bin
```

`generate-secure-boot-v2-key` only creates a local secure-boot signing key,
refuses to write inside the repo, refuses to overwrite an existing file, and
sets restrictive permissions. It does not touch eFuses.
`release-sign-secure-boot-v2` refuses in-place signing, refuses to overwrite an
existing signed output, and first checks that the input looks like an ESP app
image that fits the configured app slot.

Secure boot digest provisioning is irreversible and intentionally separate from
release signing:

```sh
make provision-summary PROVISION_PORT=/dev/ttyACM0
make provision-secure-boot-v2-digest \
  PROVISION_PORT=/dev/ttyACM0 \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2.pem \
  CONFIRM_IRREVERSIBLE=burn-secure-boot-v2
```

`provision-secure-boot-v2-digest` prints the current eFuse summary and asks for
an additional interactive confirmation before it calls
`espefuse burn-key-digest`.

Flash encryption provisioning guards:

```sh
make generate-flash-encryption-key \
  FLASH_ENCRYPTION_KEY_FILE=../nockster-secrets/flash-encryption-key.bin

make provision-summary PROVISION_PORT=/dev/ttyACM0

make provision-flash-encryption-key \
  PROVISION_PORT=/dev/ttyACM0 \
  FLASH_ENCRYPTION_KEY_FILE=../nockster-secrets/flash-encryption-key.bin \
  FLASH_ENCRYPTION_KEY_BLOCK=BLOCK_KEY4 \
  CONFIRM_IRREVERSIBLE=burn-flash-encryption-key

make provision-flash-encryption-enable \
  PROVISION_PORT=/dev/ttyACM0 \
  CONFIRM_IRREVERSIBLE=enable-flash-encryption
```

`generate-flash-encryption-key` only creates a local 32-byte key file, refuses
to write inside the repo, refuses to overwrite an existing file, and sets
restrictive permissions. It does not touch eFuses.

`provision-flash-encryption-key` burns the key with purpose `XTS_AES_128_KEY`
and prints the current eFuse summary first. `provision-flash-encryption-enable`
burns `SPI_BOOT_CRYPT_CNT` separately. Keep these separate until a
sacrificial-board run has proven the signed/encrypted image and recovery flow.

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
