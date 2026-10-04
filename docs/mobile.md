# Native mobile apps

Nockster uses SwiftUI on iOS 26+ and Jetpack Compose / Material 3 on Android 12+
(API 31). The iOS tabs, navigation bars, sheets, and buttons use the system's
Liquid Glass appearance. Both interfaces support Dynamic Type / system font
scaling, native text entry, device sharing, and light/dark appearance.

The native UI calls a bundled TypeScript/Rust-WASM wallet engine through a
noninteractive WebView. No wallet screens are rendered by that WebView. Capacitor
provides local asset loading, native HTTP, and durable native preferences. The
engine uses the same vault, planner, fee calculation, signing, and pending-input
reservation logic as the extension. RPC authentication signs a Nockblocks Iris V1 challenge with the saved wallet key using
`iris-v1` (the pinned Iris SDK). The resulting API key is encrypted by the vault
master key and reused across launches. Locking drops the in-memory credential.
The engine loads bundled code; there is no remote UI
or live-update server in the application configuration.

## Build

Install Node 22.18+, run `npm ci`, and install the Rust/WASM prerequisites listed
in the root README. Then build the shared engine and synchronize the projects:

```sh
npm run build:mobile
```

### iOS first

Use a Mac with Xcode 26+ and the iOS 26 SDK:

```sh
npm run mobile:ios
```

Open the **App** target's Signing & Capabilities panel, choose your development
team, and set the bundle identifier for your Apple developer account. The
project's identifier is `io.swps.nockster`. Choose an iPhone or simulator and
Run. Swift Package Manager resolves the native dependencies.

An unsigned simulator compilation is available from the command line:

```sh
xcodebuild -project apps/mobile/ios/App/App.xcodeproj -scheme App \
  -sdk iphonesimulator -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath /tmp/nockster-ios-build CODE_SIGNING_ALLOWED=NO build
```

The [Build and release Nockster workflow](../.github/workflows/build.yml) uses
the shared `[self-hosted, macOS, ARM64]` runner for iOS, matching Nockchain's
release builder. The runner group must allow `SWPSCO/nockster` and provide Xcode
with the iOS 26 SDK. Relevant merges to `master` and manual dispatches build
the app. The shared Rust/WASM engine builds on Linux.

The `release-ios` artifact contains a signed App Store IPA, an unsigned simulator
ZIP, and symbols. See [release setup](releases.md) for signing secrets and the
optional TestFlight upload. The simulator ZIP preserves the app bundle for
installation with `simctl`. Local physical-device development requires your
development team and provisioning profile in Xcode.

Secure Enclave enrollment requires a physical device with a passcode. The
simulator supports password-based wallet flows but cannot validate enclave
protection. Native iOS compilation requires macOS; a successful shared-engine
build is not an iOS compilation.

### Android

Install Android Studio, JDK 21, and Android SDK platform 36:

```sh
npm run mobile:android
```

Or compile a debug APK with `ANDROID_HOME` pointing to the SDK:

```sh
npm run mobile:sync
cd apps/mobile/android
./gradlew :app:assembleDebug
```

The APK is `apps/mobile/android/app/build/outputs/apk/debug/app-debug.apk`. Release signing
uses your own Android signing configuration. No release signing keys belong in
the repository.

`mobile:sync` refreshes the bundled engine without recompiling Rust. Run
`build:mobile` after editing Rust. The mobile Vite build reads environment files
from `apps/mobile/`; an optional `apps/mobile/.env.local` can set `VITE_RPC_URL` and
`VITE_RPC_V1_URL`. Defaults use `https://nockblocks.com/rpc` and `/rpc/v1`.
`VITE_ACCOUNTS_URL` selects the accounts service (default `https://nockblocks.com`),
`VITE_RPC_NETWORK` selects the API-key network (default `mainnet`), and
`VITE_WALLET_GRPC_URL` selects the authenticated wallet gRPC-web gateway (default
the RPC origin). Credentials are bound to the service, network, RPC endpoints, and
wallet address. The server sets rate limits for the address; an email is optional.

Nockster signs into Nockblocks automatically with the selected wallet's Iris
signature. Adding keys starts sign-in without opening a browser or asking for a
code. If the address is already bound to a Nockblocks account, the signature
opens that account; otherwise Nockblocks creates a wallet account. Saving a
wallet still works offline, and network requests retry sign-in when needed.
Private keys remain on the device. A saved contact or watched address is not an
account identity, and a browser session for a different account does not change
which account the wallet opens. Both mobile and extension address books use the
signed-in account's Nockblocks aliases. Local extension contacts remain local
until explicitly saved online.

