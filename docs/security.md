# Nockster ESP32-S3 security plan

Sources checked in this repo:

- `Esp32-s3_datasheet_en.pdf`
- `Esp32-s3_technical_reference_manual_en.pdf`
- `1.47inch_LCD_Datasheet.pdf`

The target board is the Waveshare ESP32-S3-Touch-LCD-1.47 class board. The
LCD PDF is useful for display/touch timing, but the security-relevant features
come from the ESP32-S3 itself.

## Current firmware behavior

- Seed slots are stored in the custom NVS region at `0x9000`.
- Slot payloads are AES-256-GCM encrypted with a key derived from the PIN and a
  per-device salt.
- The firmware now keeps the ESP32-S3 TRNG entropy source enabled for the life
  of the app before salts and nonces are generated.
- In-RAM seed slots and the decrypted NVS master key are kept in one session
  state object and zeroized when replaced, deleted, locked, or reset.
- Default firmware builds do not read eFuse security registers. Build with
  `FW_PROFILE=chip-security make fw` or
  `FW_PROFILE=chip-security make flash` when testing the real chip-security
  path.
- The CLI can report NVS state, and chip security state when that feature is
  enabled:

```sh
nockster-cli security --port hid
```

That command reports NVS schema state in all builds. With `chip-security`
enabled it also reports secure boot, flash encryption, key purposes,
JTAG/download-mode fuses, and power-glitch protection.
The browser device panel reads the same security status when firmware advertises
the security feature, so NVS schema and chip-security state can be checked from
WebHID/Web Serial sessions too.

It can also assert expected provisioning state and exit nonzero on mismatch:

```sh
nockster-cli security --port hid \
  --expect-chip-security \
  --expect-hmac-up \
  --expect-hmac-up-read-protected \
  --expect-nvs-v2
```

## Practical risk model

The biggest current storage weakness is not AES-GCM; it is that a low-entropy
PIN can be attacked offline if an attacker dumps flash. The salt and encrypted
seed are both in flash, so `PBKDF2(PIN, salt)` is only as strong as the PIN plus
the cost of the KDF.

The ESP32-S3 has a better primitive for this: a read-protected eFuse key block
with purpose `HMAC_UP`. Firmware can ask the HMAC peripheral for
`HMAC-SHA256(efuse_key, device_message)` without the key ever being readable by
software. Mixing that result into the NVS key derivation turns a flash dump into
an on-device attack.

## Security phases

### Phase 0: visibility and entropy

Implemented:

- Keep the TRNG entropy source enabled at boot.
- Add `Request::GetSecurityStatus`.
- Add `nockster-cli security`.
- Gate real eFuse reads behind the firmware `chip-security` feature.

This phase is safe on every development board because it does not write eFuses.
Default builds also avoid reading the eFuse security registers; the CLI will
show `chip: hidden (firmware built without chip-security)` until the firmware
is built with `FW_PROFILE=chip-security`.

### Phase 1: NVS v2 with eFuse HMAC pepper

Goal:

- Burn one 256-bit key into an unused key block, with key purpose `HMAC_UP`.
- Read-protect and write-protect that key block.
- Derive NVS keys from both the PIN KDF and the HMAC peripheral output:

```text
pin_key = PBKDF2-HMAC-SHA256(pin, salt, rounds)
pepper  = HMAC-SHA256(efuse_key, "nockster-nvs-v2" || salt || mac)
key     = SHA256("nockster-nvs-master-v2" || pin_key || pepper)
```

This does not change transaction signing crypto. It only changes storage
encryption.

Implemented foundation:

- NVS headers still parse schema v1 and schema v2, but new and rewritten
  storage now uses schema v2 by default.
- Default dev/test firmware uses a fixed software pepper for schema v2, so test
  builds do not need eFuse provisioning and do not burn eFuses.
- The storage layer uses a hardware HMAC_UP pepper when a chip-security build
  can provide one; otherwise it falls back to the software pepper. Production
  validation must separately assert HMAC_UP and read-protection.
- The v2 pepper message helper encodes
  `"nockster-nvs-v2" || salt || mac`, and the v2 master key uses the documented
  domain-separated SHA-256 combiner.
- `chip-security` firmware now owns the ESP32-S3 HMAC peripheral on the app
  core, detects a provisioned `HMAC_UP` key slot by eFuse purpose, and routes
  unlock, first initialization, and PIN-change NVS work through the pepper-aware
  storage APIs.
- If an `HMAC_UP` slot is present but the peripheral calculation fails, the NVS
  operation fails closed with a crypto error.
