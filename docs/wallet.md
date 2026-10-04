# Nockster wallet

Nockster is a Nockchain wallet for Chrome, Linux, macOS, Windows, iOS, and Android.
The apps share a Rust/WASM signing core.

- [Desktop app](desktop.md): setup, encrypted backups, and desktop releases.
- [Mobile apps](mobile.md): iOS and Android builds and device unlock.
- [Local testing](local-testing.md): developer setup and tests.
- [Releases](releases.md): signing, GitHub Releases, TestFlight, and store uploads.

## Build and run

Requires Node.js 22.22.2+ on the 22.x line, npm 12.1.0, Rust `nightly-2025-02-14`, the
`wasm32-unknown-unknown` target for that toolchain, and `wasm-pack`.

```sh
rustup toolchain install nightly-2025-02-14 --profile minimal --target wasm32-unknown-unknown
cargo +stable install wasm-pack --version 0.15.0 --locked
npm install --global npm@12.1.0 --ignore-scripts
npm ci
npm run build
```

Load `apps/extension/ext/` as an unpacked extension at `chrome://extensions`.
`npm run dev:ext` serves the development UI. `npm run build:zip` packages the
production extension. [Extension signing and CI](extension-release.md) describes
signed CRX and ZIP artifacts.

[Dependency policy](dependencies.md) covers the fourteen-day update delay,
security patches, disabled dependency install scripts, and audit commands.

RPC settings are `VITE_RPC_URL` and `VITE_RPC_V1_URL`. The default gateway is
`https://nockblocks.com/rpc`, with `/v1` for the v1 API. Address sign-in obtains an encrypted API key. See [Wallet services](wallet-services.md) for account linking, shared aliases, and transaction submission.
Custom extension RPC hosts also need host permissions in
`scripts/generate-manifest.cjs`. Wallet authentication uses `packages/wallet/src/platform/rpcAuth.ts`.

## Wallet operations

- Create an encrypted vault; import a 24-word mnemonic or extended private key;
  lock, unlock, export, rename, and manage wallets.
- Receive funds and track balances, notes, history, watch-only addresses, and
  pending transactions.
- Send v1 single-signer payments to one or more addresses. Max subtracts the
  calculated fee. Amounts use integer nick arithmetic (65,536 nicks per NOCK).
- Select spendable notes, including mature coinbase notes and disclosed timelocks.
  Coin selection checks the owner and current chain height.
- Create Base bridge deposits with the canonical bridge lock and destination
  note data. The minimum is 100,000 NOCK; the bridge charges 0.3% separately
  from the network fee.
- Omit output lock disclosures for private payments. Input note data is preserved
  because it is part of the note hash.
- Inspect and sign v1 transaction files, including multisig drafts and full
  Merkle proofs. Inspection computes the transaction ID, reads fees from spends,
  and checks signatures. Chain inclusion remains authoritative for acceptance.
- Compose hardware drafts with the same planner used for software wallets.
  Signed hardware responses must preserve the draft and contain valid signatures.
- Pair Nockster seed slots as individual wallets through WebHID, read device
  labels, unlock or lock the device, and verify receiving addresses on its screen.
  Signing resolves the root-key slot from the device's current public keys.

The signing core pins tx-types to `e33d5b46f630127a48633169b75795c1c38a25b5`.
It uses Bythos fee accounting and full lock proofs. Each output seed binds to
its complete input-note hash, as required by Logos. Each input balances its
outputs and fee independently.

The payment composer handles single-signer v1 inputs. Creating multisig locks,
constructing arbitrary scripts, and sweeping v0 notes use the Nockchain CLI;
Nockster signs imported v1 multisig drafts. Hardware transport needs a compatible
connected device and uses `@swps/nockster-js` 1.0.0. Hardware payments use root
keys and disclosed output locks. There is no dependency on a sibling hardware
WASM checkout. The firmware signing fixture in `tests/fixtures/nockster-signed.json`
records its `nockster-core` source revision and uses synthetic notes and a public
test key; the WASM tests verify the complete compose/sign/verify round-trip.

Encrypted vault data remains the source of signing keys. Wallet metadata and
signed pending transactions persist locally. A submitted transaction remains
reserved when an RPC response is lost, so it can be retried using the same signed
payload.

## Checks

```sh
npm run build
npm run typecheck
npm test
npm run test:core
```

`npm test` includes a test against the generated WASM bundle, so build it first.
Tests cover integer amounts, vault key preservation, input conservation, fee
convergence, Max, duplicate outputs, bridge splitting, coinbase maturity,
parent-note binding, and signed-draft verification. Tests do not broadcast funds.

## Dapp handoff links

A content script opens the wallet UI and prefills a payment:

```html
<a href="web+nockster://send?to=ADDRESS&amount=1.25">Pay with Nockster</a>
```

Repeat `to`/`amount` pairs for multiple recipients. Handoff requests open a review
flow; vault and RPC operations require the extension's own UI.
