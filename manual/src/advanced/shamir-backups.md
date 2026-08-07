# Shamir backups

The web app can split a Nockchain extended private key into several shares. You choose a threshold such as 3-of-5: any three shares restore the wallet, while fewer than three reveal nothing.

## Create shares

1. Open **Shamir backup** in the device tab and choose **split**.
2. Paste a `zprv` or a 64-byte raw coil in hex.
3. Choose how many shares are needed and how many to create, then click **split**.
4. Record every share and store them in separate places.

Splitting happens locally in the browser, but it requires key material you already possess. Nockster never exports an existing device seed or private key for this tool. Anyone who obtains the threshold number of shares controls the wallet.

## Restore shares

Connect and unlock Nockster, choose **restore**, and paste at least the threshold number of shares separated by spaces or newlines. After you confirm on the device, the recovered key is imported as a new wallet slot.

The CLI can perform the same offline split and combine operations:

```sh
nockster-cli shamir split --zprv <zprv> --threshold 3 --shares 5
nockster-cli shamir combine --share <share1> --share <share2> --share <share3>
```

A reconstructed coil restores the same wallet keys, but it does not recreate the original mnemonic words.
