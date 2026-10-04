# Wallet slots and keyfiles

Nockster is compatible with the `nockchain-wallet` key derivation path used by this repo.

## Import `keys.export`

In the seed form, use **import it** next to `keys.export`. The browser parses the file locally:

- If the file contains a seed phrase, the app fills it into the seed field.
- Review the phrase, then load it through the normal seed flow.
- If the file contains only derived private or public coils and no seed phrase, the app will not import it as a seed phrase.

The CLI equivalent is:

```sh
nockster-cli seed --keyfile keys.export --pin 1234
```

## Import a `zprv`

After the device already has an initial seed and PIN, paste a `zprv` extended private key into **Add a seed slot**. The app imports it as a standalone wallet slot.

A `zprv` cannot initialize a blank device because the initial setup path creates the device PIN from a seed phrase flow.

## Export watch-only

Use **export watch-only** on a wallet slot to download a master public key export. The device asks for on-screen confirmation because this reveals the slot's unhardened address tree, but it does not reveal private keys.

Import the downloaded file into `nockchain-wallet`:

```sh
nockchain-wallet import-master-pubkey --file master-pubkey-<label>.export
```

The CLI equivalent is:

```sh
nockster-cli export-master-pubkey --slot 0 --out master-pubkey.export
```
