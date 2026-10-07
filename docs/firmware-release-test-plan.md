# Firmware / release / OTA test plan (no eFuse burning)

Goal: exercise **all** the release-version, signing, trust-anchor, and OTA-update code on a
real dev unit **without** doing anything irreversible, so the board stays reusable.

## What is and isn't reversible

- **The firmware never burns eFuses at runtime** — regardless of build profile. The
  `chip-security`/`production` features only let the firmware *read* eFuse state for the
  security report (`crates/nockster-fw/src/security.rs` is all `Efuse::read_*`).
- **Anti-rollback is software**, not the hardware eFuse counter: the device compares a
  manifest's `release_version` against the firmware's own compile-time
  `NOCKSTER_RELEASE_VERSION` (`crates/nockster-core/src/update.rs` →
  `verify_update_release_version`). So you can flash any version up or down freely.
- **Everything here is reversible**: build, flash, OTA install, reset, `make wipe` (erases
  NVS + otadata partitions, not eFuses).
- **The ONLY irreversible things** are the explicit `make provision-*` targets, each gated
  by a `CONFIRM_IRREVERSIBLE=<token>`. See the **DO NOT RUN** list at the bottom. As long as
  you never type a `CONFIRM_IRREVERSIBLE=` token, nothing burns.

> Re-run the **security status** check (Phase 0) after each phase. As long as `secure boot`,
> `flash enc`, and every lockdown bit stay `false`/empty, the board is still pristine.

---

## Phase 0 — Baseline: confirm the board is pristine

1. Flash a dev build: `make flash FLASH_PORT=/dev/ttyACM0` (or flash via the web app's
   firmware panel).
2. Read security status — either the web **Status** panel (chip security / efuse / hmac
   rows) or `cargo run -p nockster-cli -- security --port hid`.
3. **Expect:** `secure_boot = false`, `flash_encryption = false`, all JTAG/download/direct-boot
   "disabled" bits `false`, no HMAC user-key or read-protected slots. Record this; it's your
   "nothing burned" reference.

## Phase 1 — Basic device flow + the new in-app dialogs

Using the web app (connect over USB):
1. Seed the device, set a PIN, unlock.
2. View wallet slots + addresses; copy an address; sign a draft tx; lock.
3. Exercise the **new confirm dialogs** (these replaced the browser popups): **remove a seed
   slot**, **reset PIN**, **reset device**. Confirm each shows the in-app modal and that
   Cancel/✕ aborts and the action only runs on confirm.
4. Reset the device (in-app **Reset device** → "Erase device") to start clean.

## Phase 2 — Release-version build matrix

The release version is baked at build time (`NOCKSTER_RELEASE_VERSION`).
1. Build + flash version 1: `make flash FLASH_PORT=... NOCKSTER_RELEASE_VERSION=1`
2. In the web **Status / Firmware update** panel, confirm **device release = 1** and
   **device build** shows the right profile/protocol.
3. Repeat with `=2`, `=3`. Each plain flash overrides the version (plain flashing bypasses
   anti-rollback; OTA in Phase 4 enforces it).

## Phase 3 — OTA happy path (the core test)

You need a signing key and the device must **trust** it (its pubkey SHA-256 baked into the
running firmware as the trust anchor).

1. Make a throwaway test key (keep it outside the repo):
   `cargo run -p nockster-cli -- update keygen --out "$HOME/nockster-test.key"`
2. Get its trust anchor hash:
   `cargo run -p nockster-cli -- update pubkey --signing-key-file "$HOME/nockster-test.key"`
   → copy the 64-hex SHA-256.
3. Flash the device with firmware that trusts that key, at a **starting** version:
   `make flash FLASH_PORT=... NOCKSTER_RELEASE_VERSION=1 NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=<sha>`
4. In the web **Firmware update** panel: **check trust** (the trust anchor should equal
   `<sha>`), **check boot** (note current/next OTA slot).
