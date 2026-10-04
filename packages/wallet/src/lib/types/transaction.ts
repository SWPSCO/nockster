import type { BridgeDetails } from '../utils/bridge';
// Transaction-related type definitions

export interface Transaction {
  bridge?: BridgeDetails;
  id: string;
  walletId?: string;
  notes?: string;
  from: string;
  to: string;
  amount: number;
  fee: number;
  timestamp: number;
  status: TransactionStatus;
  blockHeight?: number;
  confirmations?: number;
  /** Transaction type from the API - 'received', 'sent', or 'self' */
  type?: 'received' | 'sent' | 'self';
}

export type TransactionStatus = 'pending' | 'confirmed' | 'failed' | 'cancelled';

export interface TransactionRecipient {
  address: string;
  amount: number;
  label?: string;
}

export interface TransactionRequest {
  recipients: TransactionRecipient[];
  fee: FeeLevel | number;
  message?: string;
}

export type FeeLevel = 'fast' | 'standard' | 'economy' | 'custom';

export interface FeeEstimate {
  fast: number;
  standard: number;
  economy: number;
}
