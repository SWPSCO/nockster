# Siger ESP

Hardware wallet firmware, host tooling, and web front-end for signing Nockchain transactions on [ESP32-S3](ESP32-S3-Touch-LCD-1.47)

## Repo layout
- `crates/siger-core` — shared logic for nouns, hashing, and transaction plumbing used by every other crate
- `crates/siger-cli` — desktop binary for seeding devices, signing drafts, and general maintenance
- `crates/siger-fw` — ESP32 firmware; talks WebUSB/WebSerial and persists encrypted seed material in NVS
- `crates/siger-wasm` — wasm build of the transaction toolchain used by the browser client
- `siger-js` — thin TypeScript wrapper around the device protocol, consumed by the web app
- `web` — Vite/React interface to drive the device in-browser (load draft, sign, download final `.tx`)
- `tx-types` — transaction definitions, hashing, and Nock stack machinery shared across firmware, CLI, and wasm

## Device protocol
- Transport is plain USB CDC with COBS framing; every message is a postcard-encoded `Request` or `Response` enum from `siger-core`.
- Typical flow: `Hello` → `GetInfo`/`GetLockStatus` → `Unlock { pin }` → `GetCheetahPub { path }` → `SignSpendHash { path, msg5 }`.
- Error handling mirrors the CLI: firmware replies with `Response::Err { code }` and the host decides whether to retry or surface it.
- The JS wrapper in `siger-js` and the Rust CLI both share the same codec, so debugging on desktop transfers directly to the browser.

## Flash layout
- Bootloader lives at `0x0` (32 KB) followed by the partition table at `0x8000`.
- NVS starts at `0x9000` (28 KB slot, ~24 KB effectively used) and stores the encrypted seed, salt, nonce, and attempt counter.
- Firmware image is placed at `0x10000` with a 3 MB ceiling.
- `make flash` updates firmware without touching NVS; use `make flash-wipe` when you really need a factory reset.

#### Partitions

| Address | Size  | Type       | Purpose                        |
|---------|-------|------------|--------------------------------|
| 0x0     | 32KB  | Bootloader | First-stage bootloader         |
| 0x8000  | —     | Partition  | Partition table                |
| 0x9000  | 28KB  | NVS        | Encrypted seed storage (PIN)   |
| 0x10000 | 3MB   | APP        | Firmware binary                |

## Commands
- Provision from scratch: `make flash-wipe`, then `siger-cli seed --port /dev/ttyACM0 --mnemonic "..." --pin 1234`.
- Firmware update: `make flash` (seed stays put).
- Sanity check after flashing: `siger-cli info --port /dev/ttyACM0` followed by a lock/unlock cycle to confirm the seed loads from NVS.
- Other shortcuts:
  - `make fw` builds the ESP32-S3 image, `make monitor` tails the serial console.
  - `make wasm` rebuilds the web bundle (`crates/siger-wasm/pkg`) for the browser client.
  - `make cli` and `make core` keep the host tooling and shared library honest between firmware iterations.

## Notes
- NVS data is AES-256-GCM encrypted with a key derived from the PIN; dumping flash without the PIN is useless.
- The wasm app deserializes the draft jam, sends the values over the wire for signature, recomputes `tx_id`, jams the signed noun, and hands back a consumable `.tx`.
