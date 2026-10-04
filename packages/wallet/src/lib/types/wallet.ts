import type { BridgeDetails } from '../utils/bridge';
// Wallet-related type definitions

// Import correct Note type from RPC client (generated from OpenAPI spec)
import type { NoteV1 as Note } from '../utils/rpc';
import type { TransactionData } from '../utils/rpc';
import type { CachedData } from '../utils/cache';

export interface WalletCachedData {
  balance?: CachedData<number>;
  notes?: CachedData<Note[]>;
  transactions?: CachedData<TransactionData[]>;
}

export interface HardwareWalletInfo {
  manufacturer?: string;
  product?: string;
  transport?: 'hid' | 'serial' | 'tauri';
}

export interface PendingTransaction {
  bridge?: BridgeDetails;
  txId: string;
  recipients: Array<{ address: string; amount: number; bridgeEvmAddress?: string }>;
  totalAmount: number;
  fee: number;
  timestamp: number;
  fromAddress: string;
  signedTx?: string;
  inputNotes?: string[];
  submitAttempts?: number;
  lastSubmittedAt?: number;
  lastSubmitError?: string | null;
  submissionStatus?: string;
  mempoolPresent?: boolean;
  mempoolCheckedAt?: number;
}

export interface Wallet {
  id: string;
  name: string;
  addresses: string[];
  currentAddressIndex: number;
  createdAt: number;
  lastUsed: number;
  balance?: number;
  notes?: Note[];
  transactions?: TransactionData[];
  pendingTransactions?: PendingTransaction[];
  watchOnly?: boolean; // True for watch-only wallets (no private keys)
  hardware?: HardwareWalletInfo; // Hardware wallet metadata
  // Encrypted wallet blob (contains seedPhrase, privateKey, masterPublicKey)
  // This will be encrypted with argon2(password) in the future
  blob?: Uint8Array;
  // Legacy fields - will be removed once blob storage is implemented
  seedPhrase?: string[];
  privateKey?: string;
  masterPublicKey?: string;
  // Cached data with timestamps
  cachedData?: WalletCachedData;
}

export interface WalletState {
  wallets: Wallet[];
  activeWallet: Wallet | null;
  isLocked: boolean;
  isLoading?: boolean;
  error?: string | null;
  isAuthenticated?: boolean;
  password?: string; // Stored password for verification
  lastActivityTime?: number; // Timestamp of last user activity
  isCreatingAdditionalWallet?: boolean; // Flag for wallet creation flow
}

export interface VaultData {
  salt: number[];
  wallets: Wallet[];
  settings: VaultSettings;
}

export interface VaultSettings {
  autoLockTimeout: number;
  currency: string;
}

export interface Balance {
  total: number;
  available: number;
  pending: number;
}

// Re-export Note type for convenience
export type { Note };
