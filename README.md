# nockster

Nockster is hardware-wallet firmware, host tooling, and a browser interface for
signing Nockchain transactions on an [ESP32-S3-Touch-LCD-1.47](https://www.waveshare.com/esp32-s3-touch-lcd-1.47.htm) touchscreen board.

<img width="817" height="803" alt="image" src="https://github.com/user-attachments/assets/3273d96b-a30e-46d0-add1-eda43ad52d79" />

## Features

- On-device seed storage, transaction review, approval, and signing.
- USB HID transport by default, with serial/COBS still available for firmware
  development and diagnostics.
- Browser UI over WebHID for seed management, transaction signing, device
  status, touch calibration, and firmware updates.
- CLI for scripting the same workflows from a terminal.
- AES-256-GCM encrypted seed slots in flash, keyed by PIN-derived schema-v2 NVS
  storage.
- Optional ESP32-S3 chip-security path for eFuse/HMAC-backed NVS hardening,
  secure boot, flash encryption, and release validation.
- Signed firmware update bundles with on-device manifest and image validation.
- WASM transaction composer/parser shared by the browser app.
- Tauri desktop wrapper for packaging the web UI as a local app.
- Encrypted preimage vault for `%hax` lock secrets (HTLCs, commit-reveal),
  with device-computed Tip5 commitments and on-screen reveal confirmation.
  See [docs/preimage-vault.md](docs/preimage-vault.md).
- `keys.export` / `master-pubkey.export` interop with the official
  nockchain-wallet CLI: import its keyfiles, export watch-only keyfiles it
  can import. See [docs/wallet-keyfile-interop.md](docs/wallet-keyfile-interop.md).

## User Workflows

- Initialize a device with a mnemonic and PIN.
- Add, list, select, label, and delete seed slots.
- Unlock or lock the device from the screen, CLI, or browser.
- Compose or load a transaction draft, review it on the device, and export the
  signed transaction.
- Store, reveal, and delete `%hax` preimages in the on-device vault.
- Export a slot's master pubkey as a watch-only keyfile for nockchain-wallet,
  or import a nockchain-wallet `keys.export` seed phrase.
- Calibrate the touchscreen and run hardware smoke checks.
- Install signed firmware updates from a hosted update page or from local
  release artifacts.

## Repository Layout

- `crates/nockster-core`: shared protocol types, request/response codec, and
  crypto wrappers.
- `crates/nockster-fw`: ESP32-S3 firmware, touchscreen UI, USB HID/serial
  transports, NVS storage, and update verifier.
- `crates/nockster-cli`: desktop CLI for device operations and release/admin
  checks.
- `crates/nockster-wasm`: WASM bindings for browser transaction tooling.
- `nockster-js`: TypeScript device client used by the web app.
- `web`: Vite/React browser UI.
- `src-tauri`: desktop app wrapper. See [TAURI_SETUP.md](TAURI_SETUP.md).
- `docs`: hardware smoke checks, security/provisioning notes, update flow,
  preimage vault and wallet-keyfile interop guides, and the roadmap of
  potential paths forward ([docs/roadmap-ideas.md](docs/roadmap-ideas.md)).

## Quick Commands

- Build firmware: `make fw`
- Flash firmware without erasing seed storage: `make flash`
- Build signed OTA artifacts: `make signed-update`
- Flash and erase persistent device data: `make wipe`
- Build the CLI: `make cli`
- Seed a wiped device over HID: `nockster-cli seed --seedphrase "..." --pin 1234`
  (or `--keyfile keys.export` to use a nockchain-wallet export)
- Manage the preimage vault: `nockster-cli vault list|store|reveal|delete`
- Export a watch-only keyfile: `nockster-cli export-master-pubkey --slot 0`
- Check device info: `nockster-cli info`
- Run a hardware smoke check:
  `target/x86_64-unknown-linux-gnu/release/nockster-cli smoke`
- Serve the browser UI locally: `make serve`
- Build the web/WASM bundle: `make wasm`
- Start the desktop app in development: `make tauri-dev`

The CLI defaults to HID. Use `--device hid` or `--port hid` explicitly when you
want to be clear, and use `--port /dev/ttyACM0` for the serial path.

## Firmware Layout

| Address | Size | Purpose |
| --- | --- | --- |
| `0x0` | 32 KB | Bootloader |
| `0x8000` | varies | Partition table |
| `0x9000` | 28 KB | Encrypted NVS seed storage |
| `0x10000` | 3 MB | Factory firmware image |
| `0x310000` | 8 KB | OTA boot metadata |
| `0x320000` | 3 MB | OTA slot 0 |
| `0x620000` | 3 MB | OTA slot 1 |

## Security Notes

Seed slots are encrypted in flash with AES-256-GCM. The active storage schema
uses a PIN-derived key, per-device salt, and v2 pepper input. Dev/test builds
use a software pepper so boards can be wiped and reseeded without eFuse
provisioning. Production hardening is documented in [docs/security.md](docs/security.md).

## Development

### Dependencies

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup toolchain install nightly
rustup +nightly target add wasm32-unknown-unknown
cargo install tauri-cli --version "^2.8.4"
# esp-idf
## linux/debian deriv
sudo apt-get install git wget flex bison gperf python3 python3-pip python3-venv cmake ninja-build ccache libffi-dev libssl-dev dfu-util libusb-1.0-0 libwebkit2gtk-4.1-dev build-essential wget file libayatana-appindicator3-dev librsvg2-dev libudev-dev libusb-1.0-0-dev patchelf

## macos
brew install cmake ninja dfu-util

pip install -r pyserial-miniterm # optional for serial connections
mkdir -p ~/esp
cd ~/esp
git clone -b v5.5.1 --recursive https://github.com/espressif/esp-idf.git
cd ~/esp/esp-idf
./install.sh esp32s3
. $HOME/esp/esp-idf/export.sh
echo "alias get_idf='. $HOME/esp/esp-idf/export.sh'" >> ~/.bashrc # or .zshrc

# espup
cargo install espup --locked
espup install
cargo install espflash
```

### Building

The `Makefile` has scripts for building everything -- run `make help` to see all options.

You probably just want to run one of these:

- `make flash` to re-flash the esp
  - `make wipe` to re-flash and erase persistent data (keys)
- `make serve` to build and serve the browser UI (includes wasm build)
- `make cli` to build the CLI tool `nockster-cli`
- `target/x86_64-unknown-linux-gnu/release/nockster-cli smoke` to run a non-destructive hardware smoke check
- `make tauri` to build the desktop app
