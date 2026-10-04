# Vault API

JavaScript helpers for interacting with Nockster's wallet engine. Use these calls to manage private keys and construct wallet transactions.

## Architecture Overview

```
frontend Svelte/TS → packages/wallet/src/vaultApi.ts → packages/wallet/src/vaultController.ts → chrome.storage + WASM bridge → packages/wallet-engine/src/lib.rs (wasm_bindgen) → packages/wallet-engine/src/vault.rs / vmk_slot.rs
```

- `packages/wallet/src/vaultApi.ts` is the public surface exposed to UI code. Every helper returns a normalized `{ success, data?, error? }` envelope and never touches raw vault bytes or VMKs.
- `packages/wallet/src/vaultController.ts` owns the decrypted vault key. It is the **only** JS module allowed to hold the VMK slot handle (`vaultKey`). The actual VMK lives in WASM memory inside `packages/wallet-engine/src/vmk_slot.rs`. When the controller calls into WASM it passes the slot ID; the Rust side looks up the VMK internally.
- `packages/wallet-engine/src/vault.rs` handles all cryptography. Vault data is serialized to JSON, encrypted with the VMK using ChaCha20-Poly1305, and Base64-encoded before crossing the WASM boundary. The reverse happens when unlocking.
- The vault itself never leaves the controller layer: API callers only see summaries (nickname/publicKey) or logging metadata pulled from storage.

### VMK Slot (Vault Key) Model

- When a vault is created/unlocked, Rust allocates a slot ID via `VmkSlots::allocate` and stores the VMK in a guarded map. The JS controller stores only the slot ID (`vaultKey`), not the key bytes.
- Locking or wiping the vault calls `VmkSlots::free`, zeroizing the VMK and invalidating the slot. Any subsequent WASM call must either re-unlock or will fail.

### Encryption / Decryption Flow

1. **Encryption (Rust)**: `VaultData` → JSON → `ChaCha20Poly1305` encrypt (nonce‖ciphertext‖tag) → Base64 string stored in `VaultExtern`.
2. **Decryption (Rust)**: Base64 → split nonce/tag → decrypt with VMK retrieved from slot → deserialize JSON back into `VaultData`.
3. **Controller**: Only ever stores/forwards the encrypted Base64 blob.

### Logging Requirements

- Every `#[wasm_bindgen]` function **must** record an operation in the vault log (`packages/wallet-engine/src/vault.rs::log_operation` / `VaultExtern::append_log`). Examples already in place: `"BUILD_VAULT"`, `"UNLOCK"`, `"LOCK"`, `"CHECK_PASSWORD"`.
- **Do not log outside** WASM entrypoints; keep log mutations centralized so they remain auditable and consistent.

### Locked vs. Unlocked Calls

- Any operation that requires VMK access **must** verify the vault is unlocked (i.e., `vaultKeyStore` holds a slot). Attempting to call WASM without an unlocked vault should throw at the controller layer.
- **Exceptions** (allowed while locked, already implemented):
  - `vaultStatus` (pure storage read)
  - `newVault` (creates the vault)
  - `unlockVault` (derives the VMK slot)
  - `lockVault` (frees the slot)
  - `wipeVault` (clears storage)
  - `showLogs` (reads logs only)
  - `checkPassword` (requires unlocked slot but is explicitly guarded in the controller)

## Example Usage

```svelte
<script>
  import { onMount } from 'svelte';
  import { vaultStatus } from './vaultApi';

  onMount(async () => {
    status = await vaultStatus();
  });
</script>
```

## API Reference

### `vaultStatus()`

- Returns the current state of the vault.
- Useful for checking whether this is a fresh install or if the user has unlocked the wallet.

**Returns**

```ts
{
    success: boolean,
    data?: {
        exists: boolean,
        unlocked: boolean
    },
    error?: string
}
```

### `newVault(localPassword: string)`

- Creates a new vault secured with the provided local password.
- After a successful call, running `vaultStatus()` should show both `exists` and `unlocked` as true.

**Returns**

```ts
{
    success: boolean,
    data?: { success: boolean },
    error?: string
}
```

### `wipeVault()`

- Deletes the existing vault, effectively performing a full reset.

**Returns**

```ts
{
    success: boolean,
    data?: { success: boolean },
    error?: string
}
```

### `unlockVault(localPassword: string)`

- Decrypts the existing vault using the provided password.
- Persists the refreshed vault state (e.g., log entries) and keeps the decrypted key in memory for subsequent operations.
- Returns high-level metadata about wallets stored in the vault without exposing sensitive material.

