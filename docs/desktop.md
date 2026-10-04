# Desktop app

Nockster runs on Linux, macOS, and Windows. It supports software and watch-only
wallets. Use the Chrome extension for hardware wallets.

The wallet locks when you quit. To lock it while the app is open, press
Ctrl+Shift+L, or Cmd+Shift+L on macOS. Links open in your default browser.

## Back up your wallet

Quit Nockster and copy `wallet.json` from your user data folder:

| System  | File                                                                 |
| ------- | -------------------------------------------------------------------- |
| Linux   | `~/.local/share/io.swps.nockster.desktop/wallet.json`                |
| macOS   | `~/Library/Application Support/io.swps.nockster.desktop/wallet.json` |
| Windows | `%APPDATA%\io.swps.nockster.desktop\wallet.json`                     |

On Linux, `XDG_DATA_HOME` overrides `~/.local/share`.

To restore, quit Nockster on the destination computer. Keep a copy of any wallet
file already there, then put your backup at the path above. Open Nockster and use
the password from when you made the backup. This replaces the wallet collection;
it does not merge wallets. Keep your recovery phrases separately too.

Private keys and recovery phrases are encrypted with ChaCha20-Poly1305. Argon2id
protects the vault key using your password, 128 MiB of memory, three passes, one
lane, and a random 32-byte salt. The backup works across computers without an OS
keychain. Use a strong, unique password: someone with the file can try guesses
offline.

Addresses, wallet names, cached balances, and operation timestamps are readable
in the file. Contacts and app preferences are stored separately and aren't
included in this backup. Saves replace the file atomically, with `0600` file
permissions on Linux and macOS. The app reports unreadable data without replacing it.

## Run locally

Install the tools in the [README](../README.md#build-and-run) and the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```sh
rustup toolchain install 1.99.0 --profile minimal
npm ci
npm run build:wasm
npm run dev:desktop
```

The desktop app uses Rust 1.99.0; the shared WASM engine uses the pinned nightly
toolchain. To check the app and build an installer:

```sh
npm run typecheck
npm test
npm run test:desktop
cargo +1.99.0 test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
npm run build:desktop -- -- --locked
```

Desktop tests cover password unlock, copying and restoring a backup, locking,
onboarding, and corrupt files. They use test wallets and never send real funds.

## Build releases

Pushes to `master` build the platforms affected by the changed files.

| Changed files                                            | Builds                |
| -------------------------------------------------------- | --------------------- |
| Desktop app, Tauri config, desktop tests or signing tool | Desktop               |
| iOS app                                                  | iOS                   |
| Android app                                              | Android               |
| Mobile bridge and mobile config                          | iOS and Android       |
| Shared Svelte screens, styles and assets                 | Desktop and extension |
| Shared wallet engine or dependencies                     | All platforms         |
| Documentation                                            | None                  |

The separate mobile bridge tests also skip desktop and extension UI changes.
To choose platforms yourself, run **Build and release Nockster** in GitHub Actions
and select `desktop` or `all`. Enable `publish` on `master` to create a GitHub Release.

| System                     | Installer                   |
| -------------------------- | --------------------------- |
| Linux x86-64               | `.deb` and `.AppImage`      |
| macOS 14.4+, Apple Silicon | Signed and notarized `.dmg` |
| Windows x86-64             | NSIS `.exe`, unsigned       |

After all desktop builds pass on `master`, CI uploads the downloads to
`r2:swpsco/fletch/` using the `RCLONE_B64` secret. The public links stay the same
as new versions ship:

- [Linux AppImage](https://bin.aeroe.io/fletch/nockster-linux-x64-latest.AppImage)
- [Linux Debian package](https://bin.aeroe.io/fletch/nockster-linux-x64-latest.deb)
- [macOS Apple Silicon](https://bin.aeroe.io/fletch/nockster-macos-arm64-latest.dmg)
- [Windows x86-64](https://bin.aeroe.io/fletch/nockster-windows-x64-latest.exe)

Each download has a matching `.sha256` file. Branch builds remain in GitHub
Actions and do not replace these downloads. Windows may show a trust prompt
because its installer is unsigned.

## macOS signing

The Rust tool in `tools/macos-release` uses
[apple-codesign](https://crates.io/crates/apple-codesign) to sign the app and DMG,
submit them to Apple, and attach the notarization tickets. Set these GitHub secrets:

| Secret               | Contents                                                             |
| -------------------- | -------------------------------------------------------------------- |
| `APPLE_P12`          | Base64 Developer ID Application certificate and private key (`.p12`) |
| `APPLE_P12_PASSWORD` | Password for the `.p12`                                              |
| `AC_API_KEY_ID`      | App Store Connect API key ID                                         |
| `AC_API_ISSUER_ID`   | App Store Connect issuer UUID                                        |
| `AC_API_KEY_PATH`    | Base64 contents of the API key (`.p8`)                               |

The signing key stays in memory. No temporary keychain is needed. CI checks the
signature, notarization ticket, and Gatekeeper assessment before uploading the
DMG. A failed check stops the release. Users can open a successful release
normally, without a Gatekeeper override. macOS may still ask for its usual
first-open confirmation.
