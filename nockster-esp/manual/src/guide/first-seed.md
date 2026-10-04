# First seed and PIN

Nockster needs one seed slot before it can sign. The first seed also creates the device PIN.

## Generate on the device

1. On the Nockster screen, choose **Hardware RNG**.
2. Write down the 24 words exactly as shown.
3. Complete the on-device backup check by selecting the requested words.
4. When prompted, enter a PIN.
5. Enter the same PIN again at **Repeat PIN**.

Store the written seed somewhere offline. Anyone with the seed can control the wallet.

## Generate with physical dice

Dice mode creates a standard 24-word BIP-39 seed from 100 fair six-sided die results, providing about 258 bits of source entropy:

1. Choose four dice and give them fixed labels or colors: A, B, C, and D.
2. On Nockster, choose **4 Dice x 25**.
3. Throw all four dice and enter their results in A, B, C, D order.
4. Repeat for 25 throws. Use **DEL** to correct the most recent entry.
5. After all 100 results are entered, press **DONE**.
6. Write down the 24 words and complete the on-device backup check.

**Do not sort the dice by value or change their labels between throws** because you will discard significant entropy -- dice A is always dice A. The same ordered results always reproduce the same seed; hardware randomness is deliberately not mixed in.

## Import an existing seed in the web app

1. Connect the device.
2. In **Load a seed**, paste your seed phrase.
3. Enter the device PIN you want to set.
4. Optional: enter a BIP39 passphrase if your wallet uses one.
5. Click **load seed**.

The seed phrase must be a valid BIP39 word count: 12, 15, 18, 21, or 24 words.

## Import from `nockchain-wallet`

If you have a `keys.export` file, use the **import it** link in the seed form. The app reads the file locally and fills in the seed phrase if the file contains one. Review the filled phrase before loading it.

After the first seed exists, you can add more wallet slots from the **Wallet** panel or **Add Seed** on the device. Additional slots use the existing device PIN.

## PIN notes

- Pick a PIN you can remember.
- The device tracks failed PIN attempts.
- Locking clears unlocked key material from RAM.
- **reset** erases the seed and PIN from the device. It does not erase your written backup.
