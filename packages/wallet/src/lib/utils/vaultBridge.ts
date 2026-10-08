/**
 * Vault Bridge Service
 *
 * Bridges the vault API with the Svelte wallet store.
 * The vault API is the authoritative source for wallet data.
 * This service syncs vault state to the wallet store for UI consumption.
 */

import {
  vaultStatus,
  newVault,
  unlockVault,
  lockVault,
  generateKey,
  importWallet,
  renameWallet as vaultRenameWallet,
  deleteWallet as vaultDeleteWallet,
  getWallets,
  exportWallet,
  checkPassword,
  getMinNetworkFee as vaultGetMinNetworkFee,
  type WalletSummaryPayload,
  type WalletPayload
} from '../../vaultApi';
import type { Wallet } from '../types/wallet';
import type { WalletCandidate } from '../services/vanity';

let pendingWallet: (WalletCandidate & { walletName: string }) | null = null;

/**
 * Check the current vault status
 */
export async function getVaultStatus(): Promise<{ exists: boolean; unlocked: boolean }> {
  const result = await vaultStatus();
  if (!result.success || !result.data) {
    throw new Error(result.error || 'Unable to read vault status');
  }
  return result.data;
}

/**
 * Create a new vault with the given password
 */
export async function createVault(password: string): Promise<boolean> {
  const result = await newVault(password);
  if (!result.success) {
    console.error('Failed to create vault:', result.error);
  }
  return result.success;
}

/**
 * Unlock the vault and return wallet summaries
 */
export async function unlockVaultWithPassword(password: string): Promise<{
  success: boolean;
  wallets: Wallet[];
  error?: string;
}> {
  const result = await unlockVault(password);
  if (!result.success || !result.data) {
    return { success: false, wallets: [], error: result.error };
  }

  const wallets = result.data.map(mapVaultWalletToStoreWallet);
  return { success: true, wallets };
}

/**
 * Lock the vault
 */
export async function lockVaultSession(): Promise<boolean> {
  const result = await lockVault();
  return result.success;
}

/**
 * Verify password against the vault
 */
export async function verifyPassword(password: string): Promise<boolean> {
  const result = await checkPassword(password);
  return result.success && result.data === true;
}

/**
 * Generate a new 24-word mnemonic for wallet creation
 */
export async function generateMnemonic(): Promise<{
  success: boolean;
  mnemonic: string[];
  error?: string;
}> {
  const result = await generateKey();
  if (!result.success || !result.data) {
    return { success: false, mnemonic: [], error: result.error };
  }
  return { success: true, mnemonic: result.data };
}

export function setPendingWallet(candidate: WalletCandidate, walletName: string): void {
  clearPendingWallet();
  pendingWallet = { ...candidate, walletName };
}

export function getPendingWallet(): (WalletCandidate & { walletName: string }) | null {
  return pendingWallet;
}

export function clearPendingWallet(): void {
  if (pendingWallet) pendingWallet.key = '';
  pendingWallet = null;
}

/**
 * Import a wallet into the vault
 * @param nickname - Wallet name/nickname
 * @param key - A 24-word mnemonic or zprv extended private key
 */
export async function importWalletToVault(
  nickname: string,
  key: string
): Promise<{ success: boolean; wallet?: Wallet; error?: string }> {
  const result = await importWallet(nickname, key);
  if (!result.success || !result.data) {
    return { success: false, error: result.error };
  }

  const wallet = mapVaultWalletToStoreWallet(result.data);
  return { success: true, wallet };
}

/**
 * Rename a wallet in the vault
 */
export async function renameWalletInVault(
  oldNickname: string,
  newNickname: string
): Promise<{ success: boolean; wallet?: Wallet; error?: string }> {
  const result = await vaultRenameWallet(oldNickname, newNickname);
  if (!result.success || !result.data) {
    return { success: false, error: result.error };
  }

  const wallet = mapVaultWalletToStoreWallet(result.data);
  return { success: true, wallet };
}

/**
 * Delete a wallet from the vault
 */
export async function deleteWalletFromVault(
  nickname: string
): Promise<{ success: boolean; error?: string }> {
  const result = await vaultDeleteWallet(nickname);
  if (!result.success) {
    return { success: false, error: result.error };
  }
  return { success: true };
}

/**
 * Get all wallets from the vault
 */
