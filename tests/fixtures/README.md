# Signing fixture

`nockster-signed.json` contains a public mnemonic-derived wallet, its RPC note,
recipients, candidate height, unsigned wallet envelope, and a verified signed
envelope. Its fee comes from the shared planner at that candidate height.

Rebuild the wallet WASM, then regenerate it through the firmware signing entry
point:

```sh
bash scripts/build-wasm.sh
node scripts/update-signing-fixture.mjs
npm test
```

The generator verifies the complete signature before writing the fixture.
Independent Hoon byte/hash/proof/fee oracles live in the sibling `tx-types`
repository at `tx-types/tests/reference`. The desktop composer test compares
wallet and hardware raw JAM bytes at both sides of Bythos and at height 170000.
