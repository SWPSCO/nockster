# Transaction Composer Overhaul — Plan

The web composer (`web/src/composer/`, ~3.1k lines, a React Flow node graph of
address / note / tx / preview nodes that builds v1 drafts via
`nockster-wasm::compose_tx_v1_*`) works but is due a robustness + capability
pass. Goal: more advanced and robust, better design, **keep the theming**, and
add segwit/witness + multisig construction. The device side already *verifies*
lock trees, *surfaces* multisig coordination, and *signs into* multisig slots,
so the composer can now safely *construct* the same shapes and preview exactly
what the device will show.

## Where it stands

- Node graph: `address`, `note`, `tx`, `preview` nodes; emits `.psnt`.
- Multisig **addresses** already build: `compose_tx_v1_recipient_address({m,
  pkhs})` → lock_root; `AddressKind = 'pkh' | 'multisig'`,
  `MultisigDescriptor { m, pkhs }`.
- Single recipient/amount per output; fee handling via the planner.

## Gaps / work items (suggested order)

1. **Robustness pass (do first).** *(File-split done; robustness TODO.)*
   `Composer.tsx` (~3k lines) is split: `model.ts` holds the data types and
   the pure, React-free helpers (formatting, amount parsing, summary/preview
   normalization, recipient/lock describers); `nodes.tsx` holds the four React
   Flow node components; `Composer.tsx` (~2.4k) is now wiring + JSX. Verbatim
   move, tsc-verified. Still TODO: centralize validation (amounts, pkh/lock
   formats, note selection, fee floor), surface compose errors inline per node
   rather than one status line, guard double-submit, and make wasm-not-ready /
   parse-failure states explicit — and further split the build/wallet/preview
   sidebar panels out of `Composer.tsx`.

> Status (post-overhaul): items 1 (file-split), 2 (multisig co-signing,
> compose side), 3 (timelock outputs, compose side), and 5 (device-parity
> preview) are done and tested. See [nockchain-tx-compat.md](./nockchain-tx-compat.md)
> for the canonical-source audit — our lock primitives are exhaustive/correct.
> Remaining: the timelock-output **UI** field, hashlock/burn output
> construction, OR-composed locks, and the item-1 robustness/panel-extraction.

2. **Multisig construction.** *(Compose side done + tested.)*
   `compose_tx_v1_unsigned` accepts `source_multisig {m, pkhs}`: inputs are
   reconstructed as the m-of-n lock, change returns to the multisig, and the
   witness is seeded with m placeholder signature slots so the `.psnt` round-
   trips through each co-signer's device (which fills its own slot). The
   composer passes this automatically when the source is a multisig address-
   book entry. The co-signing **merge** step is now in the composer ("Combine
   multisig signatures": upload two partially-signed copies → `merge_signed_tx`
   unions the per-input signature maps → download the combined `.psnt`).
   Structurally tested; confirm against a real device-signed multisig `.psnt`.
   Collection progress is shown by the imported review (present/m + you-can-sign).

3. **Timelocked / hashlock / burn / HTLC outputs.** *(Done — compose + GUI.)*
   Output address nodes have a lock selector — plain / timelock / hashlock /
   **HTLC (claim-or-refund)** / burn. HTLC builds a 2-branch `or_branches`
   (claim: recipient + preimage commitment; refund: address after a height).
   The typed inspector and review render all of these. OR-composed lock roots
   are **validated against nockchain's golden `%2`/`%4` vectors** (see
   [nockchain-tx-compat.md](./nockchain-tx-compat.md)) — the fund-safety gap is
   closed.

4. **Segwit / witness model — spending OR-composed inputs.** *(Protocol core
   done + validated.)* `compose_tx_v1_unsigned` accepts `source_or_lock
   { branches, spend_branch }`: it reconstructs the OR lock, verifies the note
   name, and builds a **full lock-merkle-proof witness** for the chosen branch
   (`[%full spend-condition axis [root path]]`) with placeholder slots for that
   branch's signers — exactly the HTLC claim/refund spend path. The merkle
   prover/verifier (`++prove-hashable-by-index`/`++verify-merk-proof`, incl.
   Hoon `++peg`) are mirrored and **validated**: `lock_merkle_proof_round_trips_
   to_canonical_root` proves every leaf's (axis, path) reconstructs the
   wallet-validated root, and `compose_spends_or_composed_input_branch` proves a
   composed spend's witness verifies end-to-end. **UI done (pragmatic):** the
   Send pane has a "Spend an HTLC-locked note" form — pick claim/refund, enter
   the note + the HTLC lock params (claim recipient/commitment, refund
   address/height) + the output, and it composes the branch spend (inspect /
   download / sign on device). Covers the common HTLC case; a fully general
   N-branch OR-input editor is a future nicety.

5. **Preview parity with the device.** *(Done.)* Composed txs: the Tx node
   renders a per-output breakdown with the lock badge (`p2pkh`,
   `N-of-M multisig`) + alias/address + amount, from the compose summary.
   Imported `.psnt`: `review_draft(jam, source_pkh)` (wasm) decodes the draft
   and surfaces outputs (recipient, gift, refund, bridge EVM address, lock
   primitives) + per-input multisig coordination, rendered as a "what the
   device will show" panel. The review parse lives in
   `nockster-wasm/src/review_v1.rs`, ported onto `tx_types::pokenoun` (the
   `nockster-core`-in-wasm path was a `tx-types` std/no_std feature-unification
   conflict). It is *structural only* — lock-root Tip5 verification stays on
   the device (a host preview can't be a trust anchor), so this view omits the
   `verified` flag by design. `nockster_core::draft_sign::draft_review_v1_for_pkh`
   is the device/CLI counterpart for review-by-address.

6. **Design.** Keep the monochrome theming; tighten node layout, inline
   validation affordances, and the build/sign/export flow. The
   `frontend-design` skill is appropriate here.

## Dependencies to expose from wasm

- `draft_review_v1` (typed review incl. lock summaries, multisig inputs,
  bridge) for preview parity.
- Per-primitive lock constructors (tim/hax/brn) alongside the existing
  multisig address builder.

## Why a dedicated session

It's a large, unfamiliar 3.1k-line frontend; doing it well means component
extraction + new wasm surface + UX design, which is its own focused effort
rather than a tail-end increment. Items 1–2 are the highest leverage and a
good first sitting.
