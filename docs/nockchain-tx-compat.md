# Nockchain v1 Transaction Compatibility

Audit of nockster's transaction model against canonical nockchain
(`../nockchain`: `hoon/common/tx-engine-1.hoon`,
`crates/nockchain-types/src/tx_engine/v1/`). Goal: confirm spiritual
correctness and enumerate any tx features we don't yet cover.

## Verdict: model is correct; primitives are exhaustive

- **Lock spend-condition primitives** — `tx-engine-1.hoon:1244-1249` /
  `tx.rs:776-781`: `$%([%pkh pkh] [%tim tim] [%hax hax] [%brn ~])`. We handle
  **all four**; there are no others.
- **Witness / spend / seed shapes** match: a v1 spend is `[%1 [witness seeds
  fee]]`; a witness is `[lock-merkle-proof pkh-signature hax tim]`; a seed is
  `[output-source lock-root note-data gift parent-hash]`. Our compose and
  review use exactly these.
- **Pkh** is `[m=@ hashes=(z-set hash)]` (m-of-n). **Tim** is
  `[rel=[min max] abs=[min max]]`, each bound `(unit page-number)`, all four
  optional, AND-checked (`rel` is relative to note origin, `abs` absolute
  block height). **Hax** is `(z-set hash)` of preimage commitments. **Brn** is
  unspendable. All match what we parse/display/build.
- **Multisig signature accumulation**: the witness `pkh-signature` is a z-map
  `pkh -> [pubkey signature]`; m distinct sigs from keys in the allowed set.
  Our device fills its slot and the composer seeds m placeholder slots — a
  `.psnt` accumulates sigs across co-signers. Correct.

## Important structural fact: a lock is a tree, not a list

`++lock` (`tx-engine-1.hoon:1373-1388` / `tx.rs:617-653`) is a **power-of-two
balanced binary tree of spend-conditions** — a leaf `spend-condition`, or
`[%2 ..]`/`[%4 ..]`/`[%8 ..]`/`[%16 ..]` OR-branches. Spending reveals **one**
leaf spend-condition + a merkle proof to the lock-root. A *spend-condition* is
itself an AND-list of primitives (`[%pkh ..] [%tim ..] ...`, all required).

Consequence for us, which is already handled correctly:
- The device verifies `hash(spend-condition) == lock-root` only for
  single-condition (leaf) locks → `verified = true`. For an OR-composed lock
  the witness carries one branch, so the root can't be reproduced from it →
  `verified = false` and the device shows **LOCK UNVERIFIED** (fail-safe).
- The composer builds single-condition locks (pkh, m-of-n, pkh+tim). It does
  **not** yet construct OR-composed (`%2/%4/%8/%16`) locks.

## What we now build/parse (post-overhaul)

- Compose: pkh, m-of-n multisig outputs; multisig **inputs** (spend-from-
  multisig with per-signer placeholder slots, change returns to the multisig);
  **timelocked** outputs (`[pkh, tim]` spend-condition, full rel/abs bounds).
- Review (device + host wasm `review_draft`): recipients, gifts, refund/change,
  bridge **deposit** EVM address, lock primitives (m-of-n, timelock bounds,
  hashlock count, burn), per-input multisig coordination. Host review tolerates
  legacy v0 spends (tag 0) mixed into a v1 tx.

## Additive features — status (compose layer, all tested)

| Feature | nockchain ref | Status |
|---|---|---|
| Multisig **inputs** (spend-from-m-of-n) | tx.rs:840-874 | **Done** — `source_multisig`; per-signer placeholder slots; change returns to multisig. |
| Timelock **output** (`%tim`) | tx.rs:876-880 | **Done** — `OutputInput.timelock` (full rel/abs bounds) → `[pkh, tim]`. |
| Hashlock **output** (`%hax`) | tx.rs:889-911 | **Done** — `OutputInput.hashlock` (commitments) → `[pkh, hax]`. Also **fixed** the lock-hashing stub that used a fixed placeholder instead of the commitment z-set. |
| Burn **output** (`%brn`) | — | **Done** — `OutputInput.burn` → `[%brn]`. |
| OR-composed locks `%2/%4/%8/%16` (HTLC) | tx.rs:617-653 | **Done + validated.** `OutputInput.or_branches` pads to power-of-two with `%brn` fillers, builds the tree, hashes per `Lock::hash`. Review surfaces `or_lock`. **Lock roots match nockchain's own golden vectors** — `or_composed_lock_roots_match_nockchain_vectors` reproduces the canonical timelock (`66FL…`), hashlock (`4kwz…`), `%2` (`e3qe…`), and `%4` (`6ezb…`) roots exactly; pkh/multisig leaves match too (`DKrg…`/`4eMAT…`). The OR-composed fund-safety gap is closed. |
| Bridge **withdrawal** (`bridge-w`) | note.rs:130-136 | **Done** (display) — review flags `bridge_withdrawal`. |
| Legacy v0 spend in review | tx.rs:300-305 | **Done** — host review tolerates v0 spends mixed into a v1 tx. Device still signs v1 only (we emit v1). |
| `%lock-root`-only outputs | tx.rs:178-194 | Already display the lock-root as recipient via seed fallback. |

All compose-layer additive features are implemented and unit-tested. The
remaining work is **GUI**: surfacing timelock/hashlock/burn/OR-branch
construction on output nodes, and multisig co-signing progress/merge.
