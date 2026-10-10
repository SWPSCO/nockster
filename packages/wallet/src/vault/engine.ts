import { initializeIrisV1, signIrisV1 } from '../platform/irisV1';
import { vaultStorage, vaultWasmUrl } from '../platform/vault';
// Serialized vault operations shared by the extension worker and mobile runtime.
// The unlocked key remains in WASM memory.

import init, {
  start as wasmStart,
  sealSecret as wasmSealSecret,
  openSecret as wasmOpenSecret,
  buildVault as wasmBuildVault,
  checkPassword as wasmCheckPassword,
  unlockVault as wasmUnlockVault,
  unlockVaultWithVmk as wasmUnlockVaultWithVmk,
  exportVmk as wasmExportVmk,
  lockVault as wasmLockVault,
  freeVaultKey as wasmFreeVaultKey,
  generateKey as wasmGenerateKey,
  importWallet as wasmImportWallet,
  validateWalletKey as wasmValidateWalletKey,
  renameWallet as wasmRenameWallet,
  deleteWallet as wasmDeleteWallet,
  getPubkey as wasmGetPubkey,
  getWallets as wasmGetWallets,
  exportWallet as wasmExportWallet,
  createAndSignTx as wasmCreateAndSignTx,
  createAndSignTxWithOptions as wasmCreateAndSignTxWithOptions,
  inspectTxJam as wasmInspectTxJam,
  signTxJam as wasmSignTxJam,
  signTxJamSelected as wasmSignTxJamSelected,
  toRawTxJam as wasmToRawTxJam,
  toWalletTxJam as wasmToWalletTxJam,
  getMinNetworkFee as wasmGetMinNetworkFee,
  getBaseFeePerWord as wasmGetBaseFeePerWord
} from '../pkg/nockster_core.js';
import {
  isStoredVault,
  isVaultBuildResponse,
  isUnlockVaultResponse,
  isCheckPasswordResponse,
  isGeneratedKeyResponse,
  isImportWalletResponse,
  isRenameWalletResponse,
  isDeleteWalletResponse,
  isGetPubkeyResponse,
  isGetWalletsResponse,
  isExportWalletResponse,
  isTransactionResult,
  isJamInspectResult,
  isJamSignResult
} from './types';
import type { StoredVault } from './types';

// Opaque handle to the key held in WASM memory
let vaultKey: number | null = null;
let wasmInitPromise: Promise<void> | null = null;
let lastActivityTime: number = Date.now();
let autoLockTimeoutMinutes: number = 5; // Default, will be updated from settings

const VAULT_STORAGE_KEY = 'fletch_vault_v1';
const SETTINGS_STORAGE_KEY = 'settings';

// Update last activity time on each vault operation
function updateActivity() {
  lastActivityTime = Date.now();
}

// Check if we should auto-lock based on inactivity
export function checkAutoLock(): boolean {
  // In-process clients enforce their session lifetime through the app lifecycle
  // and user activity tracking; the extension worker tracks vault activity.
  if (['mobile', 'desktop', 'urbit'].includes(import.meta.env.MODE)) return false;
  if (vaultKey === null || autoLockTimeoutMinutes === 0) {
    return false; // Already locked or auto-lock disabled
  }

  const elapsed = Date.now() - lastActivityTime;
  const timeoutMs = autoLockTimeoutMinutes * 60 * 1000;

  if (elapsed >= timeoutMs) {
    console.log('[VaultWorker] Auto-locking due to inactivity');
    wasmFreeVaultKey(vaultKey);
    vaultKey = null;
    return true;
  }
  return false;
}

// Load auto-lock timeout from settings
async function loadAutoLockSetting(): Promise<void> {
  try {
    const result = await vaultStorage.get([SETTINGS_STORAGE_KEY]);
    const settings = result[SETTINGS_STORAGE_KEY] as { autoLockTimeout?: number } | undefined;
    if (settings && typeof settings.autoLockTimeout === 'number') {
      autoLockTimeoutMinutes = settings.autoLockTimeout;
    }
  } catch (err) {
    console.warn('[VaultWorker] Failed to load auto-lock setting:', err);
  }
}

async function ensureWasmReady(): Promise<void> {
  if (!wasmInitPromise) {
    wasmInitPromise = (async () => {
      const wasmUrl = vaultWasmUrl();
      console.info('[VaultWorker] loading wasm from', wasmUrl);
      await init({ module_or_path: wasmUrl });
      wasmStart();
      console.info('[VaultWorker] WASM initialized');
    })().catch(error => {
      wasmInitPromise = null;
      throw error;
    });
  }
  return wasmInitPromise;
}

