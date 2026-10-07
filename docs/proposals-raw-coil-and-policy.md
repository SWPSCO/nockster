# Proposals: Raw-Coil Import & Nock Signing Policy

Two roadmap items that warrant a design sign-off before implementation. The
first touches the seed-encryption subsystem (high blast radius, needs one
verified compatibility vector); the second is a novel device-local feature
with no ecosystem counterpart (speculative by the "nothing consumes it" test),
so per request it is specified here with examples rather than built blind.

---

## 1. Raw-coil import

### Goal

Import a nockchain-wallet master key directly (the `%coil [%1 [[%prv sk] cc]]`
from `keys.export`) onto a device slot, with no BIP39 seed phrase — and make
the wallet's coil the canonical interchange format, with BIP39 import as one
way to *produce* a coil. Don't worry about migrating existing on-device
formats or wiping.

### Why it isn't a one-liner

A device slot today stores a 64-byte **BIP39 seed**, which feeds *two*
derivations:

- **Cheetah** (the nockchain path): `master_from_seed(seed)` → `(sk, cc)` →
  child keys. Used by `GetCheetahPub`, `SignSpendHash`, draft signing.
- **secp256k1 / bip32** (vestigial bitcoin-style): `XPrv::new(seed)` → xpub,
  `GetPubkey`, `SignDigest`, `GetFingerprint`.

A raw coil only carries the cheetah `(sk, cc)` — there is no seed, so the
secp256k1 path cannot exist for a coil slot. Two clean options:

- **(A) Per-slot kind flag (recommended).** Keep seed slots exactly as they
  are (both derivations work); add raw-coil slots that are cheetah-only. No
  feature is removed; existing tests/CLI unaffected.
- **(B) Coil-only storage.** Store `(sk, cc)` for every slot, deriving it once
  at import (from seed for BIP39). Uniform derivation, matches the wallet's
  format exactly, but **drops the secp256k1 features** (breaks `GetXpub` /
  `GetPubkey` / `SignDigest` / the CLI `derive`/`keys` paths). Only acceptable
  if those vestigial features are truly disposable.

Recommendation: **(A)**. It is additive and removes nothing. The rest of this
section assumes (A).

### Design (option A)

**Storage (`nvs_store`).** The slot record's flag region has spare padding
bytes (`buf[1..4]`, currently `0xFF`). Use `buf[1]` as a kind:
`0x00`/`0xFF` = seed (back-compatible with existing slots), `0x01` = raw coil.
The encrypted 64-byte plaintext is the seed for a seed slot, or `sk || cc`
(32+32) for a coil slot. The stored root pubkey (`pub_x`/`pub_y`) is derived
from the seed-master for a seed, or directly from `sk` for a coil. AES / PIN /
nonce / atomicity are **untouched** — only the plaintext interpretation and one
flag byte change.

**Decoupled kind loading (keeps the hot path clean).** The kind lives in the
*plaintext* flag, so it is readable without the PIN. Add
`NvsStore::read_slot_kinds() -> Vec<u8>`. After a successful unlock populates
the session seeds, set the kinds in one shot — no change to the unlock return
tuple, `UnlockOutcome`, or the worker-core job plumbing.

**Session (`session`).** Add a parallel `slot_kinds: HVec<u8, MAX_SEED_SLOTS>`
mirroring `slots`, kept in sync by the same mutators (`update_seed_store_from_slice`
+ `set_slot_kinds`, `append`, `remove`, `wipe`), plus `get_slot_kind(slot)`.
Seeds and kinds are always set together within a single unlock handler (one
core, no interleaving), so there is no desync window.

