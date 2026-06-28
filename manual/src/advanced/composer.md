# Transaction composer

The composer builds V1 transaction drafts in the browser and can hand them directly to the connected Nockster for signing.

## Quick send

Use **Send / Sign** for a simple payment:

1. Connect and unlock the device.
2. Save a Nockblocks API key if you want the composer to sync notes.
3. Import wallet PKHs from the connected device.
4. Sync wallet notes.
5. Pick a recipient or paste a PKH.
6. Enter an amount.
7. Use **compose + sign**.
8. Approve on the Nockster screen.

## Address book and notes

The composer keeps a browser-local address book. Entries can be plain PKHs or multisig locks. Notes can be synced from Nockblocks or entered manually by note name, origin page, and assets.

## Canvas mode

Drag address and note nodes into the canvas to build a transaction. Connect inputs to the transaction node and connect the transaction node to outputs. Output locks can be plain, timelock, hashlock, burn, or HTLC claim/refund structures.

## Upload and sign

The composer can also preview an uploaded `.jam`, `.draft`, `.psnt`, or `.wallet` file. If it is a supported V1 draft and the connected device can sign it, use **sign on device**.

## Multisig

The composer can combine two partially signed copies of the same multisig transaction into one combined `.psnt` with **merge & download**.