- Explicit schema migration flows are not exposed while the only hardware in
  circulation is sacrificial/test hardware. Wipe and reseed instead of carrying
  compatibility migration UX.
- After initializing storage or changing the PIN,
  `nockster-cli security --expect-nvs-v2` should pass. That check does not read
  or write any key material.

Provisioning sketch:

```sh
make generate-hmac-up-key HMAC_KEY_FILE=../nockster-secrets/hmac-up.bin
make provision-summary PROVISION_PORT=/dev/ttyACM0
make provision-hmac-up PROVISION_PORT=/dev/ttyACM0 HMAC_KEY_FILE=../nockster-secrets/hmac-up.bin CONFIRM_IRREVERSIBLE=burn-hmac-up
nockster-cli security --expect-chip-security --expect-hmac-up --expect-hmac-up-read-protected
nockster-cli security --expect-nvs-v2
```

Do not use `--no-read-protect` for production. Keep generated key files out of
the repo. `generate-hmac-up-key` only creates a local secret file, refuses
repo-local output paths, and does not touch eFuses. The guarded
`provision-hmac-up` target prints the current eFuse summary and asks for an
additional interactive confirmation before invoking `espefuse burn-key`. Set
`ESPEFUSE=/path/to/espefuse` if the Espressif eFuse tool is not on `PATH`.

### Phase 2: secure boot v2

Goal:

- Sign release firmware.
- Burn the secure boot digest/key material.
- Verify on boot that only signed images run.
- Use the same release trust boundary for self-update bundles: the host may
  transport a bundle, but firmware verifies the manifest signature on-device
  against a pinned release public-key hash before accepting it, then hashes the
  streamed firmware image on-device before OTA activation. The first OTA writer
  now writes signed streams into the inactive slot and marks the running OTA
  image valid after boot when an OTA data partition is present.
- Build firmware with `NOCKSTER_RELEASE_VERSION=<n>` so the on-device manifest
  verifier rejects bundles whose signed release counter is not greater than the
  current firmware. This gives development and test release flows a rollback
  floor without burning eFuses.

This protects against a thief replacing the firmware with a modified signer.
It does not replace PIN/NVS protection.

Implemented foundation:

- `esp-bootloader-esp-idf` gives us the ESP-IDF app descriptor and partition
  helpers, but secure boot signing/provisioning stays outside ordinary
  flashing.
- `make generate-secure-boot-v2-key` creates a local secure-boot signing key
  file with repo-local path rejection, overwrite protection, and restrictive
  permissions; it does not touch eFuses.
- `make release-sign-secure-boot-v2` signs an already-built app image with
  Espressif secure boot v2 tooling and verifies the resulting signature block.
  Before signing, it checks that the input file looks like an ESP app image,
  fits the configured app slot, and is not being signed in place.
- `make provision-secure-boot-v2-digest` is an explicit irreversible eFuse
  target that prints the current eFuse summary and requires
  `CONFIRM_IRREVERSIBLE=burn-secure-boot-v2` plus an interactive confirmation
  before invoking `espefuse burn-key-digest`.

Still needs sacrificial-board validation before production use:

- Confirm the exact bootloader/app image generation flow on this board, using
  the preflighted unsigned app image passed into `release-sign-secure-boot-v2`.
- Confirm the bootloader rejects unsigned or incorrectly signed images on this
  board before any broader provisioning.

### Phase 3: flash encryption

Goal:

- Enable flash encryption for release devices.
- Keep application-level NVS AES-GCM even when flash encryption is enabled.

Implemented foundation:

- `make generate-flash-encryption-key` creates a local 32-byte XTS flash
  encryption key file with repo-local path rejection, overwrite protection, and
  restrictive permissions; it does not touch eFuses.
- `make provision-flash-encryption-key` is an explicit irreversible eFuse
  target that prints the current eFuse summary and requires
  `CONFIRM_IRREVERSIBLE=burn-flash-encryption-key` plus an interactive
  confirmation before invoking `espefuse burn-key ... XTS_AES_128_KEY`.
- `make provision-flash-encryption-enable` is a separate irreversible eFuse
  target that prints the current eFuse summary and requires
  `CONFIRM_IRREVERSIBLE=enable-flash-encryption` plus an interactive
  confirmation before burning `SPI_BOOT_CRYPT_CNT`.
- `make release-preflight` checks the flash encryption key path and
  permissions when `FLASH_ENCRYPTION_KEY_FILE` is provided, and requires it for
  strict production preflight.