async function fetchStoredVault(): Promise<StoredVault | null> {
  const result = await vaultStorage.get([VAULT_STORAGE_KEY]);
  const candidate = result[VAULT_STORAGE_KEY];
  if (candidate === undefined || candidate === null) return null;
  if (!isStoredVault(candidate))
    throw new Error('Stored vault is invalid; preserve its data for recovery');
  return candidate;
}

async function persistStoredVault(vault: StoredVault): Promise<void> {
  await vaultStorage.set({ [VAULT_STORAGE_KEY]: vault });
}

async function deleteStoredVault(): Promise<void> {
  await vaultStorage.remove([VAULT_STORAGE_KEY]);
}

// Message handlers for vault operations
export type VaultMessage =
  | { type: 'vault:status' }
  | { type: 'vault:signIrisV1'; nickname: string; address: string; message: string }
  | { type: 'vault:sealSecret'; context: string; value: string }
  | { type: 'vault:openSecret'; context: string; envelope: string }
  | { type: 'vault:suspend' }
  | { type: 'vault:exportUnlockKey'; password: string }
  | { type: 'vault:unlockWithDeviceKey'; key: string }
  | { type: 'vault:checkDeviceKey'; key: string }
  | { type: 'vault:newVault'; password: string }
  | { type: 'vault:unlock'; password: string }
  | { type: 'vault:lock' }
  | { type: 'vault:wipe' }
  | { type: 'vault:checkPassword'; password: string }
  | { type: 'vault:showLogs' }
  | { type: 'vault:generateKey' }
  | { type: 'vault:validateKey'; key: string }
  | { type: 'vault:importWallet'; nickname: string; key: string }
  | { type: 'vault:renameWallet'; oldNickname: string; newNickname: string }
  | { type: 'vault:deleteWallet'; nickname: string }
  | { type: 'vault:getPubkey'; nickname: string }
  | { type: 'vault:getWallets' }
  | { type: 'vault:exportWallet'; nickname: string }
  | { type: 'vault:createAndSignTx'; nickname: string; notes: unknown[]; recipients: unknown[] }
  | {
      type: 'vault:createAndSignTxWithOptions';
      nickname: string;
      notes: unknown[];
      recipients: unknown[];
      privateOutputs: boolean;
      chainHeight?: number;
    }
  | { type: 'vault:inspectTxJam'; base64Jam: string }
  | { type: 'vault:signTxJam'; nickname: string; base64Jam: string }
  | {
      type: 'vault:signTxJamSelected';
      nickname: string;
      base64Jam: string;
      selectedSpends: string[];
    }
  | { type: 'vault:toRawTxJam'; base64Jam: string }
  | { type: 'vault:toWalletTxJam'; base64Jam: string }
  | { type: 'vault:getMinNetworkFee' }
  | { type: 'vault:getBaseFeePerWord' };

let vaultQueue: Promise<unknown> = Promise.resolve();

export function handleVaultMessage(
  message: VaultMessage
): Promise<{ success: boolean; data?: unknown; error?: string }> {
  const operation = vaultQueue.then(() => processVaultMessage(message));
  vaultQueue = operation.catch(() => undefined);
  return operation;
}

