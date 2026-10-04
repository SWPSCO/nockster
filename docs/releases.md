# Building and releasing Nockster

Use **Actions → Build and release Nockster → Run workflow**. Select `ios`,
`android`, `extension`, `desktop`, or `all`. Manual builds ignore changed-path filtering.
The workflow also runs on pushes to `master`, including merged pull requests.
Feature branch pushes and pull requests do not trigger it.

Selected platforms produce download artifacts. Mobile apps and the macOS desktop
app are signed; Windows and Linux desktop installers are unsigned. Missing
required signing secrets fail that platform. Master pushes update wallet downloads
on R2; store publication requires a manual release selection.

The iOS bundle ID and Android application ID are `io.swps.nockster`. The
extension retains signing identity `nalokkijbnoknjoojndhgdlapkfjgamb`; its
organization signing secret is `FLETCH_EXT_KEY_B64`. The repository is
`SWPSCO/nockster`.

| Change | Builds |
| --- | --- |
| `apps/mobile/ios/`, iOS workflow or signing helper | iOS |
| `apps/mobile/android/`, Android workflow or signing helper | Android |
| `apps/mobile/`, `packages/wallet/src/platform/nativeBridge.ts`, Capacitor config, mobile tests/build config | iOS and Android |
| `apps/extension/`, extension workflow/build helpers/tests | Extension |
| `apps/desktop/`, desktop workflow/build helpers/tests | Desktop |
| Shared Svelte screens and styles | Extension and desktop |
| Rust engine, shared wallet logic, dependencies, release version, shared CI | All four |
| Hardware project/workflows, examples, documentation, and `.gitignore` | None |

Unknown build inputs conservatively select all platforms. The planner diffs the
entire push, including deletions and both sides of renames. A docs-only push
runs the small planning job and skips builds.

## Release controls and versions

- `version`: optional three-part version, such as `1.2.3`. An empty value reads
  `release-version.json`. Commit a version bump there for normal builds.
- `publish`: creates a GitHub Release containing the selected signed packages,
  symbols, and checksums after all selected builds pass. Requires `master`.
- `testflight`: uploads the iOS IPA to App Store Connect. Requires `master`
  and selection of `ios` or `all`. It works independently of `publish`.

App Store Connect processes the upload before it appears in TestFlight. Manage
test groups, beta review, and App Store submission in App Store Connect.
The `google_play` checkbox uploads Android to Internal testing. The `publish`
checkbox also uploads the selected mobile platforms to their stores. Upload the
extension ZIP to the Chrome Web Store separately.

