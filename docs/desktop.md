# Desktop app

Nockster runs on Linux, macOS, and Windows. It supports software, watch-only,
and Nockster hardware wallets.

The wallet locks when you quit. To lock it while the app is open, press
Ctrl+Shift+L, or Cmd+Shift+L on macOS. Links open in your default browser.

## Connect a Nockster

Open **Hardware**, connect your Nockster with a USB data cable, and select it
from the device list. Enter its device PIN to unlock it. Hardware management is
available while the software vault is locked and without creating a software wallet.

The **Wallets** tab shows device addresses. **Use wallet** adds its public account
to the app for balances, receiving, and sending. **Verify address** displays the
address on the Nockster. Transaction signing uses the connected device and requires
approval on its screen. The connection stays open as you navigate the app.

- **Wallets:** import a 24-word recovery phrase or zprv private key, rename device
  wallets, export a master public key, and remove seeds with explicit confirmation.
- **Firmware:** fetch an update or choose a signed manifest and matching image,
  review the release, and install it. The app checks the image hash and compatibility;
  the device verifies the signature. Keep USB connected until installation finishes.
- **Security:** inspect reported chip security settings, change the device PIN,
  calibrate or test the touchscreen, restart, and factory reset with confirmation.
- **Tools:** manage device contacts and preimage secrets, sign or verify messages
  and hashes, split and restore Shamir backups, and inspect jammed nouns.

Features depend on the connected firmware. Signature verification, backup
splitting, and noun inspection are also available under **Open offline tools**.
Device contacts and preimages stay on the Nockster. Import and backup secrets are
not saved to app storage. Public addresses and account names are saved locally.

USB access uses native HID. Close another app or CLI using the device before
connecting. On Linux, the signed-in user needs permission to access its HID device
(USB vendor `303a`, product `2001`). Follow the device manual for USB setup.

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
npm run build:hardware:wasm
npm run dev:desktop
```

The desktop app uses Rust 1.99.0. The wallet and hardware WASM engines use their
own pinned nightly toolchains. Linux builds also require `libudev-dev` for HID.
To check the app and build an installer:

```sh
npm run typecheck
npm test
npm run test:desktop
cargo +1.99.0 test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
npm run build:desktop -- -- --locked
```

Desktop tests cover password unlock, copying and restoring a backup, locking,
onboarding, corrupt files, hardware-only accounts, device approval and unplugging,
and firmware validation and transfer. Device tests simulate native USB I/O while
using the real SDK and WASM. They use test wallets and never send real funds.

### Hardware interface

Hardware extends the desktop wallet's visual system. Shared
[wallet variables](../packages/wallet/src/styles/variables.css) and
[themes](../packages/wallet/src/styles/themes.css) own color and type tokens;
[desktop styles](../apps/desktop/src/desktop.css) own the sidebar and workspace.
[Hardware styles](../apps/desktop/src/hardware/hardware.css) apply those tokens
to device management, with flat divided rows for devices, wallets, and facts.
Surface backgrounds distinguish inputs, confirmations, and results. Primary actions
use the theme's text color against its background color.

The hardware workspace scrolls vertically inside the desktop shell. Long
addresses and output wrap, actions wrap into additional rows, and paired form
fields stack at window widths of 960px or less. This keeps management usable
within the desktop app's minimum 800×640 window.

[HardwareWorkspace](../apps/desktop/src/hardware/HardwareWorkspace.svelte)
presents connection state and the Wallets, Firmware, Security, and Tools navigation.
Session errors and pending device instructions stay visible in a sticky feedback
region; errors use alerts and pending operations use status announcements.
Opening wallet import reveals the form and focuses its first field. Preparing
firmware reveals its review before installation. Less frequent operations use
disclosures, and destructive operations expose explicit confirmation controls.

## Build releases

Pushes to `master` build the platforms affected by the changed files.

| Changed files                                            | Builds                |
| -------------------------------------------------------- | --------------------- |
| Desktop app, Tauri config, desktop tests or signing tool | Desktop               |
| Hardware core, hardware WASM or its build inputs         | Desktop               |
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
