# Nockster ESP32-S3 security

Nockster is a hardware signer built on a Waveshare ESP32-S3-Touch-LCD-1.47 class
board. This document describes how the device protects seeds and signing keys,
which mechanisms are active today, and the reasoning behind each choice. The
security-relevant behavior comes from the ESP32-S3 chipset.

## What the device defends against

The seed never leaves the device in plaintext. The protections are layered so
that compromising one does not immediately expose the seed:

- A **lost or stolen device** is gated by the PIN, with on-device entry and
  rate limiting.
- A **flash dump** (desoldering or reading the SPI flash) yields only
  AES-256-GCM ciphertext. The remaining work is to defeat the key derivation,
  not the cipher.
- A **malicious firmware swap** (a thief reflashing a modified signer) is the
  job of secure boot and the signed-update trust boundary.
- **Physical chip attacks** (JTAG, download mode, glitching) are addressed by
  the production lockdown fuses.

Transaction signing crypto is independent of all of this — these mechanisms
protect storage, boot, and provisioning, not the signature scheme.

## How seed storage works

Seeds are kept in a custom NVS (non-volatile storage) region at flash offset
`0x9000`, separate from the system NVS. Each seed slot is stored as its own
record: a random per-record salt and GCM nonce in the clear, followed by the
seed encrypted with AES-256-GCM. The PIN is never stored — it is only ever an
input to the key derivation below — and the authentication tag means a tampered
record fails to decrypt rather than returning garbage.

When the device unlocks, it derives the storage key, decrypts the slots into a
single in-RAM session object, and holds the cleartext seeds and the derived key
only there. That session state is zeroized whenever a seed is replaced, deleted,
or the device is locked or reset, so cleartext key material does not linger.

All random material — salts, GCM nonces, and on-device seed generation — comes
from the ESP32-S3 hardware TRNG through esp-hal's RNG driver, with the entropy
source kept enabled for the life of the app so randomness is ready before the
first salt or nonce is needed.

### Why the key derivation matters

A low-entropy PIN is the weakpoint. The salt and the encrypted
seed both sit in flash, so an attacker who dumps flash can attack
`PBKDF2(PIN, salt)` offline, and that is only as strong as the PIN plus the cost
of the KDF.

The ESP32-S3 has a a read-protected eFuse key block with
purpose `HMAC_UP`. Firmware can ask the HMAC peripheral for
`HMAC-SHA256(efuse_key, message)` without the key ever being readable by
software. The storage key mixes that output into the derivation, so it can only
be reproduced on that specific chip, turning an offline flash attack back into
an on-device one. With fixed domain-separation labels, the derivation is:

```text
pin_key = PBKDF2-HMAC-SHA256(pin, salt, rounds)
pepper = HMAC-SHA256(efuse_key, domain || salt || mac)
key = SHA256(domain || pin_key || pepper)
```

The pepper source depends on the build:

- A `chip-security` firmware build owns the ESP32-S3 HMAC peripheral on the app
  core, detects a provisioned `HMAC_UP` slot by its eFuse purpose, and routes
  unlock, first initialization, and PIN-change storage through the pepper-aware
  path. If an `HMAC_UP` slot is present but the peripheral calculation fails, the
  storage operation fails closed with a crypto error rather than falling back.
- Default dev/test builds use a fixed software pepper, so they need no eFuse
  provisioning and burn nothing. The storage format is identical either way; only
  the pepper source differs, which is why the hardware-bound guarantee is
  something production validation has to assert separately.

## Boot integrity and signed updates

Secure boot v2 ensures only signed images run, which is what stops a thief from
replacing the firmware with a modified signer. It does not replace PIN/NVS
protection.

