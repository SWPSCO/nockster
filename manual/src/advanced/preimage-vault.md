# Preimage vault

The preimage vault is encrypted on-device storage for small Nockchain `%hax` lock preimages, such as HTLC secrets or commit-reveal values.

## What it stores

- Up to 8 preimages.
- Each preimage is a jammed noun up to 320 bytes.
- The device computes the Tip5 commitment itself.
- Store, reveal, and delete require on-screen confirmation.

Commitments and labels are visible metadata. The preimage bytes are encrypted.

## Use the vault in the app

1. Connect and unlock Nockster.
2. Open **Wallet** and choose the **vault** tab.
3. Click **load** or **refresh**.
4. Enter a label and secret bytes as hex.
5. Leave the jammed-noun checkbox off for ordinary bytes, or turn it on if the input is already a jammed noun.
6. Click **store** and confirm on the device.

To reveal or delete an entry, use **reveal** or **delete** and confirm on the device.

## Important caveat

The vault is not a backup. Keep an independent copy of any preimage whose loss could strand funds.

CLI:

```sh
nockster-cli vault list
nockster-cli vault store --label htlc-1 --hex deadbeef
nockster-cli vault reveal 2 --out preimage.jam
nockster-cli vault delete 2
```
