# Dice seed derivation

Nockster's dice mode creates a standard 24-word English BIP-39 mnemonic from
exactly 100 chronological d6 results. The UI collects them as 25 throws of
four distinguishable dice labeled A, B, C, and D. For each throw, enter the
four results in `A B C D` order using the six large face buttons. This means
25 physical throws and 100 face entries. Invalid faces and incomplete
transcripts are rejected.

The 32-byte BIP-39 entropy value is:

```text
SHA-256(
  "nockster/diceware/v1\0"
  || 0x64
  || byte(roll_1)
  || ...
  || byte(roll_100)
)
```

Each roll is encoded as one binary byte from `0x01` through `0x06`; `0x64` is
the binary roll count (100). No hardware-RNG output is mixed into this value,
so the same roll transcript always reproduces the same mnemonic. The normal
BIP-39 checksum is then appended and the resulting 264 bits are mapped to 24
words using the standard English list.

One hundred fair d6 rolls contain approximately 258.5 bits of source entropy.
Use fair physical dice with fixed labels or colors, preserve `A B C D` order,
and resolve a cocked or ambiguous die before entering that throw. Do not sort
otherwise indistinguishable dice after seeing the results; sorting discards
entropy. The device shows the complete mnemonic and then checks three randomly
selected word positions. The backup-check randomness does not affect seed
derivation.
