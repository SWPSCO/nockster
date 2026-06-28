# Signing transactions

Nockster signs unsigned Nockchain transaction drafts. The web app accepts `.draft`, `.wallet`, and `.psnt` files in the device tab.

## Sign a draft file

1. Connect Nockster.
2. Unlock it with your PIN.
3. In the **Wallet** panel, select the slot you want to spend from.
4. In **Transaction signing**, upload the unsigned draft.
5. Review the transaction details in the app.
6. Click **sign transaction**.
7. Review the transaction on the Nockster screen.
8. Hold **Approve** on the device to sign, or reject if anything looks wrong.
9. The browser downloads a signed `.tx` file.

Do not approve a transaction unless the device screen matches what you intend to do.

## What to check on the device

Check the recipient, amount, and any lock or refund warnings shown during review. The host computer can ask for a signature, but the device approval is the important step.

If the review shows an unexpected recipient, amount, lock, or fee, reject it and rebuild the draft.

## Signing from the composer

The **composer** tab can build V1 drafts and pass them directly to the connected device. After the composer creates a draft, use **sign on device** or **compose + sign**. The same on-device approval step applies.

## Signed output

The app downloads the signed transaction as a `.tx` file named from the transaction ID. Broadcast it using the Nockchain tooling or service you normally use.
