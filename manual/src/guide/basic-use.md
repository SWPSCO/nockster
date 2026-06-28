# Basic use

## Unlock and lock

After connecting, the device console shows whether Nockster is locked or unlocked.

- Enter your PIN and click **unlock** to use wallet features.
- Click **lock** when you are done.
- Click **test** if you want to check that the device is responsive.
- Click **disconnect** before unplugging if you want a clean browser session.

## Wallet slots

Open the **Wallet** panel. Each slot represents one wallet seed or imported key.

Common actions:

- **select** chooses the slot used for signing.
- **nickname** lets you rename a slot.
- **copy** copies the receive address.
- **verify address** shows the receive address on the device screen so you can compare it before receiving funds.
- **sign message** and **sign hash** create device-approved signatures for non-transaction payloads.
- **export watch-only** downloads a public key file for `nockchain-wallet`.
- **remove** deletes that seed slot after confirmation.

Removing a slot cannot be undone unless you still have the seed phrase or private key material somewhere else.

## Balances and notes

The app can use Nockblocks to show balances and sync notes. Paste a Nockblocks API key when the app asks for one, then use **balances** or the composer note sync tools.

Your Nockblocks API key is stored in the browser. It is used to fetch public chain data for your addresses; it is not a wallet seed.

## Address book

The **addresses** tab stores labeled PKHs on the device. You can save your own wallet address or paste another recipient address. The composer can use these labels later.

## On-device settings

The unlocked device has a settings menu with:

- **Wallets**
- **Add Seed**
- **Vault**
- **Theme**
- **Calibrate**
- **Diagnostics**
- **About**

Use **Calibrate** if touches are landing in the wrong place. Use **About** to see firmware and release information.