Transaction submission uses the Iris SDK's public V2 gRPC-web protobuf protocol.
Capacitor transfers its request and response as binary data. The gateway records
transaction IDs in durable JetStream storage and distinguishes acknowledgement
from node acceptance. Activity refresh checks `getTransactionSubmission`; neither
state implies block confirmation. Pending inputs remain reserved until chain
history confirms the payment. [Service setup](wallet-services.md) describes the
accounts, RPC gateway, and Cloudflare configuration.

## Device-protected private keys

The vault's private keys and recovery phrases are encrypted with a random
32-byte vault master key (VMK). A wallet password wraps that VMK using the
existing Argon2id/ChaCha20-Poly1305 vault format.

**Settings → Enable Device Unlock** adds a device-local encrypted envelope for
that same VMK. Enrollment requires the correct wallet password and device
authentication. It never stores the password:

- **iOS:** a non-exportable Secure Enclave P-256 key wraps the VMK using ECIES.
  Keychain holds the encrypted envelope and the enclave key reference with
  `WhenPasscodeSetThisDeviceOnly` accessibility. Private-key use requires user
  presence: Face ID, Touch ID, or the device passcode.
- **Android:** a non-exportable Android Keystore AES-256-GCM key wraps the VMK.
  StrongBox is requested on devices that advertise it; other devices must provide
  a hardware-backed trusted execution environment. Enrollment checks the key's
  security level and hardware enforcement of authentication. Every use requires
  strong biometrics or the device screen-lock credential. Software-only key
  storage is rejected.

The secure hardware decrypts the VMK after authentication. The wallet engine
uses that VMK to decrypt wallet keys for Nockchain signing. Nockchain's signing
curve is not performed inside Secure Enclave or Android Keystore. The unwrapped
VMK crosses the native/engine bridge transiently and is not written to storage.
Rust clears the key slot on lock and zeroizes the temporary key buffers in the
hardware unlock API.

Backgrounding locks the vault, clears native sensitive screens, and invalidates
prepared payments. iOS covers app-switcher snapshots; Android uses `FLAG_SECURE`
and disables application backups. Native preferences retain only the encrypted
vault and public wallet metadata, including signed pending transactions and
reserved input IDs. Disabling device unlock removes its hardware key and envelope
without deleting the vault. If the hardware key becomes unavailable, the wallet
password and recovery phrase remain the recovery paths.

## Mobile workflows

Both native interfaces provide wallet creation/import, recovery-phrase
confirmation, password/device unlock, wallet selection and rename, balance
refresh, multiple-recipient payments, exact fee review, private outputs,
receive QR codes, copying/sharing addresses, history, and retrying a persisted
pending transaction. Payment links use `nockster://send?to=ADDRESS&amount=1.25` or
`web+nockster://send?to=ADDRESS&amount=1.25`; links only prefill the review flow.
They never authorize signing or submission.

Hardware wallet pairing, watch-only management, bridge deposits, and transaction
file signing use the desktop extension. Mobile does not automatically import an
extension vault; use the recovery phrase or extended private key to import a
wallet on another device.

## Validation

```sh
npm run typecheck
npm test
npm run test:mobile
npm run test:core
```

The browser integration tests exercise the actual WASM engine using synthetic
wallets and intercepted network requests. They cover encrypted persistence,
password-gated VMK export, invalid-key rejection, relocking, queued-operation
invalidation, payment review binding, replay rejection, and reservation
preservation across wallet renames. They never broadcast funds. These tests
require Chrome (`channel: 'chrome'` in `playwright.mobile.config.ts`).

Before distributing a native release, validate device enrollment, biometric and
PIN/passcode cancellation, background locking, cold launch, removal of device
credentials, and password recovery on a physical iPhone and Android device.
Native hardware authorization is outside the browser test boundary.

Platform references: [Apple's Liquid Glass guidance](https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass),
[Secure Enclave key protection](https://developer.apple.com/documentation/security/protecting-keys-with-the-secure-enclave),
[Android Keystore](https://developer.android.com/privacy-and-security/keystore),
[Android biometric authentication](https://developer.android.com/identity/sign-in/biometric-auth),
and [Capacitor build requirements](https://capacitorjs.com/docs/getting-started/environment-setup).
