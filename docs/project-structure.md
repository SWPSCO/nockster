# Project structure

The repository separates platform shells from shared wallet code and hardware.
Run wallet npm commands from the repository root. Hardware commands run from
`nockster-esp/`, whose Cargo workspace and JavaScript projects are independent.

```text
apps/
  mobile/              Capacitor config, web entrypoint, Android and iOS projects
  extension/           Popup and approval entrypoints, workers, manifest assets
  desktop/             Desktop Svelte shell, Vite config, Tauri native project
packages/
  wallet/
    src/               Shared Svelte screens, wallet state, vault and adapters
    public/            Shared static assets
  wallet-engine/       Rust signing engine and generated API model crates
nockster-esp/           Firmware, protocol libraries, CLI, hardware GUI and web app
scripts/               Shared development and release scripts
scripts/ci/            Release planning, signing, packaging and store uploads
tools/macos-release/   Rust macOS signing and notarization tool
tests/                 Wallet unit, browser and release-tooling tests
examples/              Standalone design reference
.github/workflows/     Wallet and hardware CI
```

The root npm workspace includes the three apps and `packages/wallet`.
`package-lock.json` pins their dependencies. `packages/wallet-engine` builds
WASM into the ignored `packages/wallet/src/pkg` directory. Each app's Vite
configuration bundles that shared engine for its own runtime.

Mobile native views communicate with `packages/wallet/src/platform/nativeBridge.ts`.
The extension owns its background worker and website requests under
`apps/extension/src`. Desktop owns its window and native commands under
`apps/desktop/src` and `apps/desktop/src-tauri`; shared wallet storage selects the
appropriate platform adapter.

The wallet engine, desktop shell, macOS release tool, and hardware project keep
separate Rust lockfiles and toolchains. There is no root Cargo workspace.
See the [main README](../README.md) for component links and build commands,
and [release documentation](releases.md) for CI routing and publication.
