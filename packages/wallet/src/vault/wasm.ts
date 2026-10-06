// Responsible for loading the WASM bundle and exposing vault-specific bindings.

import init, {
  start as wasmStart,
  buildVault as wasmBuildVault,
  checkPassword as wasmCheckPassword,
  unlockVault as wasmUnlockVault,
  unlockVaultWithVmk as wasmUnlockVaultWithVmk,
  exportVmk as wasmExportVmk,
  lockVault as wasmLockVault,
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
  getBaseFeePerWord as wasmGetBaseFeePerWord,
  cheetahPkhB58 as wasmCheetahPkhB58
} from '../pkg/nockster_core.js';
import type { NoteV1 } from '../lib/utils/rpc';
import type {
  StoredVault,
  UnlockVaultResponse,
  CheckPasswordResponse,
  GeneratedKeyResponse,
  ImportWalletResponse,
  RenameWalletResponse,
  DeleteWalletResponse,
  GetPubkeyResponse,
  GetWalletsResponse,
  ExportWalletResponse,
  TxRecipient
} from './types';

let vaultInitPromise: Promise<void> | null = null;

function getWasmUrl() {
  try {
    if (typeof import.meta !== 'undefined' && import.meta.url) {
      return new URL('../pkg/nockster_core_bg.wasm', import.meta.url).href;
    }
  } catch (error) {
    console.warn('[VaultWasm] failed to resolve wasm via import.meta', error);
  }

  if (typeof chrome !== 'undefined' && chrome.runtime?.getURL) {
    return chrome.runtime.getURL('pkg/nockster_core_bg.wasm');
  }

  return './pkg/nockster_core_bg.wasm';
}

export async function ensureVaultReady() {
  if (!vaultInitPromise) {
    vaultInitPromise = (async () => {
      const wasmUrl = getWasmUrl();
      console.info('[VaultWasm] loading wasm from', wasmUrl);
      await init({ module_or_path: wasmUrl });
      wasmStart();
      console.info('[VaultWasm] initialized');
    })().catch(error => {
      vaultInitPromise = null;
      throw error;
    });
  }

  return vaultInitPromise;
}

export function buildVault(password: string) {
  return wasmBuildVault(password);
}

export function checkPassword(
  password: string,
  vault: StoredVault,
  vaultKey: number
): CheckPasswordResponse {
  return wasmCheckPassword(password, vault, vaultKey);
}

export function unlockVault(password: string, vault: StoredVault): UnlockVaultResponse {
  return wasmUnlockVault(password, vault);
}

export function unlockVaultWithVmk(vmkBase64: string, vault: StoredVault): UnlockVaultResponse {
  return wasmUnlockVaultWithVmk(vmkBase64, vault);
}

export function exportVmk(vaultKey: number): string {
  return wasmExportVmk(vaultKey);
}

export function lockVault(vault: StoredVault, vaultKey: number) {
  return wasmLockVault(vault, vaultKey);
}

export function generateKey(): GeneratedKeyResponse {
  return wasmGenerateKey() as GeneratedKeyResponse;
}

export function validateWalletKey(key: string): string {
  return wasmValidateWalletKey(key);
}

export function importWallet(
  vault: StoredVault,
  vaultKey: number,
  nickname: string,
  key: string
): ImportWalletResponse {
  return wasmImportWallet(vault, vaultKey, nickname, key) as ImportWalletResponse;
}

export function renameWallet(
  vault: StoredVault,
  vaultKey: number,
  oldNickname: string,
  newNickname: string
): RenameWalletResponse {
  return wasmRenameWallet(vault, vaultKey, oldNickname, newNickname) as RenameWalletResponse;
}

export function deleteWallet(
  vault: StoredVault,
  vaultKey: number,
  nickname: string
): DeleteWalletResponse {
  return wasmDeleteWallet(vault, vaultKey, nickname) as DeleteWalletResponse;
}

export function getPubkey(
  vault: StoredVault,
  vaultKey: number,
  nickname: string
): GetPubkeyResponse {
  return wasmGetPubkey(vault, vaultKey, nickname) as GetPubkeyResponse;
}

export function getWallets(vault: StoredVault, vaultKey: number): GetWalletsResponse {
  return wasmGetWallets(vault, vaultKey) as GetWalletsResponse;
}

export function exportWallet(
  vault: StoredVault,
  vaultKey: number,
  nickname: string
): ExportWalletResponse {
  return wasmExportWallet(vault, vaultKey, nickname) as ExportWalletResponse;
}

export function createAndSignTx(
  vault: StoredVault,
  vaultKey: number,
  nickname: string,
  notes: NoteV1[],
  recipients: TxRecipient[]
): unknown {
  return wasmCreateAndSignTx(vault, vaultKey, nickname, notes, recipients);
}

export function createAndSignTxWithOptions(
  vault: StoredVault,
  vaultKey: number,
  nickname: string,
  notes: NoteV1[],
  recipients: TxRecipient[],
  privateOutputs: boolean,
  chainHeight?: number
): unknown {
  return wasmCreateAndSignTxWithOptions(
    vault,
    vaultKey,
    nickname,
    notes,
    recipients,
    privateOutputs,
    chainHeight
  );
}

export function inspectTxJam(base64Jam: string): unknown {
  return wasmInspectTxJam(base64Jam);
}

export function signTxJam(
  vault: StoredVault,
  vaultKey: number,
  nickname: string,
  base64Jam: string
): unknown {
  return wasmSignTxJam(vault, vaultKey, nickname, base64Jam);
}

export function signTxJamSelected(
  vault: StoredVault,
  vaultKey: number,
  nickname: string,
  base64Jam: string,
  selectedSpends: string[]
): unknown {
  return wasmSignTxJamSelected(vault, vaultKey, nickname, base64Jam, selectedSpends);
}

export function toRawTxJam(base64Jam: string): string {
  return wasmToRawTxJam(base64Jam) as string;
}

export function toWalletTxJam(base64Jam: string): string {
  return wasmToWalletTxJam(base64Jam) as string;
}

/**
 * Get the minimum network fee in nicks
 */
export function getMinNetworkFee(): bigint {
  return wasmGetMinNetworkFee();
}

/**
 * Get the base fee per word in nicks (fee = words * BASE_FEE)
 */
export function getBaseFeePerWord(): bigint {
  return wasmGetBaseFeePerWord();
}

export function cheetahPkhB58(pubkeyX: string[], pubkeyY: string[]): string {
  return wasmCheetahPkhB58(pubkeyX, pubkeyY) as string;
}
