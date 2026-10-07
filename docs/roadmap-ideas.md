# Roadmap: Potential Paths Forward

Ideas surveyed against the Nockchain protocol (lock primitives, sighash
semantics, official wallet features) and the device's actual strengths: an
on-device noun codec (cue/jam), Tip5 hashing, encrypted NVS, signed OTA with
trust anchors, and a screen the user can trust. Recorded here so they don't
live only in chat logs. Roughly ordered by leverage within each group.

## Shipped from this list

- **Hold-to-confirm.** Trezor-style 3-second hold with a sweeping circular
  progress ring gates Confirm/TxReview approval and the wallet/vault
  delete-confirm screens (`advance_hold_confirm`).
- **zprv extended-key import.** Parse nockchain-wallet `zprv` (base58check,
  `nockster_core::extended_key`, vector-tested against a keygen sample) → raw
  master coil via the new `AddCoil` request. Firmware, wasm, web, and
  `nockster-cli seed --zprv`.
- **zpub watch-only export.** `export-master-pubkey` and the web watch-only
  export now also emit a `zpub` string for `nockchain-wallet import-extended`.
- **Host-side verify-message / verify-hash.** wasm Schnorr verify over the
  cheetah curve (`verify_message`/`verify_hash`/`sig_file_to_hex`) + a web
  "Verify signature" panel; the sign flows prefill it. A verifier you can
  trust is rarer than a signer.
- **Shamir seed backup.** The web Shamir panel splits the original 24-word
  BIP39 seed phrase into k-of-n 24-word BIP39 share phrases, and restores by
  recombining those share phrases before importing the recovered seed.
- **Duress PIN (silent reset).** A second PIN, set by double-entry on the
  device (`SetDuressPin`, worker-core KDF), stored as a verifier in its own NVS
  sector. Entering it at unlock makes `compute_unlock_outcome` return
  `Duress` → factory reset + reboot to a fresh device. The set flow rejects a
  duress PIN equal to the real one; status/clear require an unlocked device.
  `nockster-cli duress` + a web Control-panel section. This is the contained
  Option-A from [proposals-duress-pin.md](./proposals-duress-pin.md); the
  deniable-decoy variant (Option C) remains gated on a flash-mock test harness.
- **Preimage vault** (`%hax` secrets with device-computed commitments) — see
  [preimage-vault.md](./preimage-vault.md).
- **keys.export / master-pubkey.export interop** with the official wallet —
  see [wallet-keyfile-interop.md](./wallet-keyfile-interop.md).
- **Blind-signing warning.** `SignSpendHash`/`SignSpendHashFor` now show a
  `BLIND SIGN` / `WARNING blind sign?` review header with "Raw hash only" and
  no amounts; the verified on-device draft-review path remains the only
  "pretty" signing flow.
- **Address-book labels in TX review.** Recipient pkhs matching a device
  address-book entry render as the label (`trusted_recipient_display`).
- **Show-address / verify-receive flow.** `ShowAddress` request + device
  screen + `nockster-cli show-address` + web "verify on device" button.
- **Message / hash signing.** `SignMessage` (device hashes via nockchain
  `++page-msg`) and `SignHash`, Cheetah-schnorr signed after on-screen review
  of the message text. Wired across firmware, nockster-js (`signMessage`,
  `signHash`), `nockster-cli sign-message`, and a per-slot web button.
  `nockster_core::draft_sign::message_digest_v1` is the shared, wallet-
  compatible digest.
- **Verify-and-display output lock trees.** `draft_review_v1` parses each
  output's `note_data` "lock" spend-condition into typed primitives
  (`LockSummaryV1`/`LockPrimitiveV1`: m-of-n pkh, timelock min/max bounds
  reported verbatim with no chain-height assumption, hax, burn) and **verifies
  it hashes to the output's `lock_root`** (`lock_root == hash:spend-condition`
  for single-condition locks). The TX review screen flags `lock unverified`,
  `timelocked`, `hashlock`, and `multisig`, and prefixes multisig outputs with
  `MofN`.
- **Bridge-deposit display.** `note_data` "bridge" = `[%0 %base [a b c]]` is
  decoded to the EVM address (`a + b·p + c·p²`, p = Goldilocks); bridge
  outputs show `eth 0x…` and raise an `eth bridge out` warning.
- **Multisig signing + coordination.** The signer already fills only the
  device's slot of an m-of-n witness (respecting the m count, evicting fee-
  sizing placeholders). `draft_review_v1` now also surfaces per-input
  coordination (`MultisigInputV1`: m, n, real signatures present, whether the
  device is authorized / already signed); the TX review shows
  `MSIG present/m` and `+YOU` when the device still needs to sign.
- **Noun inspector (typed).** `inspect_noun` (wasm) cues any jam and renders a
  typed tree — atoms with number/hex/text/`%tag` heuristics, proper lists,
  improper cells, depth/size-budgeted — surfaced as a collapsible web panel
  for `.tx`/`.psnt`/`keys.export`/`.sig` files.
- **OTA stream resume.** `streamUpdateBundle` now checks `GetUpdateStatus`
  first; an active session whose verified manifest matches the bundle and is
  partway through is continued from `bytes_received` instead of re-beginning
  (the FinishUpdate full-image hash check still guards integrity).

