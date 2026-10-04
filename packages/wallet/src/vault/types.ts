// Shared vault-related types and type guards used across the popup runtime.

export type VaultKdf = {
  kdfType: string;
  salt: string;
  m: number;
  t: number;
  p: number;
};

export type WrappedVmk = {
  nonce: string;
  ct: string;
  tag: string;
};

export type VaultLogEntry = {
  timestamp: number;
  operation: string;
};

export type StoredVault = {
  kdf: VaultKdf;
  wrappedVmk: WrappedVmk;
  log: VaultLogEntry[];
  data: string;
};

export type WalletSummary = {
  nickname: string;
  publicKey: string;
  extendedPublicKey: string;
};

export type WalletDetail = {
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

export type TxRecipient = {
  address: string;
  gift: number;
  /** Optional EVM address for bridge transactions (hex string with or without 0x prefix) */
  bridgeEvmAddress?: string;
};

export type TransactionResult = {
  base64Tx: string;
  feePaid: number;
  txId: string;
  inputNotes?: string[];
};

export type JamFormat = 'walletTransaction' | 'rawTransactionV1' | 'rawTransactionV0';

export type JamLockPkh = {
  m: number;
  pubkeyHashes: string[];
};

export type JamLockSummary = {
  pkh?: JamLockPkh | null;
};

export type JamOutputSummary = {
  lockRoot: string;
  gift: number;
  lock?: JamLockSummary | null;
};

export type JamSpendSummary = {
  name: string;
  lock?: JamLockSummary | null;
  signedBy: string[];
  isFullySigned: boolean;
};

export type JamInspectResult = {
  format: JamFormat;
  txId: string;
  outputs: JamOutputSummary[];
  feePaid: number | null;
  spends: JamSpendSummary[];
};

export type JamSignResult = {
  format: JamFormat;
  txId: string;
  base64Tx: string;
  spendsSigned: number;
};

export type GeneratedKeyResponse = string[];

export type VaultBuildResponse = {
  vault: StoredVault;
  vaultKey: number;
};

export type UnlockVaultResponse = {
  vault: StoredVault;
  vaultKey: number;
  wallets: WalletSummary[];
};

export type ImportWalletResponse = {
  vault: StoredVault;
  vaultKey: number;
  wallet: WalletSummary;
};

export type RenameWalletResponse = {
  vault: StoredVault;
  vaultKey: number;
  wallet: WalletSummary;
};

export type DeleteWalletResponse = {
  vault: StoredVault;
  vaultKey: number;
  deleted: boolean;
};

export type GetPubkeyResponse = {
  vault: StoredVault;
  vaultKey: number;
  wallet: WalletSummary;
};

export type GetWalletsResponse = {
  vault: StoredVault;
  vaultKey: number;
  wallets: WalletSummary[];
};

export type ExportWalletResponse = {
  vault: StoredVault;
  vaultKey: number;
  wallet: WalletDetail;
};

export type CheckPasswordResponse = {
  vault: StoredVault;
  matches: boolean;
};

const isFiniteNumber = (value: unknown): value is number =>
  typeof value === 'number' && Number.isFinite(value);

function isVaultKdf(value: unknown): value is VaultKdf {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return (
    typeof record.kdfType === 'string' &&
    typeof record.salt === 'string' &&
    isFiniteNumber(record.m) &&
    isFiniteNumber(record.t) &&
    isFiniteNumber(record.p)
  );
}

function isWrappedVmk(value: unknown): value is WrappedVmk {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return (
    typeof record.nonce === 'string' &&
    typeof record.ct === 'string' &&
    typeof record.tag === 'string'
  );
}

function isVaultLogEntry(value: unknown): value is VaultLogEntry {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return isFiniteNumber(record.timestamp) && typeof record.operation === 'string';
}

function isWalletSummary(value: unknown): value is WalletSummary {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return (
    typeof record.nickname === 'string' &&
    typeof record.publicKey === 'string' &&
    typeof record.extendedPublicKey === 'string'
  );
}

function isWalletDetail(value: unknown): value is WalletDetail {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  const isByteArray = (candidate: unknown, length: number) => {
    if (candidate instanceof Uint8Array) {
      return candidate.length === length;
    }
    return (
      Array.isArray(candidate) &&
      candidate.length === length &&
      candidate.every(entry => typeof entry === 'number')
    );
  };
  const seedphrase = record.seedphrase;
  const isSeedphraseValid =
    seedphrase == null ||
    (Array.isArray(seedphrase) &&
      seedphrase.length === 24 &&
      seedphrase.every(entry => typeof entry === 'string'));

  return (
    typeof record.publicKey === 'string' &&
    typeof record.extendedPublicKey === 'string' &&
    typeof record.privateKey === 'string' &&
    typeof record.extendedPrivateKey === 'string' &&
    isByteArray(record.chainCode, 32) &&
    typeof record.depth === 'number' &&
    typeof record.index === 'number' &&
    typeof record.version === 'number' &&
    isByteArray(record.parentFingerprint, 4) &&
    isSeedphraseValid
  );
}

export function isGeneratedKeyResponse(value: unknown): value is GeneratedKeyResponse {
  return (
    Array.isArray(value) && value.length === 24 && value.every(entry => typeof entry === 'string')
  );
}

export function isStoredVault(value: unknown): value is StoredVault {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return (
    isVaultKdf(record.kdf) &&
    isWrappedVmk(record.wrappedVmk) &&
    Array.isArray(record.log) &&
    record.log.every(isVaultLogEntry) &&
    typeof record.data === 'string'
  );
}

export function isVaultBuildResponse(value: unknown): value is VaultBuildResponse {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return isStoredVault(record.vault) && isFiniteNumber(record.vaultKey);
}

export function isUnlockVaultResponse(value: unknown): value is UnlockVaultResponse {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return (
    isStoredVault(record.vault) &&
    isFiniteNumber(record.vaultKey) &&
    Array.isArray(record.wallets) &&
    record.wallets.every(isWalletSummary)
  );
}

export function isImportWalletResponse(value: unknown): value is ImportWalletResponse {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return (
    isStoredVault(record.vault) && isFiniteNumber(record.vaultKey) && isWalletSummary(record.wallet)
  );
}

export function isRenameWalletResponse(value: unknown): value is RenameWalletResponse {
  return isImportWalletResponse(value);
}

export function isDeleteWalletResponse(value: unknown): value is DeleteWalletResponse {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return (
    isStoredVault(record.vault) &&
    isFiniteNumber(record.vaultKey) &&
    typeof record.deleted === 'boolean'
  );
}

export function isGetWalletsResponse(value: unknown): value is GetWalletsResponse {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return (
    isStoredVault(record.vault) &&
    isFiniteNumber(record.vaultKey) &&
    Array.isArray(record.wallets) &&
    record.wallets.every(isWalletSummary)
  );
}

export function isGetPubkeyResponse(value: unknown): value is GetPubkeyResponse {
  return isImportWalletResponse(value);
}

export function isExportWalletResponse(value: unknown): value is ExportWalletResponse {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return (
    isStoredVault(record.vault) && isFiniteNumber(record.vaultKey) && isWalletDetail(record.wallet)
  );
}
export function isCheckPasswordResponse(value: unknown): value is CheckPasswordResponse {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return isStoredVault(record.vault) && typeof record.matches === 'boolean';
}

export function isTxRecipient(value: unknown): value is TxRecipient {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return (
    typeof record.address === 'string' &&
    record.address.length > 0 &&
    Number.isSafeInteger(record.gift) &&
    Number(record.gift) > 0
  );
}

export function isTxRecipientArray(value: unknown): value is TxRecipient[] {
  return Array.isArray(value) && value.every(isTxRecipient);
}

export function isTransactionResult(value: unknown): value is TransactionResult {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  const inputNotes = record.inputNotes;
  return (
    typeof record.base64Tx === 'string' &&
    typeof record.feePaid === 'number' &&
    typeof record.txId === 'string' &&
    (inputNotes === undefined ||
      (Array.isArray(inputNotes) && inputNotes.every(entry => typeof entry === 'string')))
  );
}

function isJamFormat(value: unknown): value is JamFormat {
  return (
    value === 'walletTransaction' || value === 'rawTransactionV1' || value === 'rawTransactionV0'
  );
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every(entry => typeof entry === 'string');
}

function isJamLockPkh(value: unknown): value is JamLockPkh {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return isFiniteNumber(record.m) && isStringArray(record.pubkeyHashes);
}

function isJamLockSummary(value: unknown): value is JamLockSummary {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  if (!('pkh' in record)) return true;
  return record.pkh == null || isJamLockPkh(record.pkh);
}

function isJamOutputSummary(value: unknown): value is JamOutputSummary {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  const lock = record.lock;
  return (
    typeof record.lockRoot === 'string' &&
    record.lockRoot.length > 0 &&
    isFiniteNumber(record.gift) &&
    (lock == null || isJamLockSummary(lock))
  );
}

function isJamSpendSummary(value: unknown): value is JamSpendSummary {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  const lock = record.lock;
  return (
    typeof record.name === 'string' &&
    record.name.length > 0 &&
    isStringArray(record.signedBy) &&
    typeof record.isFullySigned === 'boolean' &&
    (lock == null || isJamLockSummary(lock))
  );
}

export function isJamInspectResult(value: unknown): value is JamInspectResult {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return (
    isJamFormat(record.format) &&
    typeof record.txId === 'string' &&
    Array.isArray(record.outputs) &&
    record.outputs.every(isJamOutputSummary) &&
    Array.isArray(record.spends) &&
    record.spends.every(isJamSpendSummary)
  );
}

export function isJamSignResult(value: unknown): value is JamSignResult {
  if (!value || typeof value !== 'object' || value === null) return false;
  const record = value as Record<string, unknown>;
  return (
    isJamFormat(record.format) &&
    typeof record.txId === 'string' &&
    typeof record.base64Tx === 'string' &&
    isFiniteNumber(record.spendsSigned)
  );
}