The same release trust boundary covers self-updates. The host may transport an
update bundle, but the firmware verifies the manifest signature on-device
against a pinned release public-key-hash trust anchor before accepting it, then
hashes the streamed firmware image on-device before OTA activation. The OTA
writer writes the signed stream into the inactive slot and marks the running
image valid after boot when an OTA data partition is present. Building with
`NOCKSTER_RELEASE_VERSION=<n>` gives a rollback floor — the on-device verifier
rejects any bundle whose signed release counter is not greater than the current
firmware — without burning eFuses, so dev and test release flows get rollback
protection too.

`esp-bootloader-esp-idf` provides the ESP-IDF app descriptor and partition
helpers, but secure-boot signing and provisioning stay outside ordinary
flashing. The signing and provisioning tooling exists today:

- `make generate-secure-boot-v2-key` creates a local signing key with repo-local
  path rejection, overwrite protection, and restrictive permissions; it touches
  no eFuses.
- `make release-sign-secure-boot-v2` signs an already-built app image with
  Espressif's tooling and verifies the resulting signature block, after checking
  the input looks like an ESP app image, fits the app slot, and is not signed in
  place.
- `make provision-secure-boot-v2-digest` burns the digest behind an explicit
  `CONFIRM_IRREVERSIBLE=burn-secure-boot-v2` token and an interactive prompt.


## Flash encryption

Flash encryption is for release devices; application-level NVS AES-GCM stays in
place even when it is enabled, so storage is protected at two layers.

The key-handling and provisioning tooling:

- `make generate-flash-encryption-key` creates a local 32-byte XTS key with the
  same path/permission guards as the other key generators; it touches no eFuses.
- `make provision-flash-encryption-key` burns the key behind
  `CONFIRM_IRREVERSIBLE=burn-flash-encryption-key`.
- `make provision-flash-encryption-enable` enables it (burns
  `SPI_BOOT_CRYPT_CNT`) behind a separate `CONFIRM_IRREVERSIBLE=enable-flash-encryption`.
- `make release-preflight` checks the flash-encryption key path/permissions when
  `FLASH_ENCRYPTION_KEY_FILE` is provided (required for strict production
  preflight) and checks the flashing partition table: `nvs` stays unflagged for
  partition-level encryption unless `NVS_PARTITION_ENCRYPTION_VALIDATED=1` is set
  after board-specific raw NVS read/write testing, factory stays at `0x10000`,
  and the app image limit fits the OTA slot.


## Production lockdown

The final hardening disables the physical debug/recovery surface: USB JTAG, pad
JTAG, download mode, ROM USB serial/JTAG printing, and (optionally) power-glitch
protection. These are one-way eFuse writes that can make a board hard or
impossible to recover through the normal USB-C workflow, so they are intended
only after secure boot, flash encryption, a recovery procedure, and at least one
sacrificial-board test are in place.

Each fuse is its own guarded target — `provision-lockdown-jtag`,
`provision-lockdown-download`, `provision-lockdown-direct-boot`,
`provision-lockdown-rom-print` — and refuses to run without a matching
`CONFIRM_IRREVERSIBLE=...` token, prints the current eFuse summary, and asks for a
typed confirmation before calling `espefuse burn-efuse`. There is intentionally
no one-shot lockdown target. Power-glitch protection is kept separate
(`provision-power-glitch-protection`, checked with
`--expect-power-glitch-protection`) because it still needs board-specific
false-positive testing before it belongs in the default checklist.

## Inspecting and asserting security state

The CLI reports NVS storage state in every build, and chip-security state when
that feature is compiled in:

```sh
nockster-cli security --port hid
```

Default builds do not read the eFuse security registers, so the CLI shows
`chip: hidden (firmware built without chip-security)` until firmware is built
with `FW_PROFILE=chip-security` (`FW_PROFILE=chip-security make fw` / `make
flash`). With chip-security enabled, the report adds secure boot, flash
encryption, key purposes, JTAG/download-mode fuses, and power-glitch protection.
The browser device panel reads the same status over WebHID/Web Serial when the
firmware advertises the security feature.

