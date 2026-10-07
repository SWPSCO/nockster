# Production Provisioning Runbook

This is the ordered, end-to-end procedure for turning a bare ESP32-S3 board
into a locked-down production Nockster. It is the operator-facing companion to
the per-command reference in [`scripts/provision/README.md`](../scripts/provision/README.md):
the README documents what each target does; this document is the sequence,
the decision points, and the irreversible-step gating.

> **Read this first.** Several steps below burn eFuses. eFuse writes are
> **permanent** — they cannot be undone, and a wrong order or a bad key can
> brick a board or lock you out of recovery. Do not run any `provision-*`
> target on a board you cannot afford to destroy until the entire flow has
> been rehearsed on a **sacrificial board**. Every irreversible target already
> refuses to run without an explicit `CONFIRM_IRREVERSIBLE=…` token and prints
> the current eFuse state first; those guards are a backstop, not a substitute
> for this runbook.

## Threat model and what provisioning buys

Provisioning hardens the device against an attacker with **physical access to
the board** — chip decapping, flash desoldering, JTAG, serial download mode.
It is complementary to, not a replacement for, the on-device PIN/seed
protections (encrypted NVS, PIN attempt counter, lockout):

- **Flash encryption** makes a dumped flash image (including the encrypted NVS
  seed region) ciphertext under a key that never leaves the chip.
- **Secure Boot v2** makes the chip refuse to run firmware not signed by your
  release key — no malicious-firmware swap, no unsigned recovery image.
- **JTAG / download-mode / direct-boot / ROM-print lockdown** removes the
  debug and bootloader avenues an attacker would use to read RAM or re-flash.
- **HMAC-UP eFuse** provides the hardware pepper for NVS schema v2 (see
  `FW_PROFILE=chip-security`), binding the encrypted seed store to *this* chip.

It does **not** defend against an attacker who already has the device unlocked,
or who coerces the PIN from the user (see the duress-PIN proposal). It does not
make a lost-but-locked device's seed extractable by you either — that is the
point. Keep external seed/coil backups (BIP39 phrase, `zprv`, or the new
Shamir shares).

## Prerequisites

- Espressif tools on `PATH` (or set `ESPSECURE`, `ESPEFUSE`): `espflash`,
  `espsecure`, `espefuse`.
- ESP-IDF available through `idf.py` or `IDF_PATH`. The production secure-boot
  path builds a secure-boot-enabled second-stage bootloader with ESP-IDF.
- A **secrets directory outside the repo** (e.g. `../nockster-secrets/`). The
  generator targets refuse to write inside the repo and refuse to overwrite.
- The release signing key for OTA updates already established, and its public
  hash compiled into firmware via `NOCKSTER_UPDATE_PUBKEY_SHA256_HEX`
  (the firmware trust anchor; see [`secure-updates.md`](./secure-updates.md)).
- `PROVISION_PORT` — the board's USB-serial/CDC port in bootloader mode
  (e.g. `/dev/ttyACM0`). Note that a board running normally enumerates as HID;
  put it in bootloader/download mode for `espefuse`/`espflash` provisioning.

Generate all secrets up front and back them up offline:

```sh
make generate-hmac-up-key            HMAC_KEY_FILE=../nockster-secrets/hmac-up.bin
make generate-secure-boot-v2-key     SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2-rsa.pem
make generate-flash-encryption-key   FLASH_ENCRYPTION_KEY_FILE=../nockster-secrets/flash-encryption-key.bin
```

Loss of the secure-boot key means you can never ship another signed update to
provisioned boards. Loss of the flash-encryption key is fine (it is meant to
stay on-chip) **unless** you intend host-side pre-encryption; keep it anyway.

## Firmware profiles

`FW_PROFILE` selects the build:

| Profile         | Features            | Use                                              |
|-----------------|---------------------|--------------------------------------------------|
| `dev` (default) | none                | bench development; NVS schema v1; no chip-security reads |
| `chip-security` | `chip-security`     | reads HMAC-UP eFuse; NVS schema v2; test builds with hardware pepper |
| `production`    | `chip-security`     | release; build gated behind `ALLOW_UNSIGNED_PRODUCTION=1` for dry runs and otherwise driven by the secure-boot signing flow, not `make flash` |

`make fw-production` deliberately refuses an ordinary build: production images
must go through secure-boot signing (below), not `make flash`.

## The sequence

Run the **dry-run plan** first; it prints the ordered commands for your stage
and touches nothing:

```sh
make provision-plan PROVISION_STAGE=production PROVISION_PORT=/dev/ttyACM0
```

For a fresh board, there is also a one-confirmation command that executes the
complete production flow end to end:

```sh
make flash-prod-e2e
```

