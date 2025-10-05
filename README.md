# siger-esp
Hardware wallet firmware, host tooling, and web front-end for signing Nockchain transactions on [ESP32-S3](ESP32-S3-Touch-LCD-1.47).

The CLI/wasm app deserializes draft jams, sends the input values over the wire for signature, recomputes `tx_id`, jams the signed noun, and hands it back as a `.tx`.

NVS data is AES-256-GCM encrypted with a key derived from the PIN; dumping flash without the PIN is useless.

## Components
- `crates/siger-core` — library containing comms protocol and crypto wrappers shared between CLI, firmware and wasm (see also separate `tx-types` repo)
- `crates/siger-cli` — desktop binary for interacting with device over USB
- `crates/siger-fw` — esp32 firmware; talks serial/WebUSB/WebSerial, signs transactions without exposing key material, and persists encrypted seed material in NVS
- `crates/siger-wasm` — wasm build of the toolchain used by the browser client
- `siger-js` — thin typescript wrapper around the device protocol, consumed by the web app
- `web` — demo vite/react interface to drive the device from browser

## Device protocol
- Transport is plain USB CDC with COBS framing; every message is a postcard-encoded `Request` or `Response` enum from `siger-core`.
- Typical flow: `GetInfo`/`GetLockStatus` -> `Unlock { pin }` -> `GetCheetahPub { path }` ->  `SignSpendHash { path, msg5 }`
- Error handling mirrors the CLI: firmware replies with `Response::Err { code }` and the host decides whether to retry or surface it.
- The JS wrapper in `siger-js` and the Rust CLI both share the same codec, so debugging on desktop transfers directly to the browser.

## Flash
- Bootloader lives at `0x0` (32 KB) followed by the partition table at `0x8000`.
- NVS starts at `0x9000` (28 KB slot, ~24 KB effectively used) and stores the encrypted seed, salt, nonce, and attempt counter.
- Firmware image is placed at `0x10000` with a 3 MB ceiling (device has 16MB, maybe we put more stuff on it later?)
- `make flash` updates firmware without touching NVS; use `make flash-wipe` when you really need a factory reset.

#### Partitions

| Address | Size  | Type       | Purpose                        |
|---------|-------|------------|--------------------------------|
| 0x0     | 32KB  | Bootloader | First-stage bootloader         |
| 0x8000  | —     | Partition  | Partition table                |
| 0x9000  | 28KB  | NVS        | Encrypted seed storage (PIN)   |
| 0x10000 | 3MB   | APP        | Firmware binary                |

#### Seeds



## Commands
- Provision from scratch: `make flash-wipe`, then `siger-cli seed --port /dev/ttyACM0 --mnemonic "..." --pin 1234`.
- Firmware update: `make flash` (seed stays put).
- Sanity check after flashing: `siger-cli info --port /dev/ttyACM0` followed by a lock/unlock cycle to confirm the seed loads from NVS.
- Other shortcuts:
  - `make fw` builds the ESP32-S3 image, `make monitor` tails the serial console.
  - `make wasm` rebuilds the web bundle (`crates/siger-wasm/pkg`) for the browser client.
    - `cd web && npm run dev` to use webapp wallet
  - `make cli` and `make core` keep the host tooling and shared library honest between firmware iterations.
    - CLI app available at `target/x86_64-unknown-linux-gnu/release/siger-cli`

## Notes

---

## Development

### Dependencies

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup toolchain install nightly
rustup +nightly target add wasm32-unknown-unknown

# esp-idf
## linux/debian deriv
sudo apt-get install git wget flex bison gperf python3 python3-pip python3-venv cmake ninja-build ccache libffi-dev libssl-dev dfu-util libusb-1.0-0
## macos
brew install cmake ninja dfu-util

mkdir -p ~/esp
cd ~/esp
git clone -b v5.5.1 --recursive https://github.com/espressif/esp-idf.git
cd ~/esp/esp-idf
./install.sh esp32s3
. $HOME/esp/esp-idf/export.sh
echo "alias get_idf='. $HOME/esp/esp-idf/export.sh'" >> ~/.bashrc # or .zshrc

# espup
cargo install espup --locked
```

### Building

The `Makefile` has scripts for building everything -- run `make help` to see all options.

You probably just want to run one of these:

- `make flash` to re-flash the esp
  - `make wipe` to re-flash and erase persistent data (keys)
- `make serve` to build and serve the demo webui (includes wasm build)
- `make cli` to build the CLI tool `siger-cli`