# nockchain-wallet Keyfile Interop

Nockster and the official `nockchain-wallet` CLI derive keys identically:
BIP39 mnemonic → PBKDF2-HMAC-SHA512 (2048 rounds, `"mnemonic" + passphrase`)
→ 64-byte seed → SLIP10-over-Cheetah master key + chain code (both sides use
the shared `tx-types` implementation / its Hoon counterpart). That makes two
interop flows possible without any new trust assumptions.

## Storage format: coil-only

Device slots store the nockchain-wallet's native **Cheetah master coil**
(`sk || cc`), not a BIP39 seed. A mnemonic is converted to a coil once at
ingest (`master_from_seed`); child keys derive straight from `(sk, cc)` with
no per-signature SLIP10 master step. Derived addresses are identical to the
old seed-based path (the master `sk` is the same), so this is an internal
storage/perf change — but the on-disk slot format changed, so a device must be
wiped and re-seeded once. Consequences:

- A raw coil (no seed) is now storable as-is, so importing a wallet's `%prv`
  coil is a natural next step (needs the request wiring + one verified
  byte-order vector — see proposals doc).
- The legacy secp256k1/bip32 features (`GetXpub`, `GetPubkey`, `SignDigest`,
  `GetFingerprint`) are unsupported on coil slots and now return an error;
  they were never part of the nockchain (Cheetah) path.

## Importing `keys.export` into Nockster

`nockchain-wallet export-keys` writes a jammed `(list [trek meta])` where the
wallet's master entry usually includes a `%seed` element holding the literal
mnemonic. The web UI's seed form ("import it" link) parses the file in WASM
(`parse_wallet_keyfile`), and:

- If a seed phrase is present, it fills the seed form — review it and load it
  through the normal add-seed flow (the device confirms as usual). The same
  phrase yields the same master key as the CLI wallet.
- If the file contains only derived key material (`%prv`/`%pub` coils, no
  `%seed`), import is refused with an explanation: Nockster slots store BIP39
  seeds, not raw master keys, so the original phrase is needed. (Raw-coil
  import would require a new slot type — listed as a possible future path.)

The parser never sends the file anywhere; it runs locally and only the seed
phrase enters the existing flow.

From the CLI: `nockster-cli seed --keyfile keys.export --pin 1234` resolves
the file to its seed phrase and continues through the normal seed flow.

## Exporting a watch-only keyfile from Nockster

Per wallet slot, the device tab offers **export watch-only**. The device
returns the slot's master public key and chain code after on-screen
confirmation (`GetMasterPubkey`, feature bit `FEATURE_MASTER_PUBKEY_EXPORT` —
confirmed because it reveals the slot's unhardened address tree). The host
wraps it as the wallet's expected jammed coil
(`[%coil [%1 [[%pub p=@] cc=@]]]`) and downloads `master-pubkey-<label>.export`.

The CLI equivalent:

```
nockster-cli export-master-pubkey --slot 0 --out master-pubkey.export
```

Import it on the nockchain-wallet side with:

```
nockchain-wallet import-master-pubkey --file master-pubkey-<label>.export
```

The CLI wallet can then watch the slot's master address (and derive unhardened
child public keys) with no private material on the computer.

### Compatibility note

Watching the **master address itself** (the address Nockster shows per slot)
is exact. Watch-only **child** derivation requires unhardened-CKD parity
across implementations; the historical divergences (std `ExtendedKey`
unhardened input/retry, and the no_std public-derivation serialization) are
fixed and pinned at
[`SWPSCO/tx-types@9cc0526`](https://github.com/SWPSCO/tx-types/commit/9cc052650110218543c2acb80d318f0ec497f87f)
(branch `fix/std-slip10-hoon-parity`), which this repo's git pins point at.
That commit's `slip10_hoon_parity` test asserts identical vectors for the std
and no_std paths, so drift fails CI. Hardened paths still cannot be followed
from a public key, by design.