The target defaults to `/dev/ttyACM0`, `../nockster-secrets/`, release version
`1` when `NOCKSTER_RELEASE_VERSION=0`, secure-boot digest slot `BLOCK_KEY0`,
and flash-encryption slot `BLOCK_KEY4`. It generates missing key files outside
the repo, builds/signs/encrypts fresh artifacts under `target/prod-e2e/<time>/`,
prints the current eFuse summary, then asks once for `FLASH-PROD-E2E` before
any burn. It refuses to continue if the selected key slots, `SECURE_BOOT_EN`,
or `SPI_BOOT_CRYPT_CNT` are already set. It also prebuilds the next release as
a signed OTA before the confirmation. After enabling flash encryption, it
pauses for a manual normal reboot before HID validation; power-cycle the board
or press EN/RESET, do not hold BOOT/download, wait for HID to re-enumerate,
then press Enter. It installs that OTA, reboots automatically, and succeeds
only after all production lockdown and glitch-protection checks pass. Override
the defaults as needed:

```sh
make flash-prod-e2e \
  PROVISION_PORT=/dev/ttyACM0 \
  NOCKSTER_SECRET_DIR=../nockster-secrets \
  NOCKSTER_RELEASE_VERSION=1 \
  NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=<sha256-of-release-pubkey>
```

Preview the exact command order without touching the board:

```sh
make flash-prod-e2e PROD_E2E_DRY_RUN=1
```

`flash-prod-e2e` includes final lockdown and power-glitch protection. Those
eFuses are irreversible; after the signed OTA transition, serial download and
JTAG recovery are intentionally unavailable.

Then proceed stage by stage. After each irreversible burn, re-run
`make provision-summary PROVISION_PORT=…` and the matching
`make validate-device-state VALIDATE_STAGE=… VALIDATE_PORT=hid` check before
continuing.

### 0. Sacrificial-board rehearsal (mandatory once)

Run the **entire** sequence below — through lockdown and power-glitch — on a
board you will throw away. Confirm: the device boots signed firmware, NVS
initializes and unlocks under flash encryption, an OTA update still applies,
and the recovery story works. Only then touch real units.

### 1. HMAC-UP pepper (NVS schema v2)

```sh
make provision-summary   PROVISION_PORT=/dev/ttyACM0
make provision-hmac-up   PROVISION_PORT=/dev/ttyACM0 \
  HMAC_KEY_FILE=../nockster-secrets/hmac-up.bin \
  CONFIRM_IRREVERSIBLE=burn-hmac-up
```

Then flash a `chip-security` image with the ordinary serial flash path and let
the board boot normally. If the board is currently in HID mode, put it into
serial bootloader/download mode first; `make flash` cannot flash HID directly.
After the app boots and initializes or rewrites storage, verify NVS v2:

```sh
make flash FLASH_PORT=/dev/ttyACM0 FW_PROFILE=chip-security
```

```sh
nockster-cli security --port hid \
  --expect-chip-security --expect-hmac-up \
  --expect-hmac-up-read-protected --expect-nvs-v2
```

### 2. Secure Boot v2

ESP32-S3 secure boot v2 uses an RSA3072 key. The production path has three
artifacts that must match the same key: the signed app image, the signed
secure-boot bootloader, and the public-key digest burned into eFuse.

```sh
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

Put the board in serial bootloader/download mode, then flash the signed
bootloader, partition table, and signed app. This target uses `--after
no-reset` so the secure-boot bootloader does not run before the digest and
enable bit are reviewed:

```sh
make flash-secure-boot-v2 \
  FLASH_PORT=/dev/ttyACM0 \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2-rsa.pem \
  SECURE_BOOT_SIGNED_IMAGE=target/secure-boot-v2/nockster-fw.factory.signed.bin
```

Burn the **public-key digest** first, then burn `SECURE_BOOT_EN`, then reset:

```sh
make provision-summary PROVISION_PORT=/dev/ttyACM0
make provision-secure-boot-v2-digest \
  PROVISION_PORT=/dev/ttyACM0 \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2-rsa.pem \
  CONFIRM_IRREVERSIBLE=burn-secure-boot-v2

make provision-secure-boot-v2-enable \
  PROVISION_PORT=/dev/ttyACM0 \
  CONFIRM_IRREVERSIBLE=enable-secure-boot-v2

espflash reset --port /dev/ttyACM0
nockster-cli security --port hid --expect-chip-security --expect-secure-boot
```

Do not use ordinary `make flash` after this point; it writes the normal
bootloader and unsigned app path. For this production path, rebuild/sign and
use `flash-secure-boot-v2`.

If a sacrificial board already has `BLOCK_KEY0` set to `SECURE_BOOT_DIGEST0`
from an older non-RSA key but `SECURE_BOOT_EN` is still false, leave slot 0
alone and burn a fresh RSA3072 digest into an empty slot:

```sh
make provision-secure-boot-v2-digest \
  PROVISION_PORT=/dev/ttyACM0 \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2-rsa.pem \
  SECURE_BOOT_DIGEST_BLOCK=BLOCK_KEY1 \
  SECURE_BOOT_DIGEST_PURPOSE=SECURE_BOOT_DIGEST1 \
  CONFIRM_IRREVERSIBLE=burn-secure-boot-v2
