# Secure self-updates

This is the signed user update path for release firmware. Normal users should
not need serial flashing or CLI commands: the intended flow is to visit the
updater site, plug in the device, click `update firmware`, approve the browser
device prompt, and let the site stream the signed release. The CLI commands
below are release-operator and development tooling.

## Trust model

- The host is transport only. It can download, cache, and stream update
  bundles, but the device must decide whether a bundle is trusted.
- A bundle carries the compressed SEC1 release signing public key for
  transparency and tooling convenience.
- Firmware must compare that bundled key against a pinned trust anchor before
  verifying the signature. The current foundation uses
  `SHA256(compressed_pubkey_sec1)` as the trust anchor.
- Release private keys stay offline or in a dedicated release environment.
  They must not be embedded in firmware, the CLI, the web updater, or checked
  into the repo.

## Bundle format

`nockster-cli update sign` writes JSON with:

- `format = "nockster-update-bundle-v1"`
- `signature_scheme = "secp256k1-ecdsa-sha256-prehash-v1"`
- a manifest containing:
  - monotonic `release_version`
  - firmware `image_size`
  - firmware `image_sha256`
  - `signing_pubkey_sha256`
  - `hardware_target`
  - `build_profile`
  - protocol version
  - firmware git commit
  - tx-types revision
- bundled compressed SEC1 public key
- 64-byte ECDSA signature over the domain-separated postcard manifest digest

The JSON form is for tooling and browser USB hosts. The current web app uses
WebHID/Web Serial; a future WebUSB transport should reuse the same hosted index,
bundle parser, and device-side signature checks. The signed bytes are the
postcard-encoded `nockster_core::update::UpdateManifest` under the
`nockster-fw-update-v1` domain separator, so firmware can verify the same data
without trusting JSON canonicalization.

## CLI release tooling

Generate a release signing key as a local secret file:

```sh
nockster-cli update keygen --out ../nockster-secrets/release-signing-key.hex
nockster-cli update pubkey --signing-key-file ../nockster-secrets/release-signing-key.hex
```

The same flow is exposed through guarded Make targets:

```sh
make generate-update-signing-key UPDATE_SIGNING_KEY_FILE=../nockster-secrets/release-signing-key.hex
make update-pubkey UPDATE_SIGNING_KEY_FILE=../nockster-secrets/release-signing-key.hex
```

`keygen` refuses repo-local output paths when run from a git checkout, refuses
to overwrite an existing key file, and creates newly generated key files with
mode `0600` on Unix. The CLI accepts either raw 32-byte key files or UTF-8 hex.
Keep this file out of the repo and outside test fixtures. `update sign` and
`update pubkey` also reject repo-local signing-key paths when run from a git
checkout. The `pubkey` command prints the compressed SEC1 public key and the
`trusted_pubkey_sha256` value to pin into firmware. Hex key files are decoded
directly into a fixed-size key buffer and the raw key-file bytes are zeroized
after parsing.

## Permanent release signing key runbook

This project uses one long-lived firmware update signing key and pins the
device to `SHA256(compressed_pubkey_sec1)`. The private key signs update
manifests; it is not flashed to the device and is not needed by users. If you
intend to practice with the same key you will use for production, create it
once outside this repo and then keep using only that file for releases.

1. Create an external secrets directory:

```sh
mkdir -p ../nockster-secrets
chmod 700 ../nockster-secrets
```

2. Generate the permanent update signing key once:

```sh
make generate-update-signing-key \
  UPDATE_SIGNING_KEY_FILE=../nockster-secrets/nockster-release-signing-key.hex
```

This target refuses to overwrite an existing key. Back up this exact file
offline before treating any board as production-trusted. Do not copy it into
the repo, web assets, tests, firmware source, or checked-in CI secrets.

3. Optionally create the repo-local ignored alias used by Make defaults:

```sh
mkdir -p .secrets
ln -s /path/to/outside-repo/release-signing-key.hex .secrets/update-signing.key
```

`.secrets/` is gitignored. The CLI and release preflight resolve existing
symlinks before checking whether a secret path is inside the repo, so this
alias is allowed only when it points at an outside-repo key file.

4. Record the public release identity:

```sh
make update-pubkey
```

Save both printed values somewhere operator-visible:

- `signing_pubkey_sec1`: the compressed public key carried in release bundles.
- `trusted_pubkey_sha256`: the firmware trust anchor to pin into builds.