- `make release-preflight` also checks the partition table used for flashing:
  `nvs` must remain unflagged for partition-level encryption unless
  `NVS_PARTITION_ENCRYPTION_VALIDATED=1` is set after board-specific raw NVS
  read/write testing, factory must stay at `0x10000`, and the configured app
  image limit must fit the OTA slot size.

Provisioning sketch:

```sh
make generate-flash-encryption-key FLASH_ENCRYPTION_KEY_FILE=../nockster-secrets/flash-encryption-key.bin
make provision-summary PROVISION_PORT=/dev/ttyACM0
make provision-flash-encryption-key PROVISION_PORT=/dev/ttyACM0 FLASH_ENCRYPTION_KEY_FILE=../nockster-secrets/flash-encryption-key.bin FLASH_ENCRYPTION_KEY_BLOCK=BLOCK_KEY4 CONFIRM_IRREVERSIBLE=burn-flash-encryption-key
make provision-flash-encryption-enable PROVISION_PORT=/dev/ttyACM0 CONFIRM_IRREVERSIBLE=enable-flash-encryption
nockster-cli security --port hid --expect-flash-encryption
```

Before marking the custom NVS partition encrypted, test raw reads/writes through
`esp-storage` on this exact partition layout, then run release preflight with
`NVS_PARTITION_ENCRYPTION_VALIDATED=1`. App flash encryption is valuable even
if NVS remains protected at the application layer.

### Phase 4: production lockdown

Only after secure boot, flash encryption, recovery procedure, and at least one
sacrificial-board test:

- Disable USB JTAG.
- Disable pad JTAG.
- Disable or restrict download mode.
- Disable ROM USB serial/JTAG printing.
- Consider enabling power-glitch protection after testing false positives.

These are one-way eFuse writes and can make a board hard or impossible to
recover through the normal USB-C workflow.

After those production-only fuses are intentionally set, the CLI can enforce
the current lockdown checklist without trusting visual inspection:

```sh
nockster-cli security --port hid --expect-production-lockdown
```

Implemented foundation:

- Production lockdown eFuse writes are available only as separate guarded
  provisioning targets:
  `provision-lockdown-jtag`, `provision-lockdown-download`,
  `provision-lockdown-direct-boot`, and `provision-lockdown-rom-print`.
- Each target refuses to run without a matching `CONFIRM_IRREVERSIBLE=...`
  token, prints the current eFuse summary, and asks for an additional typed
  confirmation before invoking `espefuse burn-efuse`.
- There is intentionally no one-shot lockdown target.

Power-glitch protection is checked separately with
`--expect-power-glitch-protection` because it still needs board-specific false
positive testing before it belongs in the default production checklist.
The `provision-power-glitch-protection` target is available behind its own
confirmation token for that later validation.

## Development policy

- Never burn eFuses from a normal build/test target.
- Provisioning commands must be separate, explicit, and show current eFuse
  status first.
- `make release-preflight` is non-destructive by default: it checks release
  metadata, update trust-anchor format, release signing key/trust-hash
  consistency, local secret-file hygiene for provided key paths, signed update
  bundle verification when artifacts are provided, browser latest-release index
  consistency when `UPDATE_INDEX` is provided, and tracked secret-looking paths
  without reading eFuses. Set `RUN_EFUSE_SUMMARY=1
  PROVISION_PORT=/dev/ttyACM0` when you want the optional read-only chip status
  included. Missing `espsecure`/`espefuse` tools are warnings in the default
  pass and failures in strict/production preflight.
- `make provision-plan PROVISION_STAGE=production` prints the ordered
  production provisioning checklist without invoking flash, signing, CLI, or
  eFuse tools. Use it to review the planned irreversible steps before touching
  hardware.
- `make validate-device-state VALIDATE_STAGE=production` runs the read-only
  CLI expectation checks for provisioned hardware: HMAC_UP/read-protection,
  NVS v2, secure boot, flash encryption, production lockdown, and OTA layout
  readiness. Use `VALIDATE_DRY_RUN=1` to print the commands first.
- NVS schema migration switches are not part of the release flow. During early
  hardware testing, wipe and reseed devices instead of preserving old storage.
- `make flash` and `make fw` are for `dev` or `chip-security` test builds.
  `FW_PROFILE=production` refuses unsigned firmware unless
  `ALLOW_UNSIGNED_PRODUCTION=1` is set for release-flow dry runs.
- Keep at least one unprovisioned development board.
- Treat signing keys and HMAC key files as secrets; do not commit them.