```

### 3. Flash encryption

Flash encryption must be enabled from encrypted flash contents. Do **not** burn
`SPI_BOOT_CRYPT_CNT` over plaintext flash. First generate the key, rebuild the
signed secure-boot bootloader with flash-encryption support, and host-encrypt
the signed bootloader, partition table, signed app, and blank `otadata` at
their exact flash offsets:

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
```

Put the board in serial bootloader/download mode. Burn the XTS key, flash only
the encrypted artifacts with no reset, then burn `SPI_BOOT_CRYPT_CNT` and reset:

```sh
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

espflash reset --port /dev/ttyACM0
nockster-cli security --port hid \
  --expect-chip-security --expect-secure-boot --expect-flash-encryption
```

> **NVS partition flag.** Keep the custom `nvs` partition **unflagged** for
> partition-level encryption in `partitions.csv` unless raw NVS read/write
> testing has passed on this board and you set
> `NVS_PARTITION_ENCRYPTION_VALIDATED=1` for preflight. The firmware encrypts
> the seed region itself; double-encryption or a mismatched flag corrupts it.

### 4. Strict release preflight

Before publishing the image these units will pull as their first OTA, run the
strict preflight with all secret paths and public artifacts:

```sh
make signed-update-secure-boot-v2 \
  FW_PROFILE=production ALLOW_UNSIGNED_PRODUCTION=1 \
  NOCKSTER_RELEASE_VERSION=<n> \
  NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=<sha256-of-release-pubkey> \
  UPDATE_SIGNING_KEY_FILE=../nockster-secrets/release-signing-key.hex \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2-rsa.pem
```

```sh
make release-preflight \
  FW_PROFILE=production RELEASE_PREFLIGHT_STRICT=1 \
  NOCKSTER_RELEASE_VERSION=<n> \
  NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=<sha256-of-release-pubkey> \
  HMAC_KEY_FILE=../nockster-secrets/hmac-up.bin \
  UPDATE_SIGNING_KEY_FILE=../nockster-secrets/release-signing-key.hex \
  SECURE_BOOT_KEY_FILE=../nockster-secrets/secure-boot-v2-rsa.pem \
  FLASH_ENCRYPTION_KEY_FILE=../nockster-secrets/flash-encryption-key.bin \
  UPDATE_BUNDLE=target/update/nockster-fw.update.json \
  UPDATE_FIRMWARE=target/update/nockster-fw.bin
```

It verifies the signed bundle against the trust anchor, checks partition
layout, secret-file hygiene (and that no secret path slipped into git), and —
if `UPDATE_INDEX` is given — that the published `latest.json` matches the
bundle and firmware. Hosted release indexes should be mutable, but the bundle
and firmware URLs named by the index should be versioned.

### 5. Production lockdown

Only after secure boot, flash encryption, OTA recovery, and the sacrificial
run all pass. Each is a **separate** target with its own confirmation token;
there is intentionally no one-shot lockdown:

```sh
make provision-lockdown-jtag         PROVISION_PORT=/dev/ttyACM0 CONFIRM_IRREVERSIBLE=disable-jtag
make provision-lockdown-download     PROVISION_PORT=/dev/ttyACM0 CONFIRM_IRREVERSIBLE=disable-download-mode
make provision-lockdown-direct-boot  PROVISION_PORT=/dev/ttyACM0 CONFIRM_IRREVERSIBLE=disable-direct-boot
make provision-lockdown-rom-print    PROVISION_PORT=/dev/ttyACM0 CONFIRM_IRREVERSIBLE=disable-rom-print
```

After download mode is disabled you can no longer re-flash over serial; the
device is OTA-only. Be certain the OTA path works first.

### 6. Power-glitch protection

Separate again, and only after this exact board revision has been tested for
false positives (an over-eager glitch detector can brick good boards):

```sh
make provision-power-glitch-protection \
  PROVISION_PORT=/dev/ttyACM0 \
  CONFIRM_IRREVERSIBLE=enable-power-glitch
```

### 7. Final acceptance

```sh
make provision-summary       PROVISION_PORT=/dev/ttyACM0
make validate-device-state   VALIDATE_STAGE=production VALIDATE_PORT=hid
```

The production validation stage runs scriptable `nockster-cli` expectation
checks for HMAC-UP/NVS-v2, OTA readiness, secure boot, flash encryption,
lockdown, and power-glitch protection. It writes nothing. A clean pass here is
the ship gate.

## Irreversibility map

| Step | Reversible? | Failure consequence |
|------|-------------|---------------------|
| HMAC-UP key burn | No | wrong key → NVS v2 binds to garbage pepper; seed store unusable on this chip |
| Secure-boot digest burn | No | chip only runs images signed by that key forever |
| Flash-encryption key burn | No | flash readable only on-chip |
| Enable flash encryption | No | flash contents encrypted in place |
| JTAG / download / direct-boot / ROM-print lockdown | No | debug & re-flash avenues gone; OTA-only |
| Power-glitch protection | No | brick risk if board revision glitches falsely |

When unsure at any irreversible step: stop, run `provision-summary`, and
re-read the relevant `scripts/provision/README.md` section. A board you did not
burn is recoverable; one you burned wrong usually is not.