The current default trust anchor is:

```text
5aa46209222080a2ce107e25d427c3d9ada6cb77be25d7d2a3df8959b7fa2602
```

5. Build firmware that trusts this key:

```sh
make fw \
  NOCKSTER_RELEASE_VERSION=1
```

`NOCKSTER_RELEASE_VERSION` is the rollback floor compiled into that firmware.
Any signed update installed over it must use a strictly greater
`--release-version`. For practice, start at `1`, then sign update bundles as
`2`, `3`, and so on. Do not reuse or decrement release numbers on a board that
has already booted a higher release.

6. Build a signed update bundle for the next release:

The Makefile wraps app-image generation, signing, and trust-anchor
verification:

```sh
make signed-update NOCKSTER_RELEASE_VERSION=2 FW_PROFILE=production ALLOW_UNSIGNED_PRODUCTION=1
```

It writes `target/update/nockster-fw.bin` and
`target/update/nockster-fw.update.json` by default.

Manual equivalent:

```sh
nockster-cli update sign \
  --firmware target/xtensa-esp32s3-none-elf/release/nockster-fw.bin \
  --out nockster-fw.update.json \
  --signing-key-file .secrets/update-signing.key \
  --release-version 2 \
  --hardware-target esp32s3-touch-lcd-1.47 \
  --build-profile production
```

7. Verify the bundle with the pinned public-key hash:

```sh
nockster-cli update verify \
  --bundle nockster-fw.update.json \
  --firmware target/xtensa-esp32s3-none-elf/release/nockster-fw.bin \
  --trusted-pubkey-sha256 5aa46209222080a2ce107e25d427c3d9ada6cb77be25d7d2a3df8959b7fa2602
```

8. Publish browser update assets for local practice:

```sh
make update-web-assets \
  UPDATE_BUNDLE=nockster-fw.update.json \
  UPDATE_FIRMWARE=target/xtensa-esp32s3-none-elf/release/nockster-fw.bin
```

Then run `make serve`, open the device tab, connect the device, and use
`update firmware`. The browser fetches `/updates/latest.json`, but the device
still validates the manifest signature and streamed firmware digest before
activating the inactive OTA slot.

9. Confirm the connected device trusts the expected key:

```sh
nockster-cli update trust --port hid
```

The printed `trusted_pubkey_sha256` must match the value from step 3. If it
does not, that firmware was built with a different trust anchor or without a
secure-update trust anchor.

Key rotation should be treated as a separate migration project. A device that
only knows key A cannot trust bundles signed by key B unless firmware signed
by key A first installs a new trust policy.

Create a bundle:

```sh
nockster-cli update sign \
  --firmware target/xtensa-esp32s3-none-elf/release/nockster-fw.bin \
  --out nockster-fw.update.json \
  --signing-key-file ../nockster-secrets/release-signing-key.hex \
  --release-version 1 \
  --hardware-target esp32s3-touch-lcd-1.47 \
  --build-profile production
```

`update sign` derives the firmware git commit from `git rev-parse HEAD` and
the tx-types revision from the workspace `Cargo.toml` pin when
`--git-commit` or `--tx-types-rev` are not supplied. If the working tree is
dirty, it prints a warning because the manifest records only `HEAD`.

Generate the browser latest-release index for one-click site updates:

```sh
nockster-cli update index \
  --bundle nockster-fw.update.json \
  --firmware target/xtensa-esp32s3-none-elf/release/nockster-fw.bin \
  --out latest.json \
  --bundle-url nockster-fw.update.json \
  --firmware-url nockster-fw.bin
```

The same index generator is exposed through Make:

```sh
make update-index \
  UPDATE_BUNDLE=nockster-fw.update.json \
  UPDATE_FIRMWARE=target/xtensa-esp32s3-none-elf/release/nockster-fw.bin \
  UPDATE_INDEX=latest.json
```

For local browser testing with `make serve`, publish the same files into Vite's
public directory so `/updates/latest.json` exists:

```sh
make update-web-assets \
  UPDATE_BUNDLE=nockster-fw.update.json \
  UPDATE_FIRMWARE=target/xtensa-esp32s3-none-elf/release/nockster-fw.bin
```