**Derivation (`seed_store`).** Branch on `session::get_slot_kind`:
- seed: `master_from_seed(blob)` (today's behavior).
- coil: `(blob[0..32], blob[32..64])` as `(sk, cc)` directly.
`root_pub_from_seed` and `record_pub_matches_seed` branch the same way.
secp256k1 entry points (`get_xpub`, `GetPubkey`, `SignDigest`) return
`ERR_UNSUPPORTED` for coil slots.

**Change-PIN rewrite.** `prepare_rewrite_seed_storage` already rebuilds slot
records; have it carry each slot's existing kind and re-derive the pubkey per
kind (or simply reuse the old record's stored pubkey + kind, which it already
reads).

**Protocol.** Two new requests, mirroring `AddSeed` / `InitializePIN`:
- `AddCheetahCoil { sk32: [u8;32], cc32: [u8;32] }` — add a coil slot to an
  unlocked device (on-screen confirm; derive pub from sk; encrypt `sk||cc`).
- `InitializeWithCoil { pin, sk32, cc32 }` — first-boot init with a coil.

**Host (`wallet_keyfile` + wasm + cli + web).** Extend `keys.export` parsing
to extract the master `%prv` coil's `sk` and `cc`. **Open compatibility
question:** the wallet stores `sk`/`cc` as `@ux` atoms; pokenoun reads atoms
little-endian, and the device wants `sk_be` big-endian — so the bytes are
almost certainly reversed, but I want **one verified vector** (a known
phrase → wallet `keys.export` → confirm the device derives the same cheetah
address) before shipping, exactly as we did for the SLIP10 parity fix. The CLI
gets `seed --keyfile … --as-coil` (import the coil directly instead of the
phrase) and `import-coil --sk … --cc …`.

### Estimated surface

`nvs_store` (~80 lines), `session` (~40), `seed_store` (~50), `jobs`/`main`
wiring for the two requests (~120), `nockster-core` lib + fixtures (~30),
`wallet_keyfile`/wasm/cli/web (~120). One firmware compile pass, plus the
verified import vector. ~1 focused session.

### Why a clean session, not now

It is the most security-critical subsystem; errors there are the difference
between "wrong address shown" (visible, fail-safe) and a corrupted seed store.
It deserves fresh attention and the verification vector, not the tail of a
marathon.

---

## 2. Nock signing policy (speculative — proposal only)

### What it is

A user- (or treasury-admin-) installed **policy noun**: a Nock formula
evaluated against the device's parsed `DraftReviewV1` *before* the confirm
screen. It returns allow / deny / "require extra confirmation," giving
programmable, on-device spending rules expressed in the chain's own language —
with no firmware update and nothing for the host to forge, because the policy
is a noun the device hashes, displays, and can attest.

It is **speculative** in the precise sense you flagged: no nockchain or
wallet feature consumes or expects it; we would be inventing a device-local
mechanism. That is why this is a proposal, not code.

### Why it's attractive anyway

- Nock is a fully-specified, 12-opcode language; a **bounded, step-limited**
  interpreter over the existing `pokenoun` arena fits the device.
- A policy is a noun → Tip5-hashable → displayable → attestable. Two co-signers
  can verify they run the *same* policy by comparing one hash.
- Sandboxing is clean: the formula's subject is the review data; it returns a
  verdict; it **cannot touch key material** (different code path entirely).

### Sketch

```
policy : gate  ::  [review-noun] -> verdict
verdict : ?(%allow %deny [%confirm note=@t])
```

The device builds a `review-noun` from `DraftReviewV1`:

```
[ external-total=@      ::  nicks to non-change recipients
  fee=@                 ::  nicks
  recipients=(list @)   ::  recipient pkh digests
  multisig=?            ::  any m-of-n input
  bridge=?              ::  any Base bridge output
  flags=@               ::  the review flag bitfield
]
```

Then `nock(review-noun, policy-formula)` under a step budget (e.g. 100k
reductions) and a memory cap; any fault, overrun, or non-verdict result =
`deny` (fail-closed).

### Examples

- **Spend ceiling:** deny if `external-total > 50 NOCK`.
  `|=(r=review ?:((gth external-total.r 50.000.000) %deny %allow))`
- **Recipient allowlist:** allow only three pkhs; else require extra confirm.
- **Fee guard:** deny if `fee > external-total / 10`.
- **No-bridge:** deny any output that leaves the chain (`bridge=&`).
- **Multisig-only:** require that every spend be from a multisig input.

The device shows `policy 6mhC…ep1F active` on the review screen; install /
clear / show via confirmed requests; the policy noun lives in the existing
encrypted NVS (or the preimage-vault sector pattern).

### Risks / open questions

- **Interpreter correctness & DoS.** Needs a hard step + memory budget and a
  fuzzed, golden-vector test suite before it gates real signing.
- **Jet-free Nock is slow.** Fine for tiny policy formulas; must reject large
  ones rather than hang (the watchdog is a backstop, not a design).
- **UX of authoring policies.** Users won't write Hoon; this implies a small
  set of host-side policy *templates* that compile to the noun, with the
  device only ever verifying the hash.
- **Is it wanted?** Since nothing in the ecosystem requires it, this is a
  product bet on "programmable cold-storage rules," most compelling for
  treasuries/multisig. Worth prototyping behind a feature flag; not worth
  wiring into the default signing path until proven.

### Recommendation

Prototype the bounded interpreter + `review-noun` + a couple of template
policies as an **opt-in** behind a build flag, with its own test corpus,
before it ever sits in front of a real confirm. Keep it out of the default
firmware until the interpreter has a fuzz suite.