The command can also assert expected provisioning state and exit nonzero on
mismatch, which is how provisioned hardware is verified without trusting visual
inspection:

```sh
nockster-cli security --port hid \
  --expect-chip-security \
  --expect-hmac-up \
  --expect-hmac-up-read-protected
```

## Provisioning model and development policy

eFuse writes are dangerous and irreversible, so the build is structured to make
them impossible to trigger by accident:

- **Never burn eFuses from a normal build/test target.** `make flash` and
  `make fw` are for `dev` or `chip-security` test builds. `FW_PROFILE=production`
  refuses unsigned firmware unless `ALLOW_UNSIGNED_PRODUCTION=1` is set for
  release-flow dry runs.
- **Provisioning is always separate, explicit, and confirmed.** Each
  provisioning target prints the current eFuse summary first and requires a
  matching `CONFIRM_IRREVERSIBLE=...` token plus an interactive confirmation.
  Key generators (`generate-hmac-up-key`, `generate-secure-boot-v2-key`,
  `generate-flash-encryption-key`) only write a local secret file, reject
  repo-local output paths, and touch no eFuses. Set `ESPEFUSE=/path/to/espefuse`
  if the Espressif tool is not on `PATH`. Never use `--no-read-protect` for
  production, and keep generated key files out of the repo.
- **Review before touching hardware.** `make provision-plan
  PROVISION_STAGE=production` prints the ordered irreversible checklist without
  invoking flash, signing, CLI, or eFuse tools.
- **Preflight is non-destructive by default.** `make release-preflight` checks
  release metadata, update trust-anchor format, signing-key/trust-hash
  consistency, local secret-file hygiene, signed-update bundle verification when
  artifacts are provided, browser latest-release index consistency when
  `UPDATE_INDEX` is provided, and tracked secret-looking paths — without reading
  eFuses. Set `RUN_EFUSE_SUMMARY=1 PROVISION_PORT=/dev/ttyACM0` to include the
  optional read-only chip status. Missing `espsecure`/`espefuse` tools are
  warnings in the default pass and failures in strict/production preflight.
- **Validate provisioned hardware read-only.** `make validate-device-state
  VALIDATE_STAGE=production` runs the CLI expectation checks for HMAC_UP /
  read-protection, storage initialization, secure boot, flash encryption,
  production lockdown, and OTA layout readiness (`VALIDATE_DRY_RUN=1` prints the
  commands first).

### Provisioning reference

The HMAC_UP pepper key:

```sh
make generate-hmac-up-key HMAC_KEY_FILE=../nockster-secrets/hmac-up.bin
make provision-summary PROVISION_PORT=/dev/ttyACM0
make provision-hmac-up PROVISION_PORT=/dev/ttyACM0 HMAC_KEY_FILE=../nockster-secrets/hmac-up.bin CONFIRM_IRREVERSIBLE=burn-hmac-up
nockster-cli security --expect-chip-security --expect-hmac-up --expect-hmac-up-read-protected
```

Flash encryption:

```sh
make generate-flash-encryption-key FLASH_ENCRYPTION_KEY_FILE=../nockster-secrets/flash-encryption-key.bin
make provision-summary PROVISION_PORT=/dev/ttyACM0
make provision-flash-encryption-key PROVISION_PORT=/dev/ttyACM0 FLASH_ENCRYPTION_KEY_FILE=../nockster-secrets/flash-encryption-key.bin FLASH_ENCRYPTION_KEY_BLOCK=BLOCK_KEY4 CONFIRM_IRREVERSIBLE=burn-flash-encryption-key
make provision-flash-encryption-enable PROVISION_PORT=/dev/ttyACM0 CONFIRM_IRREVERSIBLE=enable-flash-encryption
nockster-cli security --port hid --expect-flash-encryption
```

Production lockdown is then enforced with:

```sh
nockster-cli security --port hid --expect-production-lockdown
```
