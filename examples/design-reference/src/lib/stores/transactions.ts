import { writable } from 'svelte/store';

export interface Transaction {
  id: string;
  walletId: string;
  type: 'send' | 'receive';
  amount: number;
  address: string;
  date: Date;
  status: 'pending' | 'confirmed' | 'failed';
  confirmations: number;
  txHash: string;
  fee?: number;
  note?: string;
}

export interface TransactionState {
  transactions: Transaction[];
  pendingTransaction: Partial<Transaction> | null;
}

const initialState: TransactionState = {
  transactions: [],
  pendingTransaction: null
};

function createTransactionStore() {
  const { subscribe, set, update } = writable<TransactionState>(initialState);
  
  return {
    subscribe,
    
    addTransaction: (transaction: Omit<Transaction, 'id'>) => {
      update(state => {
        const newTransaction: Transaction = {
          ...transaction,
          id: `tx-${Date.now()}`
        };
        return {
          ...state,
          transactions: [newTransaction, ...state.transactions]
        };
      });
    },
    
    updateTransaction: (id: string, updates: Partial<Transaction>) => {
      update(state => ({
        ...state,
        transactions: state.transactions.map(tx =>
          tx.id === id ? { ...tx, ...updates } : tx
        )
      }));
    },
    
    setPendingTransaction: (transaction: Partial<Transaction> | null) => {
      update(state => ({
        ...state,
        pendingTransaction: transaction
      }));
    },
    
    getTransactionsByWallet: (walletId: string) => {
      let transactions: Transaction[] = [];
      subscribe(state => {
        transactions = state.transactions.filter(tx => tx.walletId === walletId);
      })();
      return transactions;
    },
    
    clearTransactions: () => {
      set(initialState);
    }
  };
}

export const transactionStore = createTransactionStore();