# First seed and PIN

Nockster needs one seed slot before it can sign. The first seed also creates the device PIN.

## Recommended: generate on the device

1. On the Nockster screen, choose **Generate New**.
2. Write down the 24 words exactly as shown.
3. Confirm the seed on the device.
4. When prompted, enter a PIN.
5. Enter the same PIN again at **Repeat PIN**.

Store the written seed somewhere offline. Anyone with the seed can control the wallet.

## Import an existing seed in the web app

1. Connect the device.
2. In **Load a seed**, paste your seed phrase.
3. Enter the device PIN you want to set.
4. Optional: enter a BIP39 passphrase if your wallet uses one.
5. Click **load seed**.

The seed phrase must be a valid BIP39 word count: 12, 15, 18, 21, or 24 words.

## Import from `nockchain-wallet`

If you have a `keys.export` file, use the **import it** link in the seed form. The app reads the file locally and fills in the seed phrase if the file contains one. Review the filled phrase before loading it.

After the first seed exists, you can add more wallet slots from the **Wallet** panel. Additional slots use the existing device PIN.

## PIN notes

- Pick a PIN you can remember.
- The device tracks failed PIN attempts.
- Locking clears unlocked key material from RAM.
- **reset** erases the seed and PIN from the device. It does not erase your written backup.
