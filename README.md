# nockster-esp
Hardware wallet firmware, host tooling, and web front-end for signing Nockchain transactions on [ESP32-S3](ESP32-S3-Touch-LCD-1.47).

The CLI/wasm app deserializes draft jams, sends the input values over the wire for signature, recomputes `tx_id`, jams the signed noun, and hands it back as a `.tx`.

NVS data is AES-256-GCM encrypted with a key derived from the PIN and per-device salt. This protects normal firmware updates and casual flash reads, but low-entropy PINs should be paired with the ESP32-S3 eFuse/HMAC hardening described in `docs/security.md`.

## Components
- `crates/nockster-core` — library containing comms protocol and crypto wrappers shared between CLI, firmware and wasm (see also separate `tx-types` repo)
- `crates/nockster-cli` — desktop binary for interacting with device over USB
- `crates/nockster-fw` — esp32 firmware; talks serial/WebUSB/WebSerial, signs transactions without exposing key material, and persists encrypted seed material in NVS
- `crates/nockster-wasm` — wasm build of the toolchain used by the browser client
- `nockster-js` — thin typescript wrapper around the device protocol, consumed by the web app
- `web` — demo vite/react interface to drive the device from browser
- `src-tauri` — cross-platform desktop app bundler (see [TAURI_SETUP.md](TAURI_SETUP.md))

## Device protocol
- Transport is plain USB CDC with COBS framing; every message is a postcard-encoded `Request` or `Response` enum from `nockster-core`.
- Typical flow: `GetInfo`/`GetLockStatus` -> `Unlock { pin }` -> `GetCheetahPub { path }` ->  `SignSpendHash { path, msg5 }`
- Error handling mirrors the CLI: firmware replies with `Response::Err { code }` and the host decides whether to retry or surface it.
- The JS wrapper in `nockster-js` and the Rust CLI both share the same codec, so debugging on desktop transfers directly to the browser.

## Flash
- Bootloader lives at `0x0` (32 KB) followed by the partition table at `0x8000`.
- NVS starts at `0x9000` (28 KB slot, ~24 KB effectively used) and stores the encrypted seed, salt, nonce, and attempt counter.
- Factory firmware is placed at `0x10000` with a 3 MB ceiling. The 16 MB flash layout also reserves two 3 MB OTA app slots for signed self-updates.
- `make flash` updates firmware without touching NVS; use `make flash-wipe` when you really need a factory reset.

#### Partitions

| Address | Size  | Type       | Purpose                        |
|---------|-------|------------|--------------------------------|
| 0x0     | 32KB  | Bootloader | First-stage bootloader         |
| 0x8000  | —     | Partition  | Partition table                |
| 0x9000  | 28KB  | NVS        | Encrypted seed storage (PIN)   |
| 0x10000 | 3MB   | APP        | Factory firmware binary        |
| 0x310000 | 8KB  | OTA Data   | OTA boot slot metadata         |
| 0x320000 | 3MB  | APP        | OTA slot 0                     |
| 0x620000 | 3MB  | APP        | OTA slot 1                     |

#### Seeds



## Commands
- Provision from scratch: `make flash-wipe`, then `nockster-cli seed --port /dev/ttyACM0 --mnemonic "..." --pin 1234`.
- Firmware update: `make flash` (seed stays put).
- Sanity check after flashing: `nockster-cli info --port /dev/ttyACM0` followed by a lock/unlock cycle to confirm the seed loads from NVS.
- Other shortcuts:
  - `make fw` builds the ESP32-S3 image, `make monitor` tails the serial console.
  - `make wasm` rebuilds the web bundle (`crates/nockster-wasm/pkg`) for the browser client.
    - `cd web && npm run dev` to use webapp wallet
    - `make tauri-dev` to run the desktop app
  - `make cli` and `make core` build the host tooling and shared library shared between firmware iterations.
    - CLI app available at `target/x86_64-unknown-linux-gnu/release/nockster-cli`

## Notes

---

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
- `make serve` to build and serve the demo webui (includes wasm build)
- `make cli` to build the CLI tool `nockster-cli`
- `target/x86_64-unknown-linux-gnu/release/nockster-cli smoke --port hid` to run a non-destructive hardware smoke check
- `make tauri` to build the desktop app
