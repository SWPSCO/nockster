# About Nockster

![Nockster hardware wallet](../static/nockster.png)

Nockster is a small touchscreen hardware wallet for Nockchain. It stores your wallet keys on the device, shows you what a signing request is asking for, and signs only after you approve on the device screen.

Most people will use Nockster from the web app at [my.nockster.com](https://my.nockster.com). You plug the device into your computer, connect from the browser, unlock with your PIN, and use the device tab to manage wallet slots, check balances, update firmware, and sign transactions.  You can also download a local version of the app from the [Github releases](https://github.com/SWPSCO/nockster/releases).

## What Nockster protects

- Your wallet seed or imported private key material is stored on the device.
- Signing requests are reviewed on the touchscreen before approval.
- Private keys are not exported during normal signing.
- Firmware updates are signed and checked by the device before activation.

Nockster is not a backup device. If you lose the device and do not have your seed phrase, key export, or other recovery material, you can lose access to funds. Write down your recovery material and store it somewhere safe before using the wallet with real value.

## What you use with it

- The Nockster device and a USB data cable.
- A computer running Chrome, Edge, Opera, or the Nockster desktop app.
- The Nockster web app for everyday use.
- Optional: a Nockblocks API key if you want the app to show balances and notes.

## Main jobs

- Create or import the first wallet seed.
- Unlock and lock the device.
- Select wallet slots and verify receive addresses.
- Review and sign unsigned transaction drafts.
- Install signed firmware updates.

Advanced tools include a transaction composer, watch-only key export for `nockchain-wallet`, Shamir backup helpers, and the `%hax` preimage vault.
