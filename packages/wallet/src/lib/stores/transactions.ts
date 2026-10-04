import { writable, derived, get } from 'svelte/store';
import type { Writable, Readable } from 'svelte/store';
import type { Transaction, TransactionStatus } from '../types/index';
import { getRPCClient } from '../utils/rpc';
import type { TransactionData } from '../utils/rpc';

export type TransactionType = 'send' | 'receive' | 'consolidation';

export interface TransactionState {
  transactions: Transaction[];
  isLoading: boolean;
  error: string | null;
}

// Helper function to determine transaction type based on address
function determineTransactionType(tx: TransactionData, address: string): TransactionType {
  if (tx.to === address && tx.from === address) {
    return 'consolidation';
  } else if (tx.to === address) {
    return 'receive';
  } else {
    return 'send';
  }
}

const initialState: TransactionState = {
  transactions: [],
  isLoading: false,
  error: null
};

function createTransactionStore() {
  const { subscribe, set, update }: Writable<TransactionState> = writable(initialState);

  return {
    subscribe,

    addTransaction: (transaction: Omit<Transaction, 'id'>) => {
      update(state => {
        const newTransaction: Transaction = {
          ...transaction,
          id: `tx-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`
        };

        const updatedState = {
          ...state,
          transactions: [newTransaction, ...state.transactions]
        };

        // Save to storage
        saveTransactionsToStorage(updatedState.transactions);

        return updatedState;
      });
    },

    updateTransactionStatus: (txId: string, status: TransactionStatus, confirmations?: number) => {
      update(state => {
        const updatedState = {
          ...state,
          transactions: state.transactions.map(tx =>
            tx.id === txId
              ? { ...tx, status, confirmations: confirmations ?? tx.confirmations }
              : tx
          )
        };

        saveTransactionsToStorage(updatedState.transactions);
        return updatedState;
      });
    },

    getTransactionsByWallet: (walletId: string): Transaction[] => {
      const state = get({ subscribe });
      return state.transactions.filter(tx => tx.walletId === walletId);
    },

    getTransaction: (txId: string): Transaction | undefined => {
      const state = get({ subscribe });
      return state.transactions.find(tx => tx.id === txId);
    },

    getPendingTransactions: (): Transaction[] => {
      const state = get({ subscribe });
      return state.transactions.filter(tx => tx.status === 'pending');
    },

    // RPC Methods
    fetchTransactions: async (address: string, limit = 50) => {
      const rpc = getRPCClient();
      update(state => ({ ...state, isLoading: true, error: null }));

      try {
        const txData = await rpc.getTransactions(address, { limit });

        // Convert RPC transaction data to our Transaction type
        const transactions: Transaction[] = txData.map(tx => ({
          id: tx.txId,
          walletId: '', // Will be set by the component
          type: tx.type ?? (tx.from === address ? 'sent' : 'received'),
          from: tx.from || '',
          to: tx.to || '',
          amount: tx.amount,
          fee: tx.fee,
          timestamp: tx.timestamp,
          status: tx.status as TransactionStatus,
          confirmations: tx.confirmations || 0,
          notes: tx.notes,
          blockHeight: tx.blockHeight
        }));

        update(state => ({
          ...state,
          transactions,
          isLoading: false
        }));

        // Save to storage
        saveTransactionsToStorage(transactions);

        return transactions;
      } catch (error) {
        const errorMessage =
          error instanceof Error ? error.message : 'Failed to fetch transactions';
        update(state => ({
          ...state,
          error: errorMessage,
          isLoading: false
        }));
        throw error;
      }
    },

    submitTransaction: async (rawTransaction: string) => {
      const rpc = getRPCClient();
      update(state => ({ ...state, isLoading: true, error: null }));

      try {
        const txId = await rpc.submitTransaction(rawTransaction);

        update(state => ({ ...state, isLoading: false }));

        return txId;
      } catch (error) {
        const errorMessage =
          error instanceof Error ? error.message : 'Failed to submit transaction';
        update(state => ({
          ...state,
          error: errorMessage,
          isLoading: false
        }));
        throw error;
      }
    },

    fetchTransactionDetails: async (txId: string) => {
      const rpc = getRPCClient();

      try {
        const txData = await rpc.getTransaction(txId);

        const transaction: Transaction = {
          id: txData.txId,
          walletId: '',
          type: 'sent',
          from: txData.from || '',
          to: txData.to || '',
          amount: txData.amount,
          fee: txData.fee,
          timestamp: txData.timestamp,
          status: txData.status as TransactionStatus,
          confirmations: txData.confirmations || 0,
          notes: txData.notes,
          blockHeight: txData.blockHeight
        };

        // Update transaction in store
        update(state => ({
          ...state,
          transactions: state.transactions.map(tx => (tx.id === txId ? transaction : tx))
        }));

        return transaction;
      } catch (error) {
        console.error('Failed to fetch transaction details:', error);
        throw error;
      }
    },

    clearTransactions: () => {
      set(initialState);
      if (typeof chrome !== 'undefined' && chrome.storage) {
        chrome.storage.local.remove('transactions');
      } else {
        localStorage.removeItem('transactions');
      }
    },

    loadFromStorage: async (): Promise<void> => {
      return new Promise(resolve => {
        update(state => ({ ...state, isLoading: true }));

        if (typeof chrome !== 'undefined' && chrome.storage) {
          chrome.storage.local.get(['transactions'], result => {
            if (result.transactions) {
              update(state => ({
                ...state,
                transactions: result.transactions as Transaction[],
                isLoading: false
              }));
            } else {
              update(state => ({ ...state, isLoading: false }));
            }
            resolve();
          });
        } else {
          const stored = localStorage.getItem('transactions');
          if (stored) {
            try {
              const transactions = JSON.parse(stored);
              update(state => ({
                ...state,
                transactions,
                isLoading: false
              }));
            } catch (e) {
              console.error('Failed to parse transactions:', e);
              update(state => ({
                ...state,
                error: 'Failed to load transactions',
                isLoading: false
              }));
            }
          } else {
            update(state => ({ ...state, isLoading: false }));
          }
          resolve();
        }
      });
    }
  };
}

function saveTransactionsToStorage(transactions: Transaction[]) {
  if (typeof chrome !== 'undefined' && chrome.storage) {
    chrome.storage.local.set({ transactions });
  } else {
    localStorage.setItem('transactions', JSON.stringify(transactions));
  }
}

export const transactionStore = createTransactionStore();

// Derived stores for filtered transactions
export const pendingTransactions: Readable<Transaction[]> = derived(transactionStore, $store =>
  $store.transactions.filter(tx => tx.status === 'pending')
);

export const recentTransactions: Readable<Transaction[]> = derived(transactionStore, $store =>
  $store.transactions
    .slice()
    .sort((a, b) => b.timestamp - a.timestamp)
    .slice(0, 10)
);
