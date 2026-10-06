# Nockster

Nockster wallets and hardware live in one repository. The wallet apps share a
Rust signing engine and TypeScript wallet logic; each platform owns its shell,
build configuration, and native integrations.

| Component | Location | Purpose |
| --- | --- | --- |
| Mobile wallet | [`apps/mobile`](apps/mobile) | Native SwiftUI and Android Compose apps with a Capacitor wallet bridge |
| Browser extension | [`apps/extension`](apps/extension) | Chromium extension, popup, website approvals, and background worker |
| Desktop wallet | [`apps/desktop`](apps/desktop) | Tauri wallet for Linux, macOS, and Windows |
| Shared wallet | [`packages/wallet`](packages/wallet) | Svelte screens, wallet state, storage adapters, signing bridge, and shared assets |
| Wallet engine | [`packages/wallet-engine`](packages/wallet-engine) | Rust vault, transaction signing, WASM bindings, and API models |
| Hardware wallet | [`nockster-esp`](nockster-esp) | ESP32 firmware, protocol library, CLI, hardware companion GUI, web tools, enclosure, and manual |
| Release tooling | [`scripts/ci`](scripts/ci), [`tools/macos-release`](tools/macos-release) | Version planning, signing, installer collection, and store publication |
| Design reference | [`examples/design-reference`](examples/design-reference) | Standalone interface reference |

## Wallet development

Use Node and npm versions matching `package.json`. Install dependencies from
this directory; npm workspaces share the root lockfile. The Rust engine pins
its toolchain in `packages/wallet-engine/rust-toolchain.toml`; the desktop
shell pins its own toolchain in `apps/desktop/src-tauri/rust-toolchain.toml`.

```sh
npm ci
npm run build:wasm
npm run build:hardware:wasm # Required for desktop hardware features
npm run dev:ext          # Extension UI on localhost:5173
npm run dev:mobile       # Headless mobile bridge on localhost:5174
npm run dev:desktop      # Native desktop wallet; requires Tauri system dependencies
```

Run one development server per port. Load `apps/extension/ext` as an unpacked
extension after `npm run dev:setup`. For native mobile projects, run
`npm run mobile:sync`, then `npm run mobile:ios` or `npm run mobile:android`.
Capacitor sync generates native dependency paths from the mobile workspace.

```sh
npm run build            # WASM and production extension
npm run build:mobile    # WASM, mobile web bundle, and native project sync
npm run build:desktop -- --debug --no-bundle -- --locked
npm run typecheck
npm test
npm run test:desktop
npm run test:mobile
npm run test:extension
python3 -m unittest discover -s tests/ci -p '*_test.py'
```

See [local testing](docs/local-testing.md), [desktop development](docs/desktop.md),
[wallet services](docs/wallet-services.md), and [extension releases](docs/extension-release.md).

## Hardware development

Work inside `nockster-esp/` for hardware commands. It has its own Cargo workspace,
Rust toolchain, JavaScript dependencies, Makefile, and documentation.

```sh
cd nockster-esp
make help
```

The [hardware README](nockster-esp/README.md) describes firmware builds, device
provisioning, host tools, and the hardware web app. Production provisioning helpers use external secrets and default to `../../nockster-secrets/` when
run from `nockster-esp/`.

## CI and releases

The root `.github/workflows/` directory contains both wallet and hardware jobs.

- **Build and release Nockster** routes wallet changes to iOS, Android, extension,
  and desktop jobs. Shared engine changes build all wallet platforms; app changes
  build their consumers. Hardware-only changes do not select wallet releases.
- **Test mobile wallet bridge** exercises the mobile bridge on pushes and pull requests.
- **ESP protocol tests**, **ESP desktop host tools**, **firmware-release**, and
  **Deploy web** build hardware components independently. Firmware uses `fw-v*`
  tags; hardware desktop tools use `desktop-v*` tags.

`release-version.json` contains the wallet version and persistent build sequence
offset. Store publication is selected explicitly in the wallet release workflow.
Wallet downloads use `https://bin.aeroe.io/fletch/`; firmware updates use
`https://bin.aeroe.io/nockster/updates/`. The hardware web app deploys to
`https://my.nockster.com`.

See [releases](docs/releases.md), [Google Play setup](docs/google-play.md), and
[repository CI cutover](docs/ci-repository-move.md) for signing, repository
permissions, runner access, and the Google identity configuration.