## Proposed (awaiting sign-off)

- **Duress PIN / decoy wallets** — specified in
  [proposals-duress-pin.md](./proposals-duress-pin.md). Touches the encrypted
  seed store; phased (coarse duress-wipe first, deniable decoy in a dedicated
  session with a test corpus).
- **Production provisioning** is built (`scripts/provision/` + Makefile);
  the operator runbook is [provisioning.md](./provisioning.md).
- **Raw-coil import** and **Nock signing policy** — specified in
  [proposals-raw-coil-and-policy.md](./proposals-raw-coil-and-policy.md).
  Raw-coil touches the seed-encryption subsystem and needs one verified
  import vector; the policy engine is speculative (no ecosystem consumer) and
  proposed as an opt-in prototype.
- **Transaction composer overhaul.** Planned in detail in
  [composer-overhaul-plan.md](./composer-overhaul-plan.md): robustness +
  component extraction, multisig *construction* and `.psnt` co-signing,
  timelock/hashlock/burn outputs, a first-class witness/segwit model, and
  device-parity preview — keeping the theming. A dedicated frontend session.

## Security hardening (near-term, high value)

- **Backup verification quiz.** "What is word #7?" multiple-choice on the
  touchscreen — the device-appropriate analogue of the CLI's
  `show-seedphrase`.

## Protocol coverage
- **Message/hash signing with domain separation** (`sign-message` /
  `sign-hash` parity), with a distinct prompt and a prefix that can never
  collide with a spend sighash. On-device **verify-message** too — a verifier
  you can trust is rarer than a signer.
- **note_data warning.** Flag non-empty eUTXO datum on outputs during review;
  unusual for plain payments.
- **Bridge-deposit display.** Distinct review treatment for EVM-address
  outputs ("leaving the chain").
- **tx-accepted feedback.** After signing, let the host push confirmation
  status so the device screen itself shows "confirmed in block N"
  (on-demand, not polled).

## Noun-native features (the differentiator)

The device already ships cue/jam, Tip5-hash-noun, and z-map traversal; these
build on that rather than on new crypto:

- **Trusted noun inspector — ON-DEVICE.** The host-side typed inspector
  shipped (`inspect_noun`); the remaining idea is running the same typed
  render on the *device* screen (a `FragKind::Inspect`) so the tree is shown
  on the trusted display rather than the host.
- **Noun diff for multisig round-trips.** When a `.psnt` returns for
  signature, show "fee: 100 → 120; outputs unchanged; 1 signature added" by
  comparing subtree digests. Removes the scariest part of multisig
  coordination.
- **Signing policy as a Nock formula.** A bounded 12-opcode Nock interpreter
  evaluating an installed policy noun against the parsed draft review before
  the confirm screen (spend limits, recipient allowlists). Policies are
  nouns: hashable, displayable, attestable across co-signers' devices, no
  firmware update needed. Sandboxed — the formula sees the review, never key
  material.
- **Clear-signing plugins as signed nouns.** As nockapps put structured data
  in `note_data`, render it via vendor/community-signed Nock gates executed
  in the same sandboxed interpreter, verified against trust anchors exactly
  like firmware updates. No native-code plugin risk.
- **Selective disclosure via noun merkle-ization** (further out). Tip5-merkle
  a credential noun, publish the root, reveal subtree at axis N with a proof.
  Needs a verifier ecosystem.
- **Raw-coil import.** A slot type for master keys without seed phrases would
  complete keys.export import for phrase-less wallets.

## Dedicated-object roles (radios are off today; see caveat)

The ESP32-S3's WiFi/BLE are unused. Anything that turns them on must be
walled off from the signer — radio-off by default, networked modes as a
separate firmware profile or for seedless/watch-only devices.

- **Theft canary / watchtower.** Watch-only mode that alarms when a watched
  note is *spent* (consumed names are an unambiguous public signal). A phone
  app can't credibly be this; the malware that steals funds owns the phone.
- **Merchant payment terminal.** Device-derived receive address on screen,
  flips to "✓ PAID" when the note lands. Trust-minimized point-of-sale for
  ~$20 of hardware.
- **Inheritance / dead-man's switch.** `Tim` locks make "heir tx valid after
  block N" a protocol feature; the device's job is tracking the deadline and
  making the refresh a two-tap ritual.
- **Notary seal.** Host streams a file, device Tip5-hashes and signs a
  burn-locked output carrying the digest — an on-chain timestamp. (The
  enclosure could literally be a desk seal.)
- **BLE address bump.** Two Nocksters exchange address-book entries with
  confirmation on both screens; entries arrive device-verified into the
  trusted book.
- **Ceremony entropy object.** On-device seed generation mixing TRNG with
  user dice rolls, shown on screen — an auditable key ceremony.
- **Miner's companion.** Height/difficulty/coinbase-maturity dashboard;
  modest utility but meets the current user base where it is.

## Infrastructure follow-ups

- Vault A/B sector journaling (power-loss hardening; see preimage-vault.md).
- Explicit `ERR_PROTO_VERSION` response for frames with unknown protocol
  versions (today they are silently ignored).
- OTA stream resume ("resume from offset") for 3 MB images over HID.