export async function getWalletsFromVault(): Promise<{
  success: boolean;
  wallets: Wallet[];
  error?: string;
}> {
  const result = await getWallets();
  if (!result.success || !result.data) {
    return { success: false, wallets: [], error: result.error };
  }

  const wallets = result.data.map(mapVaultWalletToStoreWallet);
  return { success: true, wallets };
}

/**
 * Export full wallet details (including private key) from the vault
 */
export async function exportWalletFromVault(
  nickname: string
): Promise<{ success: boolean; wallet?: WalletPayload; error?: string }> {
  const result = await exportWallet(nickname);
  if (!result.success || !result.data) {
    return { success: false, error: result.error };
  }
  return { success: true, wallet: result.data };
}

/**
 * Map a vault WalletSummary to a store Wallet
 */
function mapVaultWalletToStoreWallet(vaultWallet: WalletSummaryPayload): Wallet {
  return {
    id: `vault-${vaultWallet.nickname}`,
    name: vaultWallet.nickname,
    addresses: [vaultWallet.publicKey],
    masterPublicKey: vaultWallet.extendedPublicKey ?? undefined,
    currentAddressIndex: 0,
    balance: 0,
    createdAt: Date.now(),
    lastUsed: Date.now()
  };
}

/**
 * Find a wallet in the store by its vault nickname
 */
export function findWalletByNickname(wallets: Wallet[], nickname: string): Wallet | undefined {
  return wallets.find(w => w.name === nickname || w.id === `vault-${nickname}`);
}

/**
 * Get the vault nickname from a store wallet
 */
export function getVaultNickname(wallet: Wallet): string {
  // If the id starts with 'vault-', extract the nickname
  if (wallet.id.startsWith('vault-')) {
    return wallet.id.substring(6);
  }
  // Otherwise use the name
  return wallet.name;
}

/**
 * Transaction result from signing
 */
export type SignedTransactionResult = {
  success: boolean;
  signedTx?: string;
  feePaid?: number;
  txId?: string;
  inputNotes?: string[];
  error?: string;
};

/** Validate RPC notes without modifying the data committed by their hashes. */
export function transformNoteForWasm(note: Record<string, unknown>): Record<string, unknown> {
  if (Number(note.version) !== 1)
    throw new Error('Spending requires v1 notes. Migrate v0 notes with the Nockchain CLI.');
  if (
    !Number.isSafeInteger(note.assets) ||
    Number(note.assets) < 0 ||
    !Number.isSafeInteger(note.originPage) ||
    Number(note.originPage) < 0
  ) {
    throw new Error('Note contains an invalid amount or block height');
  }
  if (typeof note.firstName !== 'string' || typeof note.lastName !== 'string')
    throw new Error('Note is missing its name');
  if (!note.noteData || typeof note.noteData !== 'object' || Array.isArray(note.noteData))
    throw new Error('Note data is missing');
  return note;
}

/**
 * Create and sign a transaction using the vault
 * Returns the signed transaction blob, fee paid, and transaction ID
 */
export async function createSignedTransaction(
  walletNickname: string,
  notes: Array<Record<string, unknown>>,
  recipients: Array<{ address: string; gift: number; bridgeEvmAddress?: string }>,
  walletAddress: string,
  options: { privateOutputs?: boolean; chainHeight?: number } = {}
): Promise<SignedTransactionResult> {
  // Import dynamically to avoid circular dependency issues
  const { createAndSignTxWithOptions } = await import('../../vaultApi');

  const transformedNotes = notes.map(note => transformNoteForWasm(note));
  for (const recipient of recipients) {
    if (!Number.isSafeInteger(recipient.gift) || recipient.gift <= 0)
      throw new Error('Invalid payment amount');
  }

  const { getRPCClientV1 } = await import('./rpc');
  const chainHeight = await getRPCClientV1().getTipHeight();
  const result = await createAndSignTxWithOptions(
    walletNickname,
    transformedNotes as never[],
    recipients,
    {
      ...options,
      chainHeight
    }
  );

  if (!result.success || !result.data) {
    return { success: false, error: result.error || 'Failed to sign transaction' };
  }

  return {
    success: true,
    signedTx: result.data.base64Tx,
    feePaid: result.data.feePaid,
    txId: result.data.txId,
    inputNotes: result.data.inputNotes
  };
}

/**
 * Get the minimum network fee in nicks from WASM
 */
export async function getMinNetworkFee(): Promise<bigint> {
  const result = await vaultGetMinNetworkFee();
  if (!result.success || result.data === undefined) {
    // Fallback to 1 nick if WASM not ready
    return 1n;
  }
  return result.data;
}
