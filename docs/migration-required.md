# Migration Notes: Legacy WASM Helpers

To keep the new vault API authoritative, the legacy `packages/wallet/src/lib/wasm.ts` bridge was removed. Several files still referenced those helpers; they were preserved but commented out with TODOs so designers/engineers know exactly what needs to be reimplemented via `vaultApi.ts`.

## Summary of Required Follow‑Ups

1. **`packages/wallet/src/lib/wasm.ts`** – **Removed.** All wallet creation/export helpers must be reintroduced through the new vault API.
2. **`packages/wallet/src/App.svelte`**
   - The import block that previously pulled `createWalletFromWasm`, `recoverWalletFromSeedPhrase`, etc. is commented with a note explaining the new flow (see file header comment).
   - `handleCreateWallet`, `handleImportWallet`, and `handleSeedPhraseContinue` now show placeholder alerts telling designers that the functionality is temporarily disabled until `vaultApi` equivalents are wired in.
   - The `onMount` call to `initWasm()` has been removed. The new DevPanel simply calls `vaultStatus()` during `onMount`, which internally initializes the WASM module via the vault API. No extra initialization is required on the frontend anymore.
3. **`packages/wallet/src/lib/stores/wallet.ts`**
   - The import of `exportWalletToFile`/`importWalletFromFile` from `../wasm` is commented out with guidance to move these flows into `vaultApi` helpers once they exist.
   - `exportToBlob` / `importFromBlob` now throw explicit errors explaining they’re disabled until the vault-backed export/import paths exist.
   - `sendTransaction` short-circuits with an error so no code references the removed `../wasm` helpers. Designers should reimplement this via the new vault-backed transaction flow.
4. **`packages/wallet/src/lib/components/organisms/WalletManagement.svelte`**
   - The `import { createWalletFromWasm, parseSeedPhrase, getAddressesFromKeys } from '../../wasm';` line is commented with a note directing future work to the vault API.
   - `handleCreateWallet` now just displays an alert saying the feature is disabled pending the migration.
5. **`packages/wallet/src/components/dev/DevPanel.svelte`**
   - Updated import to point at `../../vaultApi`, ensuring dev tooling uses the new plumbing.

   - A temporary "Show/Hide Dev Panel" button was added to `packages/wallet/src/App.svelte` so designers can hide the panel quickly. Remove it once the DevPanel is no longer required.
Until these spots are rebuilt against `vaultApi.ts`, the UI will show alerts instead of invoking the removed helpers. Use this file as a checklist when re‑implementing wallet flows via the new vault API stack.