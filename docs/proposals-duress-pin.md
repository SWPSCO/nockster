# Proposal: Duress PIN / Decoy Wallets

> **Status: Phase 1 (Option A, silent-reset) shipped.** A duress PIN set by
> double-entry on the device wipes + reboots on a coerced unlock — see the
> roadmap "Shipped" list and `SetDuressPin`/`ClearDuressPin`/`GetDuressStatus`,
> the `DURESS_*` NVS sector in `nvs_store`, and the `Duress` unlock outcome.
> Phase 2 (Option C, deniable decoy) remains as specified below, still gated on
> a flash-mock test harness for the seed-store rework.

A second PIN that unlocks a **decoy** wallet set while the real wallets stay
hidden — the device-appropriate defense against a coerced unlock ("give me your
PIN"). Specified here rather than built because, like raw-coil import, it
touches the **encrypted seed store**, the highest-blast-radius subsystem in the
firmware: a mistake there is the difference between "wrong screen shown"
(visible, fail-safe) and a corrupted or cross-contaminated seed store. It wants
a dedicated session with a test corpus, not the tail of a feature marathon.

This follows the structure of [proposals-raw-coil-and-policy.md](./proposals-raw-coil-and-policy.md).

## Threat model — be precise about what this defends

The defended threat is **coerced UI unlock**: an adversary in physical
possession of the device compels the user to unlock it through the normal touch
PIN entry. The user enters the **duress PIN**; the device unlocks a believable
decoy wallet (small, real balance) and gives **no UI signal** that other
wallets exist. The user keeps the real funds.

What it explicitly does **not** defend against, and the doc must say so on the
device and in user docs:

- **Flash forensics with the plaintext NVS.** On a dev build (no flash
  encryption) an attacker who desolders flash and reads NVS can see the total
  slot count and ciphertext blobs. The *strong* deniability property — "you
  cannot prove from the flash image how many wallets exist" — only holds with
  **production flash encryption on** (see [provisioning.md](./provisioning.md)),
  which is the intended production posture anyway.
- **A coerced attacker who knows the feature exists** and demands "the other
  PIN." Plausible deniability is a real but bounded defense; it raises the cost,
  it is not magic. The decoy must look lived-in (a real balance, some history)
  to be believable.
- **Rubber-hose against a cooperative user.** Nothing technical defends a user
  who reveals the real PIN.

Design target: **deniability against a flash dump (with encryption on) and
against the device UI**, plus a believable decoy. Not: deniability against an
attacker with the flash-encryption key *and* foreknowledge.

## Why it isn't a one-liner

Today the seed store is single-profile:

- One `Header` with one `salt` and one `slot_count`.
- `master_key = KDF(pin, salt, pepper)` (PBKDF2-HMAC v1, or v2 with the HMAC-UP
  hardware pepper).
- Every slot's 64-byte plaintext is AES-GCM-encrypted under `master_key`.
- `unlock` derives one key and decrypts **all** `slot_count` slots; any AEAD
  failure ⇒ `WrongPin`.
- `verify_master_key` decrypts **slot 0** specifically.
- `change_pin` re-derives a new key and **re-encrypts every slot** under it.

A second PIN that selects a different slot subset breaks every one of those
assumptions: "decrypt all slots, any failure = wrong PIN", "slot 0 is the
canonical verifier", and "change-PIN re-encrypts everything I can read."

## Design options

### (A) Duress = wipe (cheapest, weakest)

Register a duress PIN that, when entered, silently `factory_reset()`s instead
of unlocking (optionally showing a fake "empty wallet").

- **Pro:** tiny, contained change; no new storage layout.
- **Con:** destroys funds (no decoy to show); a savvy attacker who sees an
  empty/reset wallet is not satisfied and escalates. Weak deniability.
- **Verdict:** ship-able as a *coarse* option behind a setting, but it is not
  the decoy-wallet feature users picture.

### (B) Two independent regions (clean, but space-constrained)

Partition seed storage into region **R** (real) and **D** (decoy), each with
its own header/salt/slots/attempt-counter. The real PIN's key decrypts R; the
duress PIN's key decrypts D. Cryptographically independent; conceptually
simple.

- **Pro:** no change to the per-slot crypto or the "decrypt all my slots" loop
  — each region runs today's logic unchanged. Lowest *logic* risk.
- **Con:** **space.** The `nvs` partition is `0x9000..0x9000+28K` and the
  current layout (`SEED_CORE`, address book, attempt marks, vault) already
  consumes most of it. A second seed region likely needs a larger `nvs`
  partition → repartition → **wipes all existing devices**. Also two
  attempt-counters and a second vault to reason about.
- **Verdict:** the design I would pick **if** we are willing to grow the `nvs`
  partition in a release that already wipes (e.g. alongside a schema bump), and
  accept that region D's mere existence is visible in a plaintext dump (fine
  under flash encryption).

