# Hardware smoke checks

Use this after flashing a test build. By default it does not wipe, seed,
unlock, sign a draft, or write eFuses. If the device is already unlocked, it
verifies that advertised seed-slot pubkeys can be fetched through the request
path.

```sh
target/x86_64-unknown-linux-gnu/release/nockster-cli smoke --port hid
```

The command checks:

- protocol hello
- firmware info and advertised feature bits
- lock status
- NVS/security status
- seed-slot pubkey fetches when the device is already unlocked
- health signing only when the device already has a seed loaded and is unlocked

For a real end-to-end hardware signing check, opt in with a known draft. This
requires on-device approval and writes a signed transaction output:

```sh
target/x86_64-unknown-linux-gnu/release/nockster-cli smoke --port hid --sign-draft known-good.draft --out smoke.tx --host-txid
```

To change the PIN without sending the new PIN over USB, start the flow from the
CLI and enter the new PIN twice on the touchscreen:

```sh
target/x86_64-unknown-linux-gnu/release/nockster-cli pin --port hid --current-pin 0208
```

To recalibrate touch after flashing or changing display timing, run:

```sh
target/x86_64-unknown-linux-gnu/release/nockster-cli touch --port hid --calibrate
```

The device will show four targets. Touch each one once and wait for the CLI to
print the saved calibration.

For chip-security status, build firmware explicitly with:

```sh
FW_PROFILE=chip-security make flash
```

Default firmware still reports NVS status, but prints chip security as hidden.

For a provisioned chip-security board, use explicit expectations so the command
fails by exit status instead of relying on manual inspection:

```sh
target/x86_64-unknown-linux-gnu/release/nockster-cli security --port hid \
  --expect-chip-security \
  --expect-hmac-up \
  --expect-hmac-up-read-protected \
  --expect-nvs-v2
```

The same expectation checks can be run through the validation wrapper:

```sh
make validate-device-state VALIDATE_STAGE=hmac-up VALIDATE_PORT=hid
```

During signed-update validation, the passive update status can be read without
starting or cancelling a stream. It also reports `otadata`, OTA slot presence,
slot offsets/sizes, selected slot, next slot, and bootloader image state:

```sh
target/x86_64-unknown-linux-gnu/release/nockster-cli update status --port hid
```

To make that check scriptable during OTA validation:

```sh
target/x86_64-unknown-linux-gnu/release/nockster-cli update status --port hid \
  --expect-idle \
  --expect-ota-ready
```

To exercise the non-destructive reboot protocol without starting an update:

```sh
target/x86_64-unknown-linux-gnu/release/nockster-cli reboot --port hid
```

The same check is available through the validation wrapper:

```sh
make validate-device-state VALIDATE_STAGE=reboot VALIDATE_PORT=hid
```

The wrapper also has stages for repeatable release checks:

```sh
make validate-device-state VALIDATE_STAGE=update-ready VALIDATE_PORT=hid
make validate-device-state VALIDATE_STAGE=secure-boot VALIDATE_PORT=hid
make validate-device-state VALIDATE_STAGE=flash-encryption VALIDATE_PORT=hid
make validate-device-state VALIDATE_STAGE=lockdown VALIDATE_PORT=hid
make validate-device-state VALIDATE_STAGE=production VALIDATE_PORT=hid
```

Use `VALIDATE_DRY_RUN=1` to print the commands without opening the device.
