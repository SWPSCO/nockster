# Preimage Vault

Encrypted on-device storage for Nockchain `%hax` lock preimages — HTLC
secrets, commit-reveal values, or any small noun whose Tip5 commitment is
published on-chain and whose plaintext must stay guarded until reveal time.

## What it does

- Stores up to **8 preimages**, each a jammed noun of at most **320 bytes**.
- The device **cues the noun and computes the Tip5 `hash-noun` commitment
  itself** — the exact value a `%hax` lock commits to
  (`tx-engine: =(h (hash-noun:hax pre))`). The host never dictates the
  commitment; the confirm screen shows the device-computed digest.
- **Store, reveal, and delete each require on-screen confirmation.** Listing
  (commitments, labels, lengths) does not — commitments are public on-chain
  by construction.
- All operations require the device to be unlocked.

## Storage design

One dedicated 4 KB NVS sector (`nvs_store::VAULT_ADDR`), independent of the
seed region:

- Preimages are encrypted AES-256-GCM under a random **vault key (VK)**
  generated on first use.
- The VK is wrapped (AES-256-GCM) under a key derived from the PIN-derived
  master key (`SHA256("nockster-vault-v1" || master_key)`), stored in the
  sector header. A **PIN change re-wraps only the VK** (48 bytes) — records
  are untouched; this happens automatically during the change-PIN commit.
- Commitments and labels are plaintext metadata; only preimage bytes are
  encrypted.
- `make wipe` / factory reset erases the vault along with the rest of NVS.

## Caveats

- **The vault is not a backup.** A store/delete rewrites the vault sector
  (erase-then-write); power loss in that window can lose the sector. Keep an
  independent copy of any preimage whose loss would strand funds (the host
  necessarily has the plaintext at store time). A/B sector journaling is a
  possible future hardening.
- A preimage must fit one protocol frame (320-byte jam cap). Typical HTLC
  secrets are 32–64 bytes; this is not a general blob store.
- Duplicate commitments are rejected.

## Using it

Web UI: the **vault tab** on the device tab's Wallet panel (slots /
addresses / vault) — load/refresh the list, store a secret (hex bytes are
wrapped as an atom noun, or check "already a jammed noun" to store arbitrary
nouns), reveal or delete with on-device confirmation. Revealed secrets can be
copied as hex or downloaded as `.jam`.

On-device: **Settings → Vault** lists the stored entries (nickname +
commitment base58, same layout as the wallet list). Tapping an entry opens
its detail screen with the full commitment and two actions: rename (same
multi-tap keypad as wallet nicknames) and delete (with its own confirm
screen). Preimages themselves are never displayed on the device; reveal goes
through the host with on-screen confirmation.

CLI:

```
nockster-cli vault list
nockster-cli vault store --label htlc-1 --hex deadbeef…   # or --file secret.bin, --jam for raw nouns
nockster-cli vault reveal 2 [--out preimage.jam]
nockster-cli vault delete 2
```

`vault store` prints the commitment before sending so it can be compared
against the device's confirm screen.

Protocol (nockster-js): `vaultList()`, `vaultStore(label, preimageJam)`,
`vaultReveal(slot)`, `vaultDelete(slot)`; helpers in nockster-wasm:
`jam_byte_atom`, `cue_byte_atom`, `noun_commitment_b58`, `tip5_limbs_b58`
(shared Rust logic in `nockster-core::wallet_keyfile`).
Feature bit: `FEATURE_PREIMAGE_VAULT`.
