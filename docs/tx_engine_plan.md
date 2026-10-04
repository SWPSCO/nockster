## Transaction Engine Alignment Plan

### Current Findings

- The WASM entrypoint now normalizes incoming note payloads and feeds them through the tolerant `RpcNote` struct, so `create_and_sign_tx` accepts the same shapes the Hoon wallet produces.
- `build_transaction_from_api` still returns the raw `base64` jam of a `RawTransactionV1`. We need to wrap this into the wallet-specific envelope (`{name, spends}`) that `nicker wrap` and `nockchain-wallet show-tx` expect, so the upstream wallet can open files we generate without an extra CLI step.
- Fee calculation diverges by ~5 “words” (e.g., 41.5 vs 40.5 nocks on the provided fixtures). While we can temporarily overpay, a precise fix requires mirroring `num-of-leaves:shape` from `tx-engine-1.hoon`.
- There is no automated test that asserts we emit wallet-compatible transactions—a current manual gap.

### Target behavior

1. **Wallet-compatible serialization**
   - Produce the same `WalletTransaction` structure (`{name, spends}`) that `nicker wrap` yields and base64 encode it, so consumers can call `nw show-tx` directly on our output.
   - Ensure the spends map ordering matches the canonical z-map rules (gor-tip/mor-tip rotations) so the jam is identical to the Hoon output.

2. **Fixture-based tests**
   - Add a test that uses the provided wallet seed and mock note data to build a transaction, encode it as a wallet transaction, and verify the jam bytes match the server-produced `good.tx` fixture.
   - Add a second test that loads the `.wallet` sample (`4EX3WV...tx`), decodes it, and re-encodes it via our Rust path to assert round-trip fidelity.
   - Future enhancement: run the wallet output through `nockchain-wallet show-tx` in CI (requires tooling) once deterministic.

3. **Fee parity (next phase after serialization)**
   - Mirror `count-seed-words` and `count-witness-words` from Hoon, including shape traversal, so our fee calculation converges on the same minimum. This can be validated by comparing word counts on fixtures before touching wallet serialization if needed.

### Implementation plan

1. **WalletTransaction type**
   - Recreate the `WalletTransaction { name: String, spends: SpendsV1 }` struct (`#[derive(NounEncode, NounDecode]`) in Rust.
   - Change `build_transaction_from_api` to return this structure’s base64 jam (optionally keep raw tx access for debugging).
   - Ensure we maintain a path to extract fee/tx-id data for the UI.

2. **Test harness**
   - Use `RawTransactionV1::from_noun` + `WalletTransaction` to decode the server fixtures and assert our output matches exactly.
   - For deterministic wallet output, apply the same note/mock recipients from the fixture and assert byte-level equality.
   - Document the fixture seeds/recipients directly in the test for reproducibility.

3. **Fee alignment (post-serialization)**
   - Instrument the tests to print word counts (witness vs seed) to compare against Hoon logs.
   - Update `count_seed_words` / `count_witness_words` to follow Hoon’s `num-of-leaves:shape` logic using the z-map shape and witness structure rather than jam length.

### Next steps

1. Implement the wallet transaction wrapper and change `create_and_sign_tx` to return a `.wallet`-compatible base64 jam.
2. Add the fixture-based tests for wallet transactions.
3. Once serialization is correct, revisit the fee code to match the Hoon implementation exactly.