The build sequence is `buildSequenceOffset + run_number`, with the persistent
offset in `release-version.json`. Android's version code is `sequence * 100 +
run_attempt`; the iOS build number is `sequence.run_attempt`. Release tags use
`vVERSION-build.SEQUENCE.ATTEMPT`.
The planner validates the version and build number before running builds.
Keep this workflow's run numbering intact. Increase the marketing version for
each Chrome Web Store update; the extension version has no automatic build suffix.
Rebuilding an older run can produce a lower native build number than a newer run;
dispatch a fresh run for a store upload.

## Organization secrets

Grant `SWPSCO/nockster` access to each organization secret. Google Play requires the two
Actions variables described in [Google Play setup](google-play.md). The publish job uses `GITHUB_TOKEN`
with `contents: write`; build jobs use read-only repository permissions.

| Secret | Contents |
| --- | --- |
| `IOS_DISTRIBUTION_P12_B64` | Base64-encoded P12 containing an **Apple Distribution** certificate and its private key |
| `IOS_DISTRIBUTION_P12_PASSWORD` | P12 export password |
| `IOS_PROVISIONING_PROFILE_B64` | Base64-encoded **App Store Connect distribution** `.mobileprovision` for `io.swps.nockster`, including that certificate |
| `ANDROID_KEYSTORE_B64` | Base64-encoded release/upload keystore (`.jks` or PKCS12) |
| `ANDROID_KEYSTORE_PASSWORD` | Keystore password |
| `ANDROID_KEY_ALIAS` | Alias of the signing key inside the keystore |
| `ANDROID_KEY_PASSWORD` | Password for that key; set this even when equal to the keystore password |
| `FLETCH_EXT_KEY_B64` | Existing base64 RSA PEM for extension ID `nalokkijbnoknjoojndhgdlapkfjgamb` |
| `AC_API_ISSUER_ID` | Existing App Store Connect API issuer ID; only needed for TestFlight |
| `AC_API_KEY_ID` | Existing App Store Connect API key ID; only needed for TestFlight |
| `AC_API_KEY_PATH` | Existing **base64 contents of the `.p8` file**, not a runner path; only needed for TestFlight |

The `AC_API_*` credentials can be shared with Mac release jobs if the API key
has access to the Nockster app and an upload-capable role. `APPLE_P12` and
`APPLE_INSTALLER_P12` used for Developer ID desktop signing do not sign iOS
App Store builds. Keep those desktop secrets as they are. No installer
certificate, notarization credentials, or persistent keychain password is needed
for this iOS workflow. The Apple team ID comes from the provisioning profile.

### iOS setup

1. Register the explicit App ID `io.swps.nockster` in the Apple developer account
   and create the matching app record in App Store Connect.
2. Create an Apple Distribution certificate with a private key available in
   Keychain Access. Export both as a password-protected P12.
3. Create an App Store Connect distribution profile for the App ID and certificate.
4. Add the three `IOS_*` secrets, and allow this repository to access `AC_API_*`
   if TestFlight uploads are desired.

On macOS, pipe file contents directly into `gh` without printing them:

```sh
base64 -i /path/to/distribution.p12 | gh secret set IOS_DISTRIBUTION_P12_B64 --org SWPSCO --repos nockster
base64 -i /path/to/Nockster.mobileprovision | gh secret set IOS_PROVISIONING_PROFILE_B64 --org SWPSCO --repos nockster
gh secret set IOS_DISTRIBUTION_P12_PASSWORD --org SWPSCO --repos nockster
```

The runner needs labels `self-hosted`, `macOS`, `ARM64`, Xcode 26 or newer at
`/Applications/Xcode.app`, the iPhoneOS and simulator SDKs, accepted Xcode license,
and completed first-launch setup. Node 22 is installed by the workflow. The
shared Rust/WASM build runs on Linux, so the Mac only builds the native app.

Signing uses a random-password, per-run keychain and a per-run profile file.
The helper checks expiration, App ID, profile distribution type, and certificate
membership. The final cleanup step restores the original keychain search list
and removes the run's keychain, profile, API key, and build directory. A killed
runner process or machine outage can prevent Actions cleanup; run
`IOS_BUILD_ROOT=/path/from/the/run python3 scripts/ci/ios-signing.py cleanup`
on that runner account to complete it.

The IPA is for App Store Connect/TestFlight distribution. The simulator ZIP
contains an unsigned simulator app. Device sideloading requires a different
provisioning method and is not an output of this workflow.

### Android setup

Use the app's existing release/upload key if one exists. For a new app, create
a keystore outside the repository:

```sh
keytool -genkeypair -v -keystore /secure/path/nockster-upload.jks \
  -alias nockster-upload -keyalg RSA -keysize 3072 -validity 10000
base64 -w 0 /secure/path/nockster-upload.jks | gh secret set ANDROID_KEYSTORE_B64 --org SWPSCO --repos nockster
gh secret set ANDROID_KEYSTORE_PASSWORD --org SWPSCO --repos nockster
gh secret set ANDROID_KEY_ALIAS --org SWPSCO --repos nockster
gh secret set ANDROID_KEY_PASSWORD --org SWPSCO --repos nockster
```

The example uses Linux's `base64`; use `base64 -i FILE` on macOS. Preserve the
keystore in your secret manager. With Play App Signing, the AAB uses the upload
key and Google signs delivered APKs with the app signing key. The downloadable
CI APK uses the configured keystore directly; it can update another installation
only when their signing identities match.

## Build outputs and checks

- `release-ios`: signed IPA, simulator ZIP, dSYM ZIP, `ios-SHA256SUMS`.
- `release-android`: signed APK and AAB, `android-SHA256SUMS`.
- `release-extension`: verified CRX and ZIP, `extension-SHA256SUMS`,
  `extension-release.json` with signing identity, version, and source commit.

Artifacts are retained for 30 days. GitHub Releases retain the selected assets
and a combined `SHA256SUMS`. The intermediate `wallet-engine` artifact is not
published. iOS failure diagnostics are retained for seven days.

The engine job uses pinned Rust `nightly-2025-02-14`, locked Cargo/npm
dependencies, Rust unit tests, TypeScript checks, Node tests, and mobile browser
tests when a native app is selected. It builds WASM once per run and shares that
artifact. Android runs release unit tests and lint, then verifies APK and AAB
signatures. The extension runs its service-worker test and verifies its CRX
signature and expected identity. The iOS job builds the simulator app, archives
the device app, verifies its signature, and exports the IPA with Xcode.

References: [Apple signing in GitHub Actions](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications),
[App Store Connect uploads](https://developer.apple.com/help/app-store-connect/manage-builds/upload-builds),
[Android app signing](https://developer.android.com/studio/publish/app-signing).
