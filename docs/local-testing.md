# Local testing environment

Both native apps use the application identifier `io.swps.nockster`. An app
installed under a different identifier has a separate data container. Export
its encrypted vault and import it into Nockster to transfer a test wallet;
keep the original installation until the imported wallet is verified.

Run commands from the Nockster repository root unless a section says otherwise.
Use separate browser profiles and empty test wallets for local development.
The automated wallet tests intercept remote requests and do not broadcast
transactions. Manual app sessions use the configured RPC services.

## Shared prerequisites

Install Git, Node.js 22.22.2+ on the 22.x line, npm 12.1.0, Rust through rustup,
a C/C++ compiler, and Google Chrome. Linux packaging also uses `zip`, `unzip`,
and OpenSSL.
On macOS, install Xcode's command-line tools; the native iOS app needs full
Xcode as described below.

Install the wallet's pinned Rust toolchain and WASM build tool:

```sh
rustup toolchain install nightly-2025-02-14 --profile minimal --target wasm32-unknown-unknown
rustup toolchain install stable --profile minimal
cargo +stable install wasm-pack --version 0.15.0 --locked
npm install --global npm@12.1.0 --ignore-scripts
npm ci
npm run build:wasm
```

Keep `packages/wallet-engine/Cargo.lock` and the package lockfile in place. The build uses the
pinned Git dependencies in `packages/wallet-engine/Cargo.toml`; sibling Nockchain, Iris, and RPC
repositories are not needed to run client tests. `npm ci` installs the Iris SDK
under the `iris-v1` package name. The WASM build also generates the shared vanity miner in `packages/wallet/public/vanity`.
Build outputs go in `packages/wallet/src/pkg`, `apps/extension/ext/dist`, and
`apps/mobile/dist`.

The [dependency policy](dependencies.md) enforces a fourteen-day delay for
routine upgrades and disables dependency installation scripts.

## Fast checks without a backend

Install Playwright's Chromium for the extension service-worker tests. On Linux,
`--with-deps` installs the required browser system libraries and can need sudo:

```sh
npx playwright install --with-deps chromium
npm run typecheck
npm test
npm run test:core
npm run test:mobile
npm run build
npm run test:extension
```

`test:mobile` starts the mobile engine's Vite server on `127.0.0.1:5174` and uses
installed Google Chrome. This exercises the real WASM vault and the native
bridge API, including payment review, locking, RPC authentication, account
sign-in, and binary transport. It does not render SwiftUI or Compose or exercise
secure hardware. Keep port 5174 free so the tests use their own server.

`test:extension` loads the built `apps/extension/ext/` package in a temporary Chromium profile
and tests its actual service worker. Run `npm run build` first; it does not
rebuild the package itself. The package-signing test in `npm test` uses Chrome
with a generated temporary RSA key. If necessary, set `CHROME_BIN` to the Chrome
executable, for example on macOS:

```sh
export CHROME_BIN='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
```

No production signing key, GitHub secret, API key, or running backend is required
for these automated checks.

## Browser extension

Build the production package and open a dedicated Chrome profile:

```sh
npm run build
```

Open `chrome://extensions`, enable **Developer mode**, choose **Load unpacked**,
and select the repository's `apps/extension/ext/` directory. Create an empty wallet or import a
test recovery phrase. Wallet import signs in automatically; the address book uses the bound
Nockblocks account. The service worker's **Inspect** link opens its own developer console.

After TypeScript or Svelte changes, run `npm run build:svelte` and click the
extension's Reload button. After Rust changes, run `npm run build`. Reloading
clears the unlocked in-memory vault; the encrypted test wallet persists in that
browser profile. The built manifest uses `dist/index.html`,
`dist/background.js`, and `dist/nockster_core_bg.wasm`; retain those paths when
packaging.

For a signed package with the fixed Nockster ID, use the CI artifact or follow
[Extension signing and CI](extension-release.md). An unpacked local checkout can
have a different ID. Do not add signing keys to the extension directory.

## Android emulator or device

Install Android Studio, JDK 21, Android SDK platform 36, platform-tools, and an
Android emulator system image. The app supports Android 12 / API 31 and newer.
Use Android Studio's Device Manager to create an emulator, then start it.

Set your actual JDK and SDK locations in the shell. For example:

```sh
export JAVA_HOME=/path/to/jdk-21
export ANDROID_HOME="$HOME/Android/Sdk"
export PATH="$JAVA_HOME/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"
java -version
adb devices
npm run mobile:sync
```

On macOS the SDK commonly lives at `$HOME/Library/Android/sdk`; use that path
instead if it matches your installation. Then either open Android Studio:

```sh
npm run mobile:android
```

Or build and install the debug APK from the command line:

```sh
(cd apps/mobile/android && ./gradlew :app:assembleDebug :app:lintDebug)
adb -s emulator-5554 install -r apps/mobile/android/app/build/outputs/apk/debug/app-debug.apk
adb -s emulator-5554 shell am start -n io.swps.nockster/.MainActivity
```

Replace `emulator-5554` with the serial reported by `adb devices`. `install -r`
preserves the app's encrypted test vault. Use the test wallet's password to
unlock after relaunch. Device unlock requires hardware-backed Keystore support;
a software-only emulator cannot validate that flow. Use a physical Android
phone for biometric/device-credential enrollment and recovery checks.