The command parses the signed bundle and hashes the firmware file before
writing the index, so a stale or mismatched firmware binary is caught before
publication. If `--bundle-url` or `--firmware-url` is omitted, the CLI writes
the corresponding file name as a relative URL. Publish the index, bundle, and
firmware binary together under the web updater's configured release path.
Explicit artifact URLs are checked against the browser publication policy:
relative URLs are allowed, absolute URLs must be HTTPS, and plain HTTP is only
accepted for localhost testing. This keeps release automation from publishing an
index that the one-click updater cannot safely fetch.

Derive the public key and trust anchor from a private key file:

```sh
nockster-cli update pubkey --signing-key-file ../nockster-secrets/release-signing-key.hex
```

Verify a bundle with the same trust rule firmware will use:

```sh
nockster-cli update verify \
  --bundle nockster-fw.update.json \
  --firmware target/xtensa-esp32s3-none-elf/release/nockster-fw.bin \
  --trusted-pubkey-sha256 <sha256-of-compressed-release-pubkey>
```

Strict release preflight also derives `UPDATE_SIGNING_KEY_FILE` with
`nockster-cli update pubkey` and requires the resulting
`trusted_pubkey_sha256` to match `NOCKSTER_UPDATE_PUBKEY_SHA256_HEX`, then
verifies the signed bundle against that same trust anchor.

Ask a connected device which trust anchor it was built with:

```sh
nockster-cli update trust --port hid
```

Ask the device which release counter it is enforcing:

```sh
nockster-cli info --port hid
```

When the firmware advertises `release-info`, the output includes
`release: version=<n>`. Update bundles must be signed with a manifest
`release_version` greater than that value. The `device-verify`,
`device-stream-verify`, and `device-install` commands also read this counter
and, when available, firmware build metadata before asking the device to verify
or stream an update, so obvious rollback, protocol, target, and production
profile failures stop before image transfer.

Read the current update stream status plus OTA bootloader/partition state:

```sh
nockster-cli update status --port hid
```

The stream half reports whether an update session is active and how many bytes
have been received. The boot half reports whether the partition table was
readable, whether `otadata`, `ota_0`, and `ota_1` exist, the OTA slot offsets
and sizes, the selected slot, the next slot, and the bootloader's image state
(`new`, `pending-verify`, `valid`, `invalid`, `aborted`, or `undefined`).

For scripted hardware validation, make the status command fail by exit code
when the expected OTA layout or boot state is not present:

```sh
nockster-cli update status --port hid \
  --expect-idle \
  --expect-ota-ready \
  --expect-current-slot ota0 \
  --expect-next-slot ota1 \
  --expect-ota-state valid
```

Ask the device to verify a bundle manifest signature on-device:

```sh
nockster-cli update device-verify --port hid --bundle nockster-fw.update.json
```

This does not flash anything. It exercises the same trust decision used by
stream verification and OTA installation before firmware accepts image bytes.

Ask the device to verify both the signed manifest and the streamed firmware
image digest on-device without flashing:

```sh
nockster-cli update device-stream-verify \
  --port hid \
  --bundle nockster-fw.update.json \
  --firmware target/xtensa-esp32s3-none-elf/release/nockster-fw.bin
```

During this flow the host still only transports bytes. Firmware enters an
update session, rejects signing and seed-management requests, hashes chunks as
they arrive, and accepts `FinishUpdate` only when the received byte count and
SHA-256 digest match the signed manifest.
The CLI also checks each returned begin/chunk/finish status against the signed
manifest and expected byte offset, and cancels the stream if progress is stale
or metadata no longer matches.

Install a signed bundle into the inactive OTA slot:

```sh
nockster-cli update device-install \
  --port hid \
  --bundle nockster-fw.update.json \
  --firmware target/xtensa-esp32s3-none-elf/release/nockster-fw.bin \
  --reboot
```

This uses the same on-device signature and digest checks as
`device-stream-verify`, but `BeginUpdate` also asks firmware to open the
inactive OTA slot. Chunks are written as they are streamed into sector buffers;
firmware explicitly erases each inactive-slot sector before writing it. The
slot is activated only after `FinishUpdate` verifies the complete streamed image
digest and reads the written slot back from flash to compare its digest against
the signed manifest. After a successful install, the CLI immediately reads OTA
boot status and requires the selected slot to be `ota_0` or `ota_1` with image
state `new`. Pass `--reboot` to use the append-only non-destructive reboot
request after that validation succeeds; without it the command prints an
instruction to reboot manually.

For standalone hardware-validation or admin workflows, the same non-destructive
request is also exposed without starting an update:

