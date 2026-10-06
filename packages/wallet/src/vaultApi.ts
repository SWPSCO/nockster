import {
  vaultStatus as controllerStatus,
  newVault as controllerNewVault,
  wipeVault as controllerWipeVault,
  checkPassword as controllerCheckPassword,
  unlockVault as controllerUnlockVault,
  lockVault as controllerLockVault,
  showLogs as controllerShowLogs,
  generateKey as controllerGenerateKey,
  importWallet as controllerImportWallet,
  renameWallet as controllerRenameWallet,
  deleteWallet as controllerDeleteWallet,
  getPubkey as controllerGetPubkey,
  getWallets as controllerGetWallets,
  exportWallet as controllerExportWallet,
  createAndSignTx as controllerCreateAndSignTx,
  createAndSignTxWithOptions as controllerCreateAndSignTxWithOptions,
  inspectTxJam as controllerInspectTxJam,
  signTxJam as controllerSignTxJam,
  signTxJamSelected as controllerSignTxJamSelected,
  toRawTxJam as controllerToRawTxJam,
  toWalletTxJam as controllerToWalletTxJam,
  getMinNetworkFee as controllerGetMinNetworkFee,
  getBaseFeePerWord as controllerGetBaseFeePerWord
} from './vaultController';
import type { NoteV1 } from './lib/utils/rpc';
import type {
  TxRecipient,
  TransactionResult,
  JamInspectResult,
  JamSignResult
} from './vault/types';

export type { TxRecipient, TransactionResult };

export interface VaultStatusPayload {
  exists: boolean;
  unlocked: boolean;
}

export type ApiResponse<T> = {
  success: boolean;
  data?: T;
  error?: string;
};

function formatError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

export async function vaultStatus(): Promise<ApiResponse<VaultStatusPayload>> {
  try {
    const data = await controllerStatus();
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function newVault(localPassword: string): Promise<ApiResponse<{ success: boolean }>> {
  try {
    const data = await controllerNewVault(localPassword);
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export type WalletSummaryPayload = {
  nickname: string;
  publicKey: string;
  extendedPublicKey: string | null;
};

export type WalletPayload = {
  publicKey: string;
  extendedPublicKey: string | null;
  privateKey: string;
  extendedPrivateKey: string | null;
  chainCode: number[] | null;
  depth: number | null;
  index: number | null;
  parentFingerprint: number[] | null;
  version: number | null;
  seedphrase: string[] | null;
};

export async function unlockVault(
  localPassword: string
): Promise<ApiResponse<WalletSummaryPayload[]>> {
  try {
    const data = await controllerUnlockVault(localPassword);
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function lockVault(): Promise<ApiResponse<{ success: boolean }>> {
  try {
    const data = await controllerLockVault();
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function wipeVault(): Promise<ApiResponse<{ success: boolean }>> {
  try {
    const data = await controllerWipeVault();
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function checkPassword(localPassword: string): Promise<ApiResponse<boolean>> {
  try {
    const data = await controllerCheckPassword(localPassword);
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function showLogs(): Promise<
  ApiResponse<Array<{ timestamp: number; operation: string }>>
> {
  try {
    const data = await controllerShowLogs();
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function generateKey(): Promise<ApiResponse<string[]>> {
  try {
    const data = await controllerGenerateKey();
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function importWallet(
  nickname: string,
  key: string
): Promise<ApiResponse<WalletSummaryPayload>> {
  try {
    const data = await controllerImportWallet(nickname, key);
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function renameWallet(
  oldNickname: string,
  newNickname: string
): Promise<ApiResponse<WalletSummaryPayload>> {
  try {
    const data = await controllerRenameWallet(oldNickname, newNickname);
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function deleteWallet(nickname: string): Promise<ApiResponse<boolean>> {
  try {
    const data = await controllerDeleteWallet(nickname);
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function getPubkey(nickname: string): Promise<ApiResponse<WalletSummaryPayload>> {
  try {
    const data = await controllerGetPubkey(nickname);
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function getWallets(): Promise<ApiResponse<WalletSummaryPayload[]>> {
  try {
    const data = await controllerGetWallets();
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function exportWallet(nickname: string): Promise<ApiResponse<WalletPayload>> {
  try {
    const data = await controllerExportWallet(nickname);
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

/*
 * Transaction workflows
 */

export async function createAndSignTx(
  nickname: string,
  notes: NoteV1[],
  recipients: TxRecipient[]
): Promise<ApiResponse<TransactionResult>> {
  try {
    const data = await controllerCreateAndSignTx(nickname, notes, recipients);
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function createAndSignTxWithOptions(
  nickname: string,
  notes: NoteV1[],
  recipients: TxRecipient[],
  options: { privateOutputs?: boolean; chainHeight?: number } = {}
): Promise<ApiResponse<TransactionResult>> {
  try {
    const data = await controllerCreateAndSignTxWithOptions(nickname, notes, recipients, options);
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function inspectTxJam(base64Jam: string): Promise<ApiResponse<JamInspectResult>> {
  try {
    const data = await controllerInspectTxJam(base64Jam);
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function signTxJam(
  nickname: string,
  base64Jam: string
): Promise<ApiResponse<JamSignResult>> {
  try {
    const data = await controllerSignTxJam(nickname, base64Jam);
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function signTxJamSelected(
  nickname: string,
  base64Jam: string,
  selectedSpends: string[]
): Promise<ApiResponse<JamSignResult>> {
  try {
    const data = await controllerSignTxJamSelected(nickname, base64Jam, selectedSpends);
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function toRawTxJam(base64Jam: string): Promise<ApiResponse<string>> {
  try {
    const data = await controllerToRawTxJam(base64Jam);
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

export async function toWalletTxJam(base64Jam: string): Promise<ApiResponse<string>> {
  try {
    const data = await controllerToWalletTxJam(base64Jam);
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

/**
 * Get the minimum network fee in nicks
 */
export async function getMinNetworkFee(): Promise<ApiResponse<bigint>> {
  try {
    const data = await controllerGetMinNetworkFee();
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}

/**
 * Get the base fee per word in nicks
 */
export async function getBaseFeePerWord(): Promise<ApiResponse<bigint>> {
  try {
    const data = await controllerGetBaseFeePerWord();
    return { success: true, data };
  } catch (error) {
    return { success: false, error: formatError(error) };
  }
}