async function processVaultMessage(
  message: VaultMessage
): Promise<{ success: boolean; data?: unknown; error?: string }> {
  try {
    await ensureWasmReady();
    await loadAutoLockSetting();

    // Check auto-lock before processing (except for status check)
    if (message.type !== 'vault:status') {
      checkAutoLock();
    }

    // Update activity time for most operations
    if (message.type !== 'vault:status') {
      updateActivity();
    }

    switch (message.type) {
      case 'vault:signIrisV1': {
        if (vaultKey === null) throw new Error('Unlock your wallet first');
        await initializeIrisV1();
        const stored = await fetchStoredVault();
        if (!stored || vaultKey === null) throw new Error('Unlock your wallet first');
        const exported = wasmExportWallet(stored, vaultKey, message.nickname);
        if (!isExportWalletResponse(exported)) throw new Error('Invalid wallet');
        try {
          return {
            success: true,
            data: signIrisV1(exported.wallet.privateKey, message.address, message.message)
          };
        } finally {
          exported.wallet.privateKey = '';
          exported.wallet.extendedPrivateKey = '';
          exported.wallet.seedphrase = null;
        }
      }
      case 'vault:sealSecret':
      case 'vault:openSecret': {
        if (vaultKey === null) throw new Error('Unlock your wallet first');
        return {
          success: true,
          data:
            message.type === 'vault:sealSecret'
              ? wasmSealSecret(vaultKey, message.context, message.value)
              : wasmOpenSecret(vaultKey, message.context, message.envelope)
        };
      }
      case 'vault:status': {
        // Check auto-lock on status check too
        checkAutoLock();
        const vault = await fetchStoredVault();
        return {
          success: true,
          data: { exists: Boolean(vault), unlocked: vaultKey !== null }
        };
      }

      case 'vault:newVault': {
        if (await fetchStoredVault()) {
          return { success: false, error: 'Vault already exists' };
        }
        const response = wasmBuildVault(message.password);
        if (!isVaultBuildResponse(response)) {
          return { success: false, error: 'Invalid vault payload from WASM' };
        }
        await persistStoredVault(response.vault);
        vaultKey = response.vaultKey;
        return { success: true };
      }

      case 'vault:exportUnlockKey': {
        if (import.meta.env.MODE !== 'mobile' || vaultKey === null) {
          return { success: false, error: 'Device unlock requires an unlocked mobile vault' };
        }
        const vault = await fetchStoredVault();
        if (!vault) return { success: false, error: 'Vault data missing' };
        const checked = wasmCheckPassword(message.password, vault, vaultKey);
        if (!isCheckPasswordResponse(checked) || !checked.matches) {
          return { success: false, error: 'Incorrect password' };
        }
        return { success: true, data: { key: wasmExportVmk(vaultKey) } };
      }

      case 'vault:checkDeviceKey': {
        if (import.meta.env.MODE !== 'mobile' || vaultKey === null)
          return { success: false, error: 'Unlock the wallet before confirming a payment' };
        const vault = await fetchStoredVault();
        if (!vault) return { success: false, error: 'Vault data missing' };
        const checked = wasmUnlockVaultWithVmk(message.key, vault);
        if (!isUnlockVaultResponse(checked))
          return { success: false, error: 'Device authentication failed' };
        wasmFreeVaultKey(checked.vaultKey);
        return { success: true };
      }

      case 'vault:unlockWithDeviceKey': {
        if (import.meta.env.MODE !== 'mobile')
          return { success: false, error: 'Mobile operation required' };
        const vault = await fetchStoredVault();
        if (!vault) return { success: false, error: 'Vault data missing' };
        const response = wasmUnlockVaultWithVmk(message.key, vault);
        if (!isUnlockVaultResponse(response))
          return { success: false, error: 'Invalid device unlock response' };
        await persistStoredVault(response.vault);
        if (vaultKey !== null) wasmFreeVaultKey(vaultKey);
        vaultKey = response.vaultKey;
        return { success: true, data: { wallets: response.wallets } };
      }

      case 'vault:unlock': {
        const vault = await fetchStoredVault();
        if (!vault) {
          return { success: false, error: 'Vault data missing' };
        }
        const response = wasmUnlockVault(message.password, vault);
        if (!isUnlockVaultResponse(response)) {
          return { success: false, error: 'Invalid unlock response from WASM' };
        }
        await persistStoredVault(response.vault);
        if (vaultKey !== null) wasmFreeVaultKey(vaultKey);
        vaultKey = response.vaultKey;
        return { success: true, data: { wallets: response.wallets } };
      }

      case 'vault:suspend': {
        if (vaultKey !== null) wasmFreeVaultKey(vaultKey);
        vaultKey = null;
        return { success: true };
      }

      case 'vault:lock': {
        if (vaultKey === null) {
          return { success: false, error: 'Vault is already locked' };
        }
        const vault = await fetchStoredVault();
        if (!vault) {
          return { success: false, error: 'Vault data missing' };
        }
        const response = wasmLockVault(vault, vaultKey);
        if (!response || !Array.isArray(response.log)) {
          return { success: false, error: 'Invalid lock response from WASM' };
        }
        const updatedVault = { ...vault, log: response.log };
        await persistStoredVault(updatedVault);
        vaultKey = null;
        return { success: true };
      }

      case 'vault:wipe': {
        if (vaultKey !== null) wasmFreeVaultKey(vaultKey);
        vaultKey = null;
        await deleteStoredVault();
        vaultKey = null;
        return { success: true };
      }

      case 'vault:checkPassword': {
        if (vaultKey === null) {
          return { success: false, error: 'Vault is locked' };
        }
        const vault = await fetchStoredVault();
        if (!vault) {
          return { success: false, error: 'Vault data missing' };
        }
        const response = wasmCheckPassword(message.password, vault, vaultKey);
        if (!isCheckPasswordResponse(response)) {
          return { success: false, error: 'Invalid checkPassword response from WASM' };
        }
        await persistStoredVault(response.vault);
        return { success: true, data: { matches: response.matches } };
      }

      case 'vault:showLogs': {
        const vault = await fetchStoredVault();
        if (!vault) {
          return { success: false, error: 'Vault data missing' };
        }
        return { success: true, data: { logs: vault.log ?? [] } };
      }

      case 'vault:generateKey': {
        const response = wasmGenerateKey();
        if (!isGeneratedKeyResponse(response)) {
          return { success: false, error: 'Invalid generateKey response from WASM' };
        }
        return { success: true, data: { mnemonic: response } };
      }

      case 'vault:validateKey':
        return { success: true, data: { address: wasmValidateWalletKey(message.key) } };

      case 'vault:importWallet': {
        if (vaultKey === null) {
          return { success: false, error: 'Vault is locked' };
        }
        const vault = await fetchStoredVault();
        if (!vault) {
          return { success: false, error: 'Vault data missing' };
        }
        const response = wasmImportWallet(vault, vaultKey, message.nickname, message.key);
        if (!isImportWalletResponse(response)) {
          return { success: false, error: 'Invalid importWallet response from WASM' };
        }
        await persistStoredVault(response.vault);
        vaultKey = response.vaultKey;
        return { success: true, data: { wallet: response.wallet } };
      }

      case 'vault:renameWallet': {
        if (vaultKey === null) {
          return { success: false, error: 'Vault is locked' };
        }
        const vault = await fetchStoredVault();
        if (!vault) {
          return { success: false, error: 'Vault data missing' };
        }
        const response = wasmRenameWallet(
          vault,
          vaultKey,
          message.oldNickname,
          message.newNickname
        );
        if (!isRenameWalletResponse(response)) {
          return { success: false, error: 'Invalid renameWallet response from WASM' };
        }
        await persistStoredVault(response.vault);
        vaultKey = response.vaultKey;
        return { success: true, data: { wallet: response.wallet } };
      }

      case 'vault:deleteWallet': {
        if (vaultKey === null) {
          return { success: false, error: 'Vault is locked' };
        }
        const vault = await fetchStoredVault();
        if (!vault) {
          return { success: false, error: 'Vault data missing' };
        }
        const response = wasmDeleteWallet(vault, vaultKey, message.nickname);
        if (!isDeleteWalletResponse(response)) {
          return { success: false, error: 'Invalid deleteWallet response from WASM' };
        }
        await persistStoredVault(response.vault);
        vaultKey = response.vaultKey;
        return { success: true, data: { deleted: response.deleted } };
      }

      case 'vault:getPubkey': {
        if (vaultKey === null) {
          return { success: false, error: 'Vault is locked' };
        }
        const vault = await fetchStoredVault();
        if (!vault) {
          return { success: false, error: 'Vault data missing' };
        }
        const response = wasmGetPubkey(vault, vaultKey, message.nickname);
        if (!isGetPubkeyResponse(response)) {
          return { success: false, error: 'Invalid getPubkey response from WASM' };
        }
        await persistStoredVault(response.vault);
        vaultKey = response.vaultKey;
        return { success: true, data: { wallet: response.wallet } };
      }

      case 'vault:getWallets': {
        if (vaultKey === null) {
          return { success: false, error: 'Vault is locked' };
        }
        const vault = await fetchStoredVault();
        if (!vault) {
          return { success: false, error: 'Vault data missing' };
        }
        const response = wasmGetWallets(vault, vaultKey);
        if (!isGetWalletsResponse(response)) {
          return { success: false, error: 'Invalid getWallets response from WASM' };
        }
        await persistStoredVault(response.vault);
        vaultKey = response.vaultKey;
        return { success: true, data: { wallets: response.wallets } };
      }

      case 'vault:exportWallet': {
        if (vaultKey === null) {
          return { success: false, error: 'Vault is locked' };
        }
        const vault = await fetchStoredVault();
        if (!vault) {
          return { success: false, error: 'Vault data missing' };
        }
        const response = wasmExportWallet(vault, vaultKey, message.nickname);
        if (!isExportWalletResponse(response)) {
          return { success: false, error: 'Invalid exportWallet response from WASM' };
        }
        await persistStoredVault(response.vault);
        vaultKey = response.vaultKey;
        return { success: true, data: { wallet: response.wallet } };
      }

      case 'vault:createAndSignTx': {
        if (vaultKey === null) {
          return { success: false, error: 'Vault is locked' };
        }
        const vault = await fetchStoredVault();
        if (!vault) {
          return { success: false, error: 'Vault data missing' };
        }
        const response = wasmCreateAndSignTx(
          vault,
          vaultKey,
          message.nickname,
          message.notes as Parameters<typeof wasmCreateAndSignTx>[3],
          message.recipients as Parameters<typeof wasmCreateAndSignTx>[4]
        );
        if (!isTransactionResult(response)) {
          return { success: false, error: 'Invalid createAndSignTx response from WASM' };
        }
        return { success: true, data: response };
      }

      case 'vault:createAndSignTxWithOptions': {
        if (vaultKey === null) {
          return { success: false, error: 'Vault is locked' };
        }
        const vault = await fetchStoredVault();
        if (!vault) {
          return { success: false, error: 'Vault data missing' };
        }
        const response = wasmCreateAndSignTxWithOptions(
          vault,
          vaultKey,
          message.nickname,
          message.notes as Parameters<typeof wasmCreateAndSignTxWithOptions>[3],
          message.recipients as Parameters<typeof wasmCreateAndSignTxWithOptions>[4],
          message.privateOutputs,
          message.chainHeight
        );
        if (!isTransactionResult(response)) {
          return { success: false, error: 'Invalid createAndSignTxWithOptions response from WASM' };
        }
        return { success: true, data: response };
      }

      case 'vault:inspectTxJam': {
        const response = wasmInspectTxJam(message.base64Jam);
        if (!isJamInspectResult(response)) {
          return { success: false, error: 'Invalid inspectTxJam response from WASM' };
        }
        return { success: true, data: response };
      }

      case 'vault:signTxJam': {
        if (vaultKey === null) {
          return { success: false, error: 'Vault is locked' };
        }
        const vault = await fetchStoredVault();
        if (!vault) {
          return { success: false, error: 'Vault data missing' };
        }
        const response = wasmSignTxJam(vault, vaultKey, message.nickname, message.base64Jam);
        if (!isJamSignResult(response)) {
          return { success: false, error: 'Invalid signTxJam response from WASM' };
        }
        return { success: true, data: response };
      }

      case 'vault:signTxJamSelected': {
        if (vaultKey === null) {
          return { success: false, error: 'Vault is locked' };
        }
        const vault = await fetchStoredVault();
        if (!vault) {
          return { success: false, error: 'Vault data missing' };
        }
        const response = wasmSignTxJamSelected(
          vault,
          vaultKey,
          message.nickname,
          message.base64Jam,
          message.selectedSpends
        );
        if (!isJamSignResult(response)) {
          return { success: false, error: 'Invalid signTxJamSelected response from WASM' };
        }
        return { success: true, data: response };
      }

      case 'vault:toRawTxJam': {
        const response = wasmToRawTxJam(message.base64Jam);
        if (typeof response !== 'string' || response.trim().length === 0) {
          return { success: false, error: 'Invalid toRawTxJam response from WASM' };
        }
        return { success: true, data: response };
      }

      case 'vault:toWalletTxJam': {
        const response = wasmToWalletTxJam(message.base64Jam);
        if (typeof response !== 'string' || response.trim().length === 0) {
          return { success: false, error: 'Invalid toWalletTxJam response from WASM' };
        }
        return { success: true, data: response };
      }

      case 'vault:getMinNetworkFee': {
        const fee = wasmGetMinNetworkFee();
        return { success: true, data: { fee: fee.toString() } };
      }

      case 'vault:getBaseFeePerWord': {
        const fee = wasmGetBaseFeePerWord();
        return { success: true, data: { fee: fee.toString() } };
      }

      default:
        return { success: false, error: 'Unknown vault message type' };
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    console.error('[VaultWorker] Error:', message);
    return { success: false, error: message };
  }
}