**Returns**

```ts
{
    success: boolean,
    data?: Array<{
        nickname: string;
        publicKey: string;
        extendedPublicKey: string;
    }>,
    error?: string
}
```

### `lockVault()`

- Clears the decrypted vault key from memory and zeroizes the VMK slot inside WASM.
- Adds a `LOCK` entry to the vault log to reflect the transition.

**Returns**

```ts
{
    success: boolean,
    data?: { success: boolean },
    error?: string
}
```

### `checkPassword(localPassword: string)`

- Validates the provided password against the currently unlocked vault key.
- Updates the vault log with a `CHECK_PASSWORD` entry.

**Returns**

```ts
{
    success: boolean,
    data?: boolean, // true when the password matches
    error?: string
}
```

### `showLogs()`

- Reads the vault log directly from local storage (no WASM calls).
- Useful for displaying auditing information in the UI.

**Returns**

```ts
{
    success: boolean,
    data?: Array<{
        timestamp: number,
        operation: string
    }>,
    error?: string
}
```

## Wallet Management APIs

### `generateKey()`

- Uses the WASM helper exposed via `Wallet::generate_key()` to produce a 24-word mnemonic derived from Argon2id entropy.
- Safe to call even when the vault is locked, since it does not touch VMK material.

**Returns**

```ts
{
    success: boolean,
    data?: string[], // always 24 entries
    error?: string
}
```

### `importWallet(nickname: string, key: string)`

- Adds a wallet entry to the encrypted vault data using either an extended private key or a 24-word seed phrase.
- WASM enforces two safety checks before writing to the vault:
  - Nicknames must be unique. Attempting to reuse a nickname throws `wallet already exists`.
  - Private keys must be unique. If the derived wallet would reuse an existing private key, the error identifies the conflicting nickname.
- Requires the vault to be unlocked; otherwise the controller rejects the call before reaching WASM.

**Returns**

```ts
{
    success: boolean,
    data?: {
        nickname: string;
        publicKey: string;
        extendedPublicKey: string;
    },
    error?: string
}
```

### `renameWallet(oldNickname: string, newNickname: string)`

- Renames an existing wallet entry. Fails if the source nickname is missing or the destination nickname already exists.
- Requires an unlocked vault.

**Returns**

```ts
{
    success: boolean,
    data?: {
        nickname: string;
        publicKey: string;
        extendedPublicKey: string;
    },
    error?: string
}
```

### `deleteWallet(nickname: string)`

- Removes a wallet entry from the vault data. Throws if the nickname does not exist.
- Requires an unlocked vault.

**Returns**

```ts
{
    success: boolean,
    data?: boolean, // true indicates the wallet was removed
    error?: string
}
```

### `getPubkey(nickname: string)`

- Retrieves a single wallet summary (nickname, public key, extended public key) without exposing private fields.
- Requires an unlocked vault.

**Returns**

```ts
{
    success: boolean,
    data?: {
        nickname: string;
        publicKey: string;
        extendedPublicKey: string;
    },
    error?: string
}
```

### `getWallets()`

- Returns the same wallet summaries emitted by `unlockVault`, reflecting the vault’s latest state.
- Requires an unlocked vault.

**Returns**

```ts
{
    success: boolean,
    data?: Array<{
        nickname: string;
        publicKey: string;
        extendedPublicKey: string;
    }>,
    error?: string
}
```

### `exportWallet(nickname: string)`

- Exports the full wallet record, including private keys, chain-code metadata, and (when available) the 24-word seedphrase. Imports that begin from an extended private key (`zprv`) currently have `seedphrase: null` because the phrase cannot be recovered.
- Requires an unlocked vault. Consumers must handle the sensitive payload carefully since it contains everything needed to recreate the wallet.

**Returns**

```ts
{
    success: boolean,
    data?: {
        publicKey: string;
        extendedPublicKey: string;
        privateKey: string;
        extendedPrivateKey: string;
        chainCode: number[];           // 32-byte array (Uint8Array on the JS side)
        depth: number;
        index: number;
        parentFingerprint: number[];   // 4 bytes
        version: number;
        seedphrase: string[] | null;   // 24 entries or null when unavailable
    },
    error?: string
}
```

## Planned / Unimplemented APIs

The only remaining helper stub is outlined below.

### `createAndSignTransaction()`

- _Status_: Not implemented.
- Intended to take notes/recipients/gifts, build a transaction, sign it, and return a JAM blob ready for submission.

**Returns**

```ts
// TBD
```