5. Build + sign a **newer** bundle with the same key/profile:
   `make signed-update FW_PROFILE=dev NOCKSTER_RELEASE_VERSION=2 \
       UPDATE_SIGNING_KEY_FILE="$HOME/nockster-test.key" \
       NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=<sha>`
   (emits `target/update/nockster-fw.bin` + `nockster-fw.update.json`).
6. Install it — web **Firmware update → install** (advanced panel: load the bundle JSON + the
   .bin, **verify manifest**, **verify image**, **install**), or
   `cargo run -p nockster-cli -- update device-install --port hid …` (see `--help`).
7. **Expect:** streams into the *inactive* OTA slot → verifies signature + image SHA-256 →
   activates it → prompts reboot (new in-app modal) → after reboot the device marks the image
   valid. Re-check: **device release = 2**, **OTA boot** current slot flipped.

## Phase 4 — OTA negative tests (all reversible, this is the important matrix)

With the Phase-3 setup, confirm each of these is **rejected** on-device:
1. **Rollback:** sign a bundle with `NOCKSTER_RELEASE_VERSION=1` (≤ installed) and try to
   install → rejected (`RollbackVersion`). Try `=2` again (equal) → also rejected.
2. **Untrusted key:** keygen a *second* key, sign a `=3` bundle with it, try to install →
   rejected (trust-anchor / signature mismatch). Use `update device-verify` /
   `update device-stream-verify` to see the failure without writing flash.
3. **Tampered image:** flip a byte in the `.bin` (keep the manifest) → image SHA-256 mismatch
   → rejected at finish/verify.
4. **Wrong target/profile:** sign with a different `--hardware-target` or `--build-profile`
   than the device → policy rejection.

After each, the device should remain on the previous good image (no bricking, no fuse change).

## Phase 5 — Trust-anchor badge (web)

1. Flash with `NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=5aa46209222080a2ce107e25d427c3d9ada6cb77be25d7d2a3df8959b7fa2602`
   (the official SWPSCo key) → web trust-anchor row shows the **✓ SWPSCo!** badge.
2. Flash with any other anchor → no badge, raw hash only. (Pure UI/firmware-config check; no
   burning.)

## Phase 6 — chip-security profile (still no burning)

1. `make fw-chip-security` then `make flash FLASH_PORT=...`.
2. Read security status → `chip_security_available` is now `true`, but every actual eFuse
   field (`secure_boot`, `flash_encryption`, lockdown bits, key slots) is still
   `false`/empty. This proves the reporting path works without provisioning.
3. Reset / re-flash dev when done.

## Phase 7 — production release path, dry-run only (no flash, no burn)

1. Metadata-only build: `make fw FW_PROFILE=production ALLOW_UNSIGNED_PRODUCTION=1`.
2. `make release-preflight …` and `make provision-plan PROVISION_PORT=/dev/ttyACM0` — these
   **print** what a production provisioning *would* do; they do not burn.
3. `make validate-device-state VALIDATE_PORT=hid` — read-only state check.
4. **Stop here.** Do not run the actual burn targets.

## Recovery between iterations

- `cargo run -p nockster-cli -- reset --port hid` (or web **Reset device**) — wipes seed +
  persistent state.
- `make wipe` — erases the NVS + otadata partitions and re-flashes (still no eFuses).

## DO NOT RUN on this board (these burn eFuses — one-way, board becomes locked/sacrificial)

Any target that requires a `CONFIRM_IRREVERSIBLE=` token:

- `make provision-hmac-up`                        (burns HMAC_UP key → BLOCK_KEY5)
- `make provision-secure-boot-v2-digest`          (burns secure-boot digest)
- `make provision-flash-encryption-key`           (burns XTS-AES key)
- `make provision-flash-encryption-enable`        (burns SPI_BOOT_CRYPT_CNT)
- `make provision-lockdown-jtag` / `-download` / `-direct-boot` / `-rom-print`
- `make provision-power-glitch-protection`

If you never pass a `CONFIRM_IRREVERSIBLE=` token, you cannot burn anything by accident.