```sh
nockster-cli reboot --port hid
```

This is not part of the normal user upgrade path; the hosted updater should
offer the reboot step in-browser after a successful OTA install.

## Web host

The browser device tab includes a firmware update panel for WebHID/Web Serial
sessions. It accepts the same bundle JSON and firmware `.bin` produced by the
CLI release flow. The browser only transports bytes: manifest verification,
streamed image hashing, and OTA activation decisions stay on the device.
When firmware advertises update boot status, the panel also shows `otadata`,
OTA slot presence, offsets/sizes, selected slot, next slot, and bootloader
image state, with a manual refresh control for hardware validation.

Use `verify manifest` for a trust-anchor/signature preflight, `verify image`
to stream and hash the image without flashing, and `install` to write the
verified image to the inactive OTA slot. After install, the browser refreshes
OTA boot status and uses the shared `nockster-js` post-install validator to
require the selected slot to be an OTA slot in `new` state before reporting
success. When release/build metadata is available, the browser applies the
shared `nockster-js` manifest policy preflight before enabling these actions,
so obvious rollback, protocol, target, image size, and production-profile
failures stop before transfer.
The browser update library also validates returned begin/chunk/finish progress
against the signed manifest and expected byte offsets before continuing.
Firmware also exposes an append-only non-destructive `Reboot` request when the
`device-reboot` feature bit is set, so the hosted browser updater can reboot
automatically after a verified OTA install. Advanced local bundle installs
still ask before rebooting.

The normal user path is site-hosted, not CLI-driven. Build or deploy the web app with
`VITE_NOCKSTER_RELEASE_INDEX_URL` pointing at the latest-release index, or
serve `/updates/latest.json` from the same site. The index is JSON:

```json
{
  "format": "nockster-release-index-v1",
  "bundle_url": "nockster-fw.update.json",
  "firmware_url": "nockster-fw.bin",
  "release_version": 1,
  "image_size": 123456,
  "image_sha256_hex": "...",
  "hardware_target": "esp32s3-touch-lcd-1.47",
  "build_profile": "production",
  "protocol_v": 1,
  "git_commit": "...",
  "tx_types_rev": "..."
}
```

Only `bundle_url` and `firmware_url` are required by the browser; the CLI adds
manifest metadata for operator review, cache/debug visibility, and an early
browser preflight. `nockster-js` owns the hosted-release fetch path used by the
web app: it validates the index shape, resolves relative artifact URLs relative
to the index URL, requires the index and artifacts to use HTTPS except on
localhost, fetches with `cache: no-store`, uses same-origin credentials for
same-site artifacts, checks optional index metadata against the signed bundle
before downloading firmware, rejects mismatched firmware `Content-Length`, and
hashes the firmware against the signed manifest. The `update firmware` button
opens the browser device prompt if needed and streams the image to the device
for on-device signature, digest, OTA-slot, and activation checks. On firmware
that advertises `device-reboot`, the hosted update button requests reboot
automatically after the staged install so the device boots the selected OTA
slot. Older firmware still reports that the install is staged and asks the user
to press reset or replug. Users should only need to visit the site, plug in the
device, and click that button.

For private release distribution, the same panel can fetch a bundle URL and
firmware URL directly. A bearer token is held only in browser memory, passed
through the shared `nockster-js` update fetcher, and cleared after a successful
fetch. If a token is present, both artifact URLs must share one origin and must
use HTTPS, except localhost testing; tokened latest-index fetches also require
the index, bundle, and firmware URLs to share one origin. The shared fetcher
attaches the bearer header only after that policy passes and defaults tokened
fetch credentials to `omit` so browser cookies are not sent with direct
bearer-token update requests. The browser also checks the firmware
`Content-Length`, when present, and hashes the loaded firmware bytes against
the signed manifest before streaming. These host checks are preflight only; the
device still verifies the manifest signature and final image digest before
accepting or activating an update.

The browser protocol library rejects malformed bundle and latest-release index
JSON before any device request: manifest version must be supported, numeric
manifest fields must be bounded integers, image size must be nonzero and
within the firmware limit, manifest strings must be present, hashes/signatures
must have exact lengths, the bundled release public key must be a 33-byte
compressed SEC1 key, index artifact URLs must be present and HTTP(S), and
optional index metadata must be well-typed with exact-length image hashes
before it can be compared to the signed bundle.

## Firmware path

