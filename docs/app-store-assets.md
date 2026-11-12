# App Store screenshots

Capture the production SwiftUI screens on disposable iPhone 17 Pro Max and
13-inch iPad Pro simulators. The capture test imports a wallet from a local
24-word recovery-phrase file, refreshes real network data, and saves five PNGs
per device. It never places the phrase in source code or command-line arguments.

```sh
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
npm run mobile:sync
python3 scripts/ci/ios-store-screenshots.py /path/to/local-seed /path/to/output
```

Use a dedicated demonstration wallet whose public address, balance, and activity
are suitable for publication. Keep the seed file outside the repository.
The output directory is also best kept outside the repository.

The capture session blocks transaction submission, retries, and secret export.
The Send image shows an unsubmitted 25 NOCK draft to the wallet's own address.
It does not broadcast a transaction. The wrapper deletes its disposable
simulators on success or failure, including their wallet data; it retains the
original input seed file. The test is skipped in ordinary test runs unless its
capture environment is explicitly provided.

The screenshot order is wallet overview, receive, activity, send, and wallet in
dark mode. Captures use English/US, portrait orientation, and a normalized 9:41
status bar. `simctl` captures the native display after each view settles; images
are not UI mockups. Inspect every output, especially tab selection and text
layout, before uploading. A live service failure causes the capture to fail
rather than publishing an error state as a wallet screenshot.

Upload the iPhone and iPad sets to the corresponding App Store Connect screenshot
slots. Match the screenshots to the build being submitted. Apple requires a new
version to replace screenshots on an already approved version.

- [Screenshot sizes and file requirements](https://developer.apple.com/help/app-store-connect/reference/app-information/screenshot-specifications)
- [Uploading screenshots](https://developer.apple.com/help/app-store-connect/manage-app-information/upload-app-previews-and-screenshots/)
- [Product page guidance](https://developer.apple.com/app-store/product-page/)

## Public data for offline mocks

`tests/fixtures/demo-wallet-rpc.json` records the demonstration wallet's public
balance, recent activity (up to 20 transactions), V0/V1 notes, chain tip, and
price at capture time. It contains no API key, recovery phrase, account cookies,
or private address book. Public wallet and transaction addresses remain intact
so ownership and transaction-direction parsing can be tested faithfully. The
timestamp in the fixture identifies the snapshot; prices and balances are not
live estimates. The six calls are sequential, not an atomic chain snapshot.

Refresh it with a read-only API key entered at the hidden prompt:

```sh
python3 scripts/ci/capture-wallet-fixture.py PUBLIC_DEMO_ADDRESS tests/fixtures/demo-wallet-rpc.json
```

The script uses only six fixed read methods on `https://nockblocks.com`, rejects
redirects, and never puts the key in arguments or saved responses. Review the
public wallet data before committing a refreshed snapshot.

For browser tests, install `mockDemoWallet(page)` from
`tests/mobile/demo-wallet.fixture.ts` before navigation. Add `mockAccounts(page)`
afterward if the test needs synthetic authentication. Replay blocks unmatched
HTTPS requests and RPC methods, including transaction submission, and rejects
queries for other wallet addresses. Contacts are deliberately empty. See
`tests/mobile/demo-wallet.spec.ts` for a seed-free example exercising the real
RPC parser and the replay restrictions.

This fixture does not change the native screenshot capture flow: those PNGs
still use actual live wallet state through the opt-in XCTest above.