`adb logcat` provides native errors. Debug builds also expose the engine's
WebView through Chrome's `chrome://inspect/#devices`; the actual UI is Compose.
Native bridge logging is disabled because calls can carry sensitive values.

The native keyboard regression check uses an emulator with an existing test
vault. It captures the locked screen with and without the keyboard and checks
that the entire unlock button remains visible:

```sh
(cd apps/mobile/android && ./gradlew :app:connectedDebugAndroidTest -Pandroid.testInstrumentationRunnerArguments.class=io.swps.nockster.UnlockLayoutTest)
```

The test temporarily permits screenshots of the locked screen and enters a
synthetic password without submitting it. Production screen capture protection
remains enabled.

## iOS simulator or iPhone

Use an Apple-silicon or Intel Mac with Xcode 26+, the iOS 26 SDK, and an installed
iOS 26 simulator runtime. Xcode 26.0–26.3 require macOS Sequoia 15.6 or newer;
check [Apple's Xcode requirements](https://developer.apple.com/xcode/system-requirements)
for the selected Xcode version. Command Line Tools alone cannot build the app.

Select full Xcode for this shell, then check the SDK:

```sh
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
xcodebuild -version
xcrun --sdk iphoneos --show-sdk-version
npm run mobile:sync
npm run mobile:ios
```

In Xcode, select the **App** scheme and an iPhone simulator, then Run. Install a
simulator runtime from Xcode Settings if no matching device is available.
Swift Package Manager resolves Capacitor and the native preferences plugin.

An unsigned simulator build from the command line is:

```sh
xcodebuild -project apps/mobile/ios/App/App.xcodeproj -scheme App \
  -sdk iphonesimulator -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath /tmp/nockster-ios-build CODE_SIGNING_ALLOWED=NO build
```

With an iOS 26 simulator already booted, install and launch it:

```sh
xcrun simctl install booted /tmp/nockster-ios-build/Build/Products/Debug-iphonesimulator/App.app
xcrun simctl launch booted io.swps.nockster
```

The simulator covers SwiftUI navigation, wallet creation/import, password unlock,
send review, automatic account sign-in, and aliases. Secure Enclave enrollment requires a
physical iPhone with a passcode. For a device build, choose your development team
under **Signing & Capabilities**, select the connected iPhone, and run from
Xcode. An unsigned device build from CI is not installable on an iPhone.

## Iterating on the shared engine

`npm run mobile:sync` rebuilds TypeScript assets and copies them into both native
projects. It does not rebuild Rust. After Rust changes use:

```sh
npm run build:mobile
```

Rebuild and relaunch the native app after synchronization. The apps load bundled
assets, so running a Vite server alone does not update an installed native app.
`npm run dev:mobile` serves the bridge engine for browser diagnostics and tests;
it is not a browser version of the native wallet screens.

## Choosing RPC and accounts services

Default manual builds use `https://nockblocks.com`, the `mainnet` API-key network,
JSON RPC at `/rpc` and `/rpc/v1`, and public V2 gRPC-web routes on the same origin.
Wallet sign-in obtains its own API key; no static RPC token is needed.

For extension overrides, use a root `.env.local`. Mobile reads from
`apps/mobile/.env.local`. Use the same values in both when comparing platforms:

```dotenv
VITE_ACCOUNTS_URL=https://your-test-host.example
VITE_RPC_URL=https://your-test-host.example/rpc
VITE_RPC_V1_URL=https://your-test-host.example/rpc/v1
VITE_WALLET_GRPC_URL=https://your-test-host.example
VITE_RPC_NETWORK=your-configured-network-slug
```

The accounts service must issue API keys for that network, and its RPC gateway
must use the corresponding node. Add custom extension origins to host
permissions in `scripts/generate-manifest.cjs`. Rebuild after changing any of
these values. Native builds expect HTTPS; Android emulator loopback addresses
and desktop `localhost` do not identify the same host. A trusted HTTPS development
endpoint avoids certificate and native cleartext-transport failures.

A complete local service stack lives in `../nockchain-rpc-server`: the accounts
API with PostgreSQL, the RPC API with persistent NATS JetStream, and Curator's
authenticated private-node connection. The browser client lives in
`../nockblocks`. See [Wallet services](wallet-services.md) for endpoint contracts,
service credentials, node routing, and binary-preserving proxy configuration.
The client's isolated tests are the starting point when that stack is not
running.

## Native smoke check

Create an empty test wallet, confirm its recovery phrase, lock it, unlock with
the password, background and reopen the app, and verify it is locked. Then check
wallet selection, receive/copy/share, address-book refresh, optional browser
automatic account sign-in, and send-review validation. Network checks use the selected
backend; submitting a reviewed transaction is a real broadcast.

Check Send's reset, recipient copy, contact filter, shortened contact address,
and QR scanner. Camera access is requested when scanning; cancellation returns
to the form. Review a payment, enter an incorrect password, and cancel device
authentication: neither action submits it. Every send and explicit retry
requires confirmation. The isolated transaction tests verify password/device
credentials and acknowledgement handling without broadcasting.

In Activity, pull to refresh, open a transaction, and copy its recipient and ID.
Contacts use saved nicknames. Pending, submitted, accepted, and confirmed are
distinct states; a confirmed history entry clears its pending input reservation.

On physical hardware also check device unlock enrollment, cancellation,
backgrounding during authentication, cold launch, and password recovery after
disabling device unlock. Keep simulator/emulator results separate from those
hardware checks.