The shared verifier lives in `nockster_core::update` and is `no_std`. Firmware
uses it as follows:

1. Enter update-only mode, reject signing/seed-management requests, and keep
   passive info/lock/security/build/release/trust/status reads available for
   host diagnostics.
2. Receive the bundle manifest, signature, and bundled public key.
3. Compare `SHA256(bundled_pubkey)` to the firmware-pinned trust anchor.
4. Verify the manifest signature on-device.
5. Stream the firmware image while hashing it on-device.
6. Compare the streamed image hash and size to the signed manifest.
7. Reject manifests whose `release_version` does not advance beyond the
   firmware build's current release counter.
8. Write verified chunks into an inactive OTA/staging slot.
9. Read the written slot back from flash and compare its digest to the signed
   manifest.
10. Activate the slot only after signature, digest, and write checks pass.
11. Mark the selected OTA image valid only after basic firmware initialization
   succeeds on the next boot.
12. Keep rollback metadata so a failed boot returns to the previous image.

Test builds can exercise this flow with a test trust anchor. Production
anti-rollback/eFuse enforcement remains gated behind the explicit production
provisioning flow.

To build firmware with an update trust anchor:

```sh
NOCKSTER_RELEASE_VERSION=1 \
NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=<sha256-of-compressed-release-pubkey> \
make fw
```

The Makefile passes that trust anchor into the firmware build only when it is
set. Empty values are unset for normal dev builds, so a stale exported empty
environment variable does not accidentally advertise secure-update support.
The same value can be passed as a make variable:

```sh
make fw \
  NOCKSTER_RELEASE_VERSION=1 \
  NOCKSTER_UPDATE_PUBKEY_SHA256_HEX=<sha256-of-compressed-release-pubkey>
```

Before secure-boot signing or strict release preflight, the provisioning
helpers sanity-check the app image input: it must start with the ESP image
magic byte, have a plausible segment count, and fit the configured app slot
size. This does not replace sacrificial-board bootloader validation, but it
catches obvious wrong-file and wrong-partition-flow mistakes before signing.

When that environment variable is set, firmware advertises `secure-update`,
`GetUpdateTrust` reports the pinned hash, and `VerifyUpdateManifest` checks the
bundle signature on-device against that pinned hash. `NOCKSTER_RELEASE_VERSION`
sets the current firmware rollback floor; firmware rejects an update bundle
unless the signed manifest has a strictly greater `release_version`.
Firmware also exposes that counter through append-only `GetReleaseInfo`, so the
CLI and browser can preflight rollback failures before streaming an image.
Before checking the signature, firmware also enforces local manifest policy:
the signed hardware target and protocol version must match the running firmware,
the image size must be nonzero and within the firmware limit, and production
firmware accepts only production-profile bundles. Dev and chip-security builds
may still install any supported bundle profile so signed-update testing does
not require production provisioning.
`BeginUpdate`,
`UpdateChunk`, `FinishUpdate`, `CancelUpdate`, and `GetUpdateStatus` now
exercise the update-only streaming verifier. `BeginUpdate { write_flash:
true }` additionally requires an `otadata` partition plus `ota_0` and `ota_1`
app partitions. Without the trust anchor, normal dev builds still compile but
do not advertise secure update support and reject manifest/update verification
with `ERR_UNSUPPORTED_VERSION`.
Host install tooling reads `GetUpdateBootStatus` after a successful flash
write and requires the selected OTA slot to report image state `new` before it
tells the user to reboot.

For diagnostics during a hardware validation run, the CLI can read passive
stream and boot status without entering update mode:

```sh
nockster-cli update status --port hid
```

Useful scripted checks:

```sh
nockster-cli update status --port hid --expect-idle --expect-ota-ready
nockster-cli update status --port hid --expect-ota-state new
```

The checked-in partition table keeps the factory image at `0x10000` and adds
two 3 MB OTA slots. The `esp-bootloader-esp-idf` OTA helper notes that some
prebuilt bootloaders may not include OTA support; hardware validation still
needs to confirm the flashed bootloader honors `otadata` on this board.

After basic startup succeeds, firmware marks a selected OTA image valid only
when `otadata` exists and the selected image is in `new` or `pending-verify`
state. It leaves `valid`, `invalid`, `aborted`, and `undefined` OTA metadata
untouched, so failed-boot rollback state is not accidentally overwritten.
If there is no OTA data partition, this is a no-op so factory/dev flashing
keeps working.
