import { writable, derived } from 'svelte/store';

export interface Wallet {
  id: string;
  name: string;
  address: string;
  balance: number;
  seedPhrase: string[];
  network: 'mainnet' | 'testnet';
}

export interface WalletState {
  wallets: Wallet[];
  activeWalletId: string | null;
  isAuthenticated: boolean;
}

const initialState: WalletState = {
  wallets: [],
  activeWalletId: null,
  isAuthenticated: false
};

function createWalletStore() {
  const { subscribe, set, update } = writable<WalletState>(initialState);
  
  return {
    subscribe,
    
    createWallet: (wallet: Omit<Wallet, 'id'>) => {
      update(state => {
        const newWallet: Wallet = {
          ...wallet,
          id: `wallet-${Date.now()}`
        };
        return {
          ...state,
          wallets: [...state.wallets, newWallet],
          activeWalletId: newWallet.id,
          isAuthenticated: true
        };
      });
    },
    
    importWallet: (seedPhrase: string[], name: string = 'Imported Wallet') => {
      update(state => {
        const newWallet: Wallet = {
          id: `wallet-${Date.now()}`,
          name,
          address: `bc1q${Math.random().toString(36).substr(2, 15)}`,
          balance: 0,
          seedPhrase,
          network: 'mainnet'
        };
        return {
          ...state,
          wallets: [...state.wallets, newWallet],
          activeWalletId: newWallet.id,
          isAuthenticated: true
        };
      });
    },
    
    selectWallet: (walletId: string) => {
      update(state => ({
        ...state,
        activeWalletId: walletId
      }));
    },
    
    updateBalance: (walletId: string, balance: number) => {
      update(state => ({
        ...state,
        wallets: state.wallets.map(w => 
          w.id === walletId ? { ...w, balance } : w
        )
      }));
    },
    
    logout: () => {
      set(initialState);
    },
    
    authenticate: () => {
      update(state => ({
        ...state,
        isAuthenticated: true
      }));
    },

    renameWallet: (walletId: string, newName: string) => {
      update(state => ({
        ...state,
        wallets: state.wallets.map(w =>
          w.id === walletId ? { ...w, name: newName } : w
        )
      }));
    },

    deleteWallet: (walletId: string) => {
      update(state => {
        const remainingWallets = state.wallets.filter(w => w.id !== walletId);
        let newActiveId = state.activeWalletId;

        // If we're deleting the active wallet, switch to another
        if (state.activeWalletId === walletId) {
          newActiveId = remainingWallets.length > 0 ? remainingWallets[0].id : null;
        }

        return {
          ...state,
          wallets: remainingWallets,
          activeWalletId: newActiveId
        };
      });
    },

    duplicateWallet: (walletId: string) => {
      update(state => {
        const walletToDuplicate = state.wallets.find(w => w.id === walletId);
        if (!walletToDuplicate) return state;

        const duplicatedWallet: Wallet = {
          ...walletToDuplicate,
          id: `wallet-${Date.now()}`,
          name: `${walletToDuplicate.name} (Copy)`
        };

        return {
          ...state,
          wallets: [...state.wallets, duplicatedWallet]
        };
      });
    }
  };
}

export const walletStore = createWalletStore();

export const activeWallet = derived(
  walletStore,
  $walletStore => $walletStore.wallets.find(w => w.id === $walletStore.activeWalletId)
);