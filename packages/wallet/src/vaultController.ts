// Public vault API for extension, mobile, and desktop clients.
import type { NoteV1 } from './lib/utils/rpc';
import type {
  TxRecipient,
  TransactionResult,
  JamInspectResult,
  JamSignResult
} from './vault/types';

// Helper to send messages to the service worker
async function sendVaultMessage<T = unknown>(
  message: { type: string } & Record<string, unknown>
): Promise<T> {
  if (['mobile', 'desktop'].includes(import.meta.env.MODE)) {
    const { handleVaultMessage } = await import('./vault/engine');
    const response = await handleVaultMessage(message as import('./vault/engine').VaultMessage);
    if (!response.success) throw new Error(response.error || 'Vault operation failed');
    return response.data as T;
  }
  return new Promise((resolve, reject) => {
    chrome.runtime.sendMessage(message, response => {
      if (chrome.runtime.lastError) {
        reject(new Error(chrome.runtime.lastError.message));
        return;
      }
      if (!response) {
        reject(new Error('No response from service worker'));
        return;
      }
      if (!response.success) {
        reject(new Error(response.error || 'Unknown error'));
        return;
      }
      resolve(response.data as T);
    });
  });
}

/**
 * Returns high-level vault status information for UI callers.
 */
export async function vaultStatus() {
  return sendVaultMessage<{ exists: boolean; unlocked: boolean }>({
    type: 'vault:status'
  });
}

/**
 * Creates and persists a brand new vault blob using the supplied password.
 */
export async function newVault(localPassword: string) {
  if (typeof localPassword !== 'string' || !localPassword) {
    throw new Error('localPassword missing');
  }
  await sendVaultMessage({ type: 'vault:newVault', password: localPassword });
  return { success: true };
}

export async function unlockVault(localPassword: string) {
  if (typeof localPassword !== 'string' || !localPassword) {
    throw new Error('localPassword missing');
  }
  const result = await sendVaultMessage<{
    wallets: Array<{ nickname: string; publicKey: string; extendedPublicKey: string }>;
  }>({ type: 'vault:unlock', password: localPassword });
  return result.wallets;
}

export async function lockVault() {
  await sendVaultMessage({ type: 'vault:lock' });
  return { success: true };
}

/**
 * Deletes any stored vault blob.
 */
export async function wipeVault() {
  await sendVaultMessage({ type: 'vault:wipe' });
  return { success: true };
}

/**
 * Verifies that the provided password matches the unlocked vault key.
 */
export async function checkPassword(localPassword: string) {
  if (typeof localPassword !== 'string' || !localPassword) {
    throw new Error('localPassword missing');
  }
  const result = await sendVaultMessage<{ matches: boolean }>({
    type: 'vault:checkPassword',
    password: localPassword
  });
  return result.matches;
}

export async function showLogs() {
  const result = await sendVaultMessage<{ logs: Array<{ timestamp: number; operation: string }> }>({
    type: 'vault:showLogs'
  });
  return result.logs;
}

export async function generateKey(): Promise<string[]> {
  const result = await sendVaultMessage<{ mnemonic: string[] }>({
    type: 'vault:generateKey'
  });
  return result.mnemonic;
}

export async function importWallet(nickname: string, key: string) {
  if (typeof nickname !== 'string' || !nickname) {
    throw new Error('nickname missing');
  }
  if (typeof key !== 'string' || !key) {
    throw new Error('key missing');
  }
  const result = await sendVaultMessage<{
    wallet: { nickname: string; publicKey: string; extendedPublicKey: string };
  }>({ type: 'vault:importWallet', nickname, key });
  return result.wallet;
}

export async function renameWallet(oldNickname: string, newNickname: string) {
  if (typeof oldNickname !== 'string' || !oldNickname) {
    throw new Error('oldNickname missing');
  }
  if (typeof newNickname !== 'string' || !newNickname) {
    throw new Error('newNickname missing');
  }
  const result = await sendVaultMessage<{
    wallet: { nickname: string; publicKey: string; extendedPublicKey: string };
  }>({ type: 'vault:renameWallet', oldNickname, newNickname });
  return result.wallet;
}

export async function deleteWallet(nickname: string) {
  if (typeof nickname !== 'string' || !nickname) {
    throw new Error('nickname missing');
  }
  const result = await sendVaultMessage<{ deleted: boolean }>({
    type: 'vault:deleteWallet',
    nickname
  });
  return result.deleted;
}

export async function getPubkey(nickname: string) {
  if (typeof nickname !== 'string' || !nickname) {
    throw new Error('nickname missing');
  }
  const result = await sendVaultMessage<{
    wallet: { nickname: string; publicKey: string; extendedPublicKey: string };
  }>({ type: 'vault:getPubkey', nickname });
  return result.wallet;
}

export async function getWallets() {
  const result = await sendVaultMessage<{
    wallets: Array<{ nickname: string; publicKey: string; extendedPublicKey: string }>;
  }>({ type: 'vault:getWallets' });
  return result.wallets;
}

export async function createAndSignTx(
  nickname: string,
  notes: NoteV1[],
  recipients: TxRecipient[]
): Promise<TransactionResult> {
  if (typeof nickname !== 'string' || !nickname) {
    throw new Error('nickname missing');
  }
  if (!Array.isArray(notes)) {
    throw new Error('notes must be an array');
  }
  if (!Array.isArray(recipients) || recipients.length === 0) {
    throw new Error('recipients must include at least one destination');
  }
  return sendVaultMessage<TransactionResult>({
    type: 'vault:createAndSignTx',
    nickname,
    notes,
    recipients
  });
}

export async function createAndSignTxWithOptions(
  nickname: string,
  notes: NoteV1[],
  recipients: TxRecipient[],
  options: { privateOutputs?: boolean; chainHeight?: number } = {}
): Promise<TransactionResult> {
  if (typeof nickname !== 'string' || !nickname) {
    throw new Error('nickname missing');
  }
  if (!Array.isArray(notes)) {
    throw new Error('notes must be an array');
  }
  if (!Array.isArray(recipients) || recipients.length === 0) {
    throw new Error('recipients must include at least one destination');
  }
  return sendVaultMessage<TransactionResult>({
    type: 'vault:createAndSignTxWithOptions',
    nickname,
    notes,
    recipients,
    privateOutputs: Boolean(options.privateOutputs),
    chainHeight: options.chainHeight
  });
}

export async function inspectTxJam(base64Jam: string): Promise<JamInspectResult> {
  if (typeof base64Jam !== 'string' || !base64Jam.trim()) {
    throw new Error('base64Jam missing');
  }
  return sendVaultMessage<JamInspectResult>({
    type: 'vault:inspectTxJam',
    base64Jam
  });
}

export async function signTxJam(nickname: string, base64Jam: string): Promise<JamSignResult> {
  if (typeof nickname !== 'string' || !nickname) {
    throw new Error('nickname missing');
  }
  if (typeof base64Jam !== 'string' || !base64Jam.trim()) {
    throw new Error('base64Jam missing');
  }
  return sendVaultMessage<JamSignResult>({
    type: 'vault:signTxJam',
    nickname,
    base64Jam
  });
}

export async function signTxJamSelected(
  nickname: string,
  base64Jam: string,
  selectedSpends: string[]
): Promise<JamSignResult> {
  if (typeof nickname !== 'string' || !nickname) {
    throw new Error('nickname missing');
  }
  if (typeof base64Jam !== 'string' || !base64Jam.trim()) {
    throw new Error('base64Jam missing');
  }
  if (!Array.isArray(selectedSpends)) {
    throw new Error('selectedSpends must be an array');
  }
  return sendVaultMessage<JamSignResult>({
    type: 'vault:signTxJamSelected',
    nickname,
    base64Jam,
    selectedSpends
  });
}

export async function toRawTxJam(base64Jam: string): Promise<string> {
  if (typeof base64Jam !== 'string' || !base64Jam.trim()) {
    throw new Error('base64Jam missing');
  }
  return sendVaultMessage<string>({
    type: 'vault:toRawTxJam',
    base64Jam
  });
}

export async function toWalletTxJam(base64Jam: string): Promise<string> {
  if (typeof base64Jam !== 'string' || !base64Jam.trim()) {
    throw new Error('base64Jam missing');
  }
  return sendVaultMessage<string>({
    type: 'vault:toWalletTxJam',
    base64Jam
  });
}

export async function exportWallet(nickname: string) {
  if (typeof nickname !== 'string' || !nickname) {
    throw new Error('nickname missing');
  }
  const result = await sendVaultMessage<{
    wallet: {
      publicKey: string;
      extendedPublicKey: string;
      privateKey: string;
      extendedPrivateKey: string;
      chainCode: number[];
      depth: number;
      index: number;
      parentFingerprint: number[];
      version: number;
      seedphrase: string[] | null;
    };
  }>({ type: 'vault:exportWallet', nickname });

  return {
    publicKey: result.wallet.publicKey,
    extendedPublicKey: result.wallet.extendedPublicKey,
    privateKey: result.wallet.privateKey,
    extendedPrivateKey: result.wallet.extendedPrivateKey,
    chainCode: [...result.wallet.chainCode],
    depth: result.wallet.depth,
    index: result.wallet.index,
    parentFingerprint: [...result.wallet.parentFingerprint],
    version: result.wallet.version,
    seedphrase: result.wallet.seedphrase ? [...result.wallet.seedphrase] : null
  };
}

/**
 * Get the minimum network fee in nicks
 */
export async function getMinNetworkFee(): Promise<bigint> {
  const result = await sendVaultMessage<{ fee: string }>({
    type: 'vault:getMinNetworkFee'
  });
  return BigInt(result.fee);
}

/**
 * Get the base fee per word in nicks
 */
export async function getBaseFeePerWord(): Promise<bigint> {
  const result = await sendVaultMessage<{ fee: string }>({
    type: 'vault:getBaseFeePerWord'
  });
  return BigInt(result.fee);
}

/** Export the vault master key for hardware wrapping after password verification. */
export async function exportDeviceUnlockKey(password: string): Promise<string> {
  const response = await sendVaultMessage<{ key: string }>({
    type: 'vault:exportUnlockKey',
    password
  });
  return response.key;
}

export async function unlockWithDeviceKey(key: string): Promise<void> {
  await sendVaultMessage({ type: 'vault:unlockWithDeviceKey', key });
}
