# Browser extension builds

The **Build and release Nockster** workflow accepts manual builds with
`platform=extension` and selects the extension when relevant changes reach
`master`. See [release setup](releases.md) for signing secrets and GitHub Release
controls. Feature-branch pushes and pull requests do not start release builds.

Set the repository secret `FLETCH_EXT_KEY_B64` to the base64-encoded RSA signing
PEM. The signing step checks its public key against the Nockster extension ID:

```text
nalokkijbnoknjoojndhgdlapkfjgamb
```

This identity matches the `real.pem` signing key and the existing signed CRX.
Private key files stay outside Git and build artifacts. A CRX is a public,
signed package; it is not an input secret.

CI builds the Rust/WASM core and extension, runs type checks and tests, packs
with Chrome, and verifies the CRX signature and extension ID. The PEM is decoded
into a private temporary file only for signing and removed afterward. The ZIP
contains the same archive bytes as the signed CRX. Both retain the `dist/`
layout required by the bundled WASM engine. The manifest includes only the
signing key's public portion, which preserves the ID when the ZIP is loaded
unpacked.

Download **release-extension** from the Actions run. It contains:

- `nockster-extension-VERSION.crx`
- `nockster-extension-VERSION.zip`
- `extension-SHA256SUMS`
- `extension-release.json` with the extension ID, version, and source commit

The workflow uploads GitHub Actions artifacts and optionally creates a GitHub
Release. Chrome Web Store publication
uses the ZIP and the store's separate publishing credentials. Browser policies
can restrict installing a self-signed CRX; loading the extracted ZIP through
**chrome://extensions → Developer mode → Load unpacked** supports local testing.

For a local unsigned ZIP:

```sh
npm run build:zip
```

For signing, supply `FLETCH_EXT_KEY_B64` from your secret manager and run
`npm run package:extension` after building. Set `CHROME_BIN` if Chrome is not
available as `google-chrome`. Output is under `release/extension/`.

The package version comes from `release-version.json`, with an optional manual
workflow override. Increase it for an extension update while retaining the same
signing key. Builds do not change the version automatically.