### (C) Single region, AEAD-tag profile selection (true deniability, most invasive)

Keep one region. Each slot is encrypted under the key of **whichever profile
owns it**. `unlock(pin)` derives that PIN's key and, for each slot record,
**attempts** AES-GCM decryption: slots whose tag authenticates belong to this
profile; slots that fail are "not mine" (another profile, or noise) and are
skipped rather than treated as `WrongPin`. From a flash image you cannot tell
how many profiles exist — the AEAD tag *is* the selector, and there is no
plaintext profile marker to leak.

- **Pro:** genuine plausible deniability even against someone reading
  plaintext NVS; no second partition; no fixed cap on the number of profiles.
- **Con:** the invasive option. It rewrites the core invariants:
  - **`unlock` loop:** "any AEAD failure ⇒ WrongPin" becomes "collect the
    slots that authenticate; require ≥1; zero authenticating slots ⇒ WrongPin."
    Must stay constant-time-ish so timing doesn't reveal how many slots matched.
  - **`verify_master_key`:** can no longer assume slot 0 is yours. Verify
    against "the first slot that authenticates under this key."
  - **`slot_count`:** now counts *all* profiles' slots. The header leaks the
    total (acceptable — it does not say which belong to whom), but the UI must
    show only *this* profile's slots.
  - **`change_pin` (the hard part):** today it decrypts every slot and
    re-encrypts under the new key. It must instead re-encrypt **only the
    current profile's** slots and carry the other profile's opaque,
    un-decryptable slot records through **verbatim**. Get this wrong and a PIN
    change silently destroys the decoy (or the real) wallet.
  - **Vault:** the preimage vault key is wrapped under one master key. A decoy
    profile needs either its own wrapped vault key or no vault — decide
    explicitly; don't let one profile's PIN change unwrap the other's vault.
  - **Registration:** a flow to "set a duress PIN and add decoy seeds under it"
    without ever exposing, via timing or counters, that a second profile was
    created.
- **Verdict:** the *right* end state for strong deniability, and the one worth
  the dedicated session. Build behind a schema bump with a fuzzed,
  golden-vector test corpus before it ships.

## Recommendation

Two-phase:

1. **Phase 1 — Option (A) behind a setting.** A "duress wipe PIN" is small,
   contained, and immediately useful for the destroy-on-coercion use case. It
   gives users *something* while (C) is designed properly. Implement as: an
   extra registered PIN whose verifier, on match, routes to `factory_reset()`
   instead of unlock; never reveal via error timing whether the entered PIN was
   the duress one vs. simply wrong.

2. **Phase 2 — Option (C) in a dedicated session.** The deniable decoy wallet,
   with the `unlock`/`verify`/`change_pin`/vault rework above, a schema bump,
   and a test corpus covering: real-only, decoy-only, both-present; PIN change
   on each profile preserving the other; delete within a profile; attempt
   counter shared correctly; and a constant-time check that match-count is not
   observable. Pair it with the production **flash-encryption** posture, since
   that is what upgrades (C) from "UI deniability" to "flash-dump deniability."

Do **not** attempt (C) without the test corpus. The failure mode is silent
cross-profile seed-store corruption, exactly the class of bug the team gates
behind a dedicated session for raw-coil.

## Estimated surface (Phase 2 / Option C)

`nvs_store` unlock + verify + change-PIN rework (~200 lines, the risky core),
duress-PIN registration flow (~80), `session` profile-awareness so the UI shows
only the active profile's slots (~60), vault keying decision (~40), protocol +
GUI for "set duress PIN" (~120), and the test corpus (~300). One firmware
compile pass; the work is correctness and tests, not volume. Roughly one
focused session — the same shape as the raw-coil estimate, and for the same
reason it should be its own.

## Open questions

- **Decoy believability.** A convincing decoy needs a small real balance and
  ideally some history. Document a setup ritual ("fund the decoy lightly") —
  the feature is only as good as the decoy's plausibility.
- **Attempt counter sharing.** One shared lockout counter across profiles is
  simplest and avoids leaking profile existence through differing counters;
  confirm that is the desired UX (a wrong duress PIN burns a real-PIN attempt
  too).
- **Interaction with Shamir/zprv backups.** A decoy profile's coil can be
  backed up with the same tooling; make sure the backup UX never accidentally
  reveals the real profile when the user is operating the decoy.
