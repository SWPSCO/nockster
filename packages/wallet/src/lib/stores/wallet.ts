import { browserStorage as localStorage } from '../../platform/browserStorage';
import { pendingWithoutConfirmed } from '../utils/pendingStatus';
import { bridgeFromRecipients, verifyBridgeTransaction } from '../utils/bridge';
import { vaultStorage } from '../../platform/vault';
import { synchronizeWalletState } from '../utils/walletState.ts';
import { migrateHardwareWallets } from '../utils/hardwareMigration.ts';
import { writable, derived, get } from 'svelte/store';
import type { Writable, Readable } from 'svelte/store';
import type { HardwareWalletInfo, Wallet, WalletState } from '../types/index';
import { WALLET } from '../constants';
import { getRPCClient, getRPCClientV1 } from '../utils/rpc';
import type { Note, NoteV1 } from '../utils/rpc';
import { collectReservedNoteIds, noteNameToId } from '../utils/noteIds';
/**
 * Legacy: this store previously imported WASM helpers from `../wasm` to export/import
 * wallet blobs directly. That module has been retired in favor of the new vault API.
 *
 * TODO: replace export/import flows with vault-backed equivalents (e.g.,
 * encrypt wallet data into the vault, use `vaultApi` helpers for persistence).
 */
// import { exportWalletToFile, importWalletFromFile } from '../wasm';
import { CACHE_TTL, isCachedDataValid, createCachedData } from '../utils/cache';
import { signHardwareTransaction } from '../utils/hardwareTx';

const initialState: WalletState = {
  wallets: [],
  activeWallet: null,
  isLocked: true,
  isCreatingAdditionalWallet: false,
  lastActivityTime: Date.now()
};

function createWalletStore() {
  const sendingWallets = new Set<string>();
  const { subscribe, set, update }: Writable<WalletState> = writable(initialState);
  let storageListener:
    ((changes: Record<string, chrome.storage.StorageChange>, area: string) => void) | null = null;

  // The legacy Svelte wallet store relied on direct WASM helpers (see ../wasm)
  // to export/import wallet data. The new architecture centralizes vault access
  // through `vaultController` (VMK slot) and `vaultApi`. Any future wallet
  // persistence should happen via those helpers, not direct WASM bindings.

  // Helper function to generate unique wallet name
  function generateWalletName(): string {
    const state = get(walletStore);
    const existingNames = state.wallets.map(w => w.name);

    let name = 'Wallet';
    let counter = 2;

    while (existingNames.includes(name)) {
      name = `Wallet ${counter}`;
      counter++;
    }

    return name;
  }

  // Helper stubs for legacy blob export/import (disabled until vault flow exists).
  const exportToBlob = async () => {
    throw new Error('exportToBlob is disabled until the vault-backed export flow is implemented.');
  };

  const importFromBlob = async () => {
    throw new Error(
      'importFromBlob is disabled until the vault-backed import flow is implemented.'
    );
  };

  return {
    subscribe,

    createWallet: async (
      name: string,
      addresses: string[],
      seedPhrase?: string[],
      privateKey?: string,
      masterPublicKey?: string
    ) => {
      // Check if wallet with same addresses already exists
      const state = get(walletStore);
      const existingWallet = state.wallets.find(
        w => w.addresses && addresses && w.addresses[0] === addresses[0]
      );

      if (existingWallet) {
        console.log('Wallet with same address already exists:', existingWallet.name);
        // Select the existing wallet instead of creating duplicate
        update(state => ({
          ...state,
          activeWallet: existingWallet,
          isLocked: false
        }));
        return;
      }

      const newWallet: Wallet = {
        id: `wallet-${Date.now()}`,
        name: name.substring(0, WALLET.MAX_NAME_LENGTH),
        addresses: addresses.slice(0, WALLET.MAX_ADDRESSES),
        seedPhrase: seedPhrase,
        privateKey: privateKey,
        masterPublicKey: masterPublicKey,
        currentAddressIndex: 0,
        balance: 0,
        createdAt: Date.now(),
        lastUsed: Date.now()
      };

      // Export wallet to blob for secure storage
      // TODO: JAM export currently disabled - need to fix HashMap/array compatibility in WASM
      // try {
      //   const blob = await exportToBlob(newWallet);
      //   newWallet.blob = blob;
      //   console.log('Wallet blob created successfully');
      // } catch (error) {
      //   console.error('Failed to create wallet blob:', error);
      //   // Continue without blob for backward compatibility
      // }
      console.log('JAM blob export skipped - feature in development');

      update(state => ({
        ...state,
        wallets: [...state.wallets, newWallet],
        activeWallet: newWallet,
        isLocked: false
      }));
    },

    importWallet: async (
      seedPhrase: string[],
      addresses: string[],
      name: string = '',
      privateKey?: string,
      masterPublicKey?: string
    ) => {
      // Check if wallet with same addresses already exists
      const state = get(walletStore);
      const existingWallet = state.wallets.find(
        w => w.addresses && addresses && w.addresses[0] === addresses[0]
      );

      if (existingWallet) {
        console.log('Wallet with same address already exists:', existingWallet.name);
        // Select the existing wallet instead of creating duplicate
        update(state => ({
          ...state,
          activeWallet: existingWallet,
          isLocked: false
        }));
        return;
      }

      // Auto-generate name if not provided
      const walletName = name && name.trim() ? name.trim() : generateWalletName();

      const newWallet: Wallet = {
        id: `wallet-${Date.now()}`,
        name: walletName.substring(0, WALLET.MAX_NAME_LENGTH),
        addresses: addresses.slice(0, WALLET.MAX_ADDRESSES),
        seedPhrase: seedPhrase,
        privateKey: privateKey,
        masterPublicKey: masterPublicKey,
        currentAddressIndex: 0,
        balance: 0,
        createdAt: Date.now(),
        lastUsed: Date.now()
      };

      // Export wallet to blob for secure storage
      // TODO: JAM export currently disabled - need to fix HashMap/array compatibility in WASM
      // try {
      //   const blob = await exportToBlob(newWallet);
      //   newWallet.blob = blob;
      //   console.log('Imported wallet blob created successfully');
      // } catch (error) {
      //   console.error('Failed to create imported wallet blob:', error);
      //   // Continue without blob for backward compatibility
      // }
      console.log('JAM blob export skipped - feature in development');

      update(state => ({
        ...state,
        wallets: [...state.wallets, newWallet],
        activeWallet: newWallet,
        isLocked: false
      }));
    },

    importWatchOnlyAddress: async (address: string, name: string = 'Watch-Only Wallet') => {
      // Check if wallet with same address already exists
      const state = get(walletStore);
      const existingWallet = state.wallets.find(w => w.addresses && w.addresses[0] === address);

      if (existingWallet) {
        console.log('Wallet with same address already exists:', existingWallet.name);
        // Select the existing wallet instead of creating duplicate
        update(state => ({
          ...state,
          activeWallet: existingWallet,
          isLocked: false
        }));
        return;
      }

      // Auto-generate name if not provided
      const walletName = name && name.trim() ? name.trim() : generateWalletName();

      const newWallet: Wallet = {
        id: `wallet-${Date.now()}`,
        name: walletName.substring(0, WALLET.MAX_NAME_LENGTH),
        addresses: [address],
        currentAddressIndex: 0,
        balance: 0,
        watchOnly: true,
        createdAt: Date.now(),
        lastUsed: Date.now()
      };

      update(state => ({
        ...state,
        wallets: [...state.wallets, newWallet],
        activeWallet: newWallet,
        isLocked: false
      }));
    },

    upsertHardwareWallet: (
      address: string,
      name: string,
      hardware: HardwareWalletInfo,
      select: boolean = false
    ) => {
      const normalizedAddresses = [address];

      update(state => {
        const walletId = `hardware-${address}`;
        const existingWallet = state.wallets.find(w => !!w.hardware && w.addresses[0] === address);

        if (existingWallet) {
          const updatedWallet: Wallet = {
            ...existingWallet,
            addresses: normalizedAddresses,
            currentAddressIndex: 0,
            watchOnly: true,
            hardware: {
              ...existingWallet.hardware,
              ...hardware
            },
            lastUsed: Date.now()
          };

          return {
            ...state,
            wallets: state.wallets.map(w => (w.id === existingWallet.id ? updatedWallet : w)),
            activeWallet:
              select || state.activeWallet?.id === existingWallet.id
                ? updatedWallet
                : state.activeWallet
          };
        }

        const newWallet: Wallet = {
          id: walletId,
          name: name.substring(0, WALLET.MAX_NAME_LENGTH),
          addresses: normalizedAddresses,
          currentAddressIndex: 0,
          balance: 0,
          watchOnly: true,
          hardware: { ...hardware },
          createdAt: Date.now(),
          lastUsed: Date.now()
        };

        return {
          ...state,
          wallets: [...state.wallets, newWallet],
          activeWallet: select ? newWallet : state.activeWallet
        };
      });

      walletStore.saveToStorage();
    },

    selectWallet: (walletId: string) => {
      update(state => {
        const wallet = state.wallets.find(w => w.id === walletId);
        if (wallet) {
          wallet.lastUsed = Date.now();
        }
        return {
          ...state,
          activeWallet: wallet ?? null
        };
      });
    },

    updateBalance: (walletId: string, balance: number) => {
      update(state => ({
        ...state,
        wallets: state.wallets.map(w => (w.id === walletId ? { ...w, balance } : w))
      }));
    },

    renameWallet: (walletId: string, newName: string) => {
      update(state => {
        const updatedWallets = state.wallets.map(w =>
          w.id === walletId ? { ...w, name: newName } : w
        );

        // Also update activeWallet if it's the one being renamed
        let updatedActiveWallet = state.activeWallet;
        if (state.activeWallet?.id === walletId) {
          updatedActiveWallet = { ...state.activeWallet, name: newName };
        }

        return {
          ...state,
          wallets: updatedWallets,
          activeWallet: updatedActiveWallet
        };
      });

      // Save to storage after renaming
      walletStore.saveToStorage();
    },

    deleteWallet: (walletId: string) => {
      update(state => {
        const remainingWallets = state.wallets.filter(w => w.id !== walletId);
        let newActiveWallet = state.activeWallet;

        // If we're deleting the active wallet, switch to another
        if (state.activeWallet?.id === walletId) {
          newActiveWallet = remainingWallets.length > 0 ? remainingWallets[0] : null;
        }

        return {
          ...state,
          wallets: remainingWallets,
          activeWallet: newActiveWallet
        };
      });

      // Save to storage after deleting
      walletStore.saveToStorage();
    },

    duplicateWallet: (walletId: string) => {
      update(state => {
        const walletToDuplicate = state.wallets.find(w => w.id === walletId);
        if (!walletToDuplicate) return state;

        const duplicatedWallet: Wallet = {
          ...walletToDuplicate,
          id: `wallet-${Date.now()}`,
          name: `${walletToDuplicate.name} (Copy)`,
          balance: walletToDuplicate.balance || 0,
          createdAt: Date.now(),
          lastUsed: Date.now()
        };

        return {
          ...state,
          wallets: [...state.wallets, duplicatedWallet]
        };
      });
    },

    /**
     * @deprecated Password is now handled by the vault. This is kept for backward
     * compatibility but does nothing security-related. Use vault unlock instead.
     */
    setPassword: (_password: string) => {
      // Password is handled by vault - this just marks as authenticated for UI state
      update(state => ({
        ...state,
        isAuthenticated: true
      }));
    },

    /**
     * Unlock the wallet store UI state. Authentication is handled by the vault.
     * This method just updates the UI lock state - call after vault unlock succeeds.
     */
    unlock: (_password?: string): boolean => {
      // Vault handles actual authentication - this just updates UI state
      update(s => ({
        ...s,
        isLocked: false,
        isAuthenticated: true,
        lastActivityTime: Date.now()
      }));
      return true;
    },

    lock: () => {
      update(state => ({
        ...state,
        isLocked: true,
        isAuthenticated: false
      }));
    },

    logout: () => {
      update(state => ({
        ...state,
        isLocked: true,
        isAuthenticated: false,
        activeWallet: null
      }));
    },

    clearAllData: async () => {
      if (['desktop', 'urbit'].includes(import.meta.env.MODE)) {
        const { wipeVault } = await import('../../vaultController');
        await wipeVault();
        await vaultStorage.remove(['walletState']);
      }

      // Clear localStorage
      localStorage.clear();

      // Clear Chrome storage if available
      if (typeof chrome !== 'undefined' && chrome.storage) {
        await new Promise<void>(resolve => {
          chrome.storage.local.clear(() => {
            resolve();
          });
        });
      }

      // Reset store to initial state
      set(initialState);

      // Reload the window to start fresh
      window.location.reload();
    },

    // RPC Methods
    fetchBalance: async (walletId: string, forceRefresh = false) => {
      const rpc = getRPCClient();

      const wallet = get(walletStore).wallets.find(w => w.id === walletId);
      if (!wallet) {
        throw new Error('Wallet not found');
      }

      // Check cache first unless force refresh
      if (
        !forceRefresh &&
        wallet.cachedData?.balance &&
        isCachedDataValid(wallet.cachedData.balance)
      ) {
        console.log('Using cached balance for wallet', walletId);
        return wallet.cachedData.balance.data;
      }

      update(state => ({ ...state, isLoading: true, error: null }));

      try {
        // Get balance for the current address
        const address = wallet.addresses[wallet.currentAddressIndex];
        const balance = await rpc.getBalance(address);

        // Create cached data
        const cachedBalance = createCachedData(balance, CACHE_TTL.BALANCE);

        update(state => ({
          ...state,
          wallets: state.wallets.map(w =>
            w.id === walletId
              ? {
                  ...w,
                  balance,
                  cachedData: {
                    ...w.cachedData,
                    balance: cachedBalance
                  }
                }
              : w
          ),
          activeWallet:
            state.activeWallet?.id === walletId
              ? {
                  ...state.activeWallet,
                  balance,
                  cachedData: {
                    ...state.activeWallet.cachedData,
                    balance: cachedBalance
                  }
                }
              : state.activeWallet,
          isLoading: false
        }));

        // Save to storage
        walletStore.saveToStorage();

        return balance;
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : 'Failed to fetch balance';
        update(state => ({
          ...state,
          error: errorMessage,
          isLoading: false
        }));
        throw error;
      }
    },

    fetchNotes: async (walletId: string, forceRefresh = false) => {
      // Use V1 API for notes - returns proper noteData.lock format
      const rpcV1 = getRPCClientV1();

      const wallet = get(walletStore).wallets.find(w => w.id === walletId);
      if (!wallet) {
        throw new Error('Wallet not found');
      }

      // Check cache first unless force refresh
      if (!forceRefresh && wallet.cachedData?.notes && isCachedDataValid(wallet.cachedData.notes)) {
        console.log('Using cached notes for wallet', walletId);
        return wallet.cachedData.notes.data;
      }

      update(state => ({ ...state, isLoading: true, error: null }));

      try {
        // Get notes for all addresses using V1 API
        const allNotes: NoteV1[] = [];
        for (const address of wallet.addresses) {
          const notes = await rpcV1.getNotesByAddress(address);
          allNotes.push(...notes);
        }

        // Cache the notes with TTL
        const cachedNotes = createCachedData(allNotes, CACHE_TTL.NOTES);

        update(state => ({
          ...state,
          wallets: state.wallets.map(w =>
            w.id === walletId
              ? {
                  ...w,
                  notes: allNotes,
                  cachedData: {
                    ...w.cachedData,
                    notes: cachedNotes
                  }
                }
              : w
          ),
          activeWallet:
            state.activeWallet?.id === walletId
              ? {
                  ...state.activeWallet,
                  notes: allNotes,
                  cachedData: {
                    ...state.activeWallet.cachedData,
                    notes: cachedNotes
                  }
                }
              : state.activeWallet,
          isLoading: false
        }));

        // Persist to storage
        walletStore.saveToStorage();

        return allNotes;
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : 'Failed to fetch notes';
        update(state => ({
          ...state,
          error: errorMessage,
          isLoading: false
        }));
        throw error;
      }
    },

    fetchTransactions: async (walletId: string, forceRefresh = false) => {
      const rpc = getRPCClient();

      const wallet = get(walletStore).wallets.find(w => w.id === walletId);
      if (!wallet) {
        throw new Error('Wallet not found');
      }

      const cachedTransactionsData = wallet.cachedData?.transactions?.data as unknown;
      const cacheLooksMisclassified =
        Array.isArray(cachedTransactionsData) &&
        cachedTransactionsData.some(
          (tx: any) => tx?.type === 'self' && typeof tx?.amount === 'number' && tx.amount > 0
        );

      // Check cache first unless force refresh
      const shouldUseCache =
        !forceRefresh &&
        wallet.cachedData?.transactions &&
        isCachedDataValid(wallet.cachedData.transactions) &&
        !cacheLooksMisclassified;
      if (shouldUseCache) {
        return wallet.cachedData!.transactions!.data;
      }

      update(state => ({ ...state, isLoading: true, error: null }));

      try {
        // Get transactions for the current address
        const address = wallet.addresses[wallet.currentAddressIndex];
        const newTransactions = await rpc.getTransactions(address);

        // Replace transactions with fresh data from API
        // We don't merge because API returns authoritative data with correct type/from/to mapping
        const cachedTransactions = createCachedData(newTransactions, CACHE_TTL.TRANSACTIONS);

        update(state => ({
          ...state,
          wallets: state.wallets.map(w =>
            w.id === walletId
              ? {
                  ...w,
                  transactions: newTransactions,
                  pendingTransactions: pendingWithoutConfirmed(w.pendingTransactions, newTransactions),
                  cachedData: {
                    ...w.cachedData,
                    transactions: cachedTransactions
                  }
                }
              : w
          ),
          activeWallet:
            state.activeWallet?.id === walletId
              ? {
                  ...state.activeWallet,
                  transactions: newTransactions,
                  pendingTransactions: pendingWithoutConfirmed(state.activeWallet.pendingTransactions, newTransactions),
                  cachedData: {
                    ...state.activeWallet.cachedData,
                    transactions: cachedTransactions
                  }
                }
              : state.activeWallet,
          isLoading: false
        }));

        // Persist to storage
        walletStore.saveToStorage();

        return newTransactions;
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

    refreshWallet: async (walletId: string) => {
      try {
        await walletStore.fetchBalance(walletId);
        await walletStore.fetchNotes(walletId);
      } catch (error) {
        console.error('Failed to refresh wallet:', error);
      }
    },

    // Remove confirmed transactions from pending list
    removePendingTransactions: (walletId: string, txIds: string[]) => {
      update(state => ({
        ...state,
        wallets: state.wallets.map(w =>
          w.id === walletId
            ? {
                ...w,
                pendingTransactions: (w.pendingTransactions || []).filter(
                  tx => !txIds.includes(tx.txId)
                )
              }
            : w
        ),
        activeWallet:
          state.activeWallet?.id === walletId
            ? {
                ...state.activeWallet,
                pendingTransactions: (state.activeWallet.pendingTransactions || []).filter(
                  tx => !txIds.includes(tx.txId)
                )
              }
            : state.activeWallet
      }));

      // Persist to storage
      walletStore.saveToStorage();
    },

    // Send transaction via vault API
    // If prebuiltTx is provided, skips building and uses the cached transaction
    sendTransaction: async (
      recipients: Array<{ address: string; amount: number; bridgeEvmAddress?: string }>,
      _feeLevel: string = 'standard',
      prebuiltTx?: { signedTx: string; feePaid: number; txId: string; inputNotes?: string[] },
      options: { privateOutputs?: boolean; expectedNetworkFee?: number } = {}
    ): Promise<{ signedTx: string; feePaid: number; txId: string }> => {
      const state = get(walletStore);
      const wallet = state.activeWallet;
      if (state.isLocked) throw new Error('Wallet is locked');

      if (!wallet) {
        throw new Error('No active wallet selected');
      }

      // Get the wallet's primary address (needed for pending tx tracking)
      const walletAddress = wallet.addresses?.[0];
      if (!walletAddress) {
        throw new Error('Wallet has no address');
      }

      if (wallet.watchOnly && !wallet.hardware) {
        throw new Error('This wallet is watch-only and cannot sign transactions.');
      }

      if (sendingWallets.has(wallet.id))
        throw new Error('A transaction is already being prepared for this wallet');
      for (const recipient of recipients) {
        if (!Number.isSafeInteger(recipient.amount) || recipient.amount <= 0)
          throw new Error('Invalid payment amount');
      }
      if (recipients.length === 0) throw new Error('A recipient is required');
      const bridge = bridgeFromRecipients(recipients);
      if (bridge && options.privateOutputs) throw new Error('Bridge deposits require public outputs');
      sendingWallets.add(wallet.id);
      try {
        const reservedNoteIds = collectReservedNoteIds(wallet.pendingTransactions);
        const notes = wallet.notes || [];
        const availableNotes = reservedNoteIds.size
          ? notes.filter(note => {
              const noteId = noteNameToId(note);
              return !noteId || !reservedNoteIds.has(noteId);
            })
          : notes;

        let result: {
          success: boolean;
          signedTx?: string;
          feePaid?: number;
          txId?: string;
          inputNotes?: string[];
          error?: string;
        };

        if (wallet.hardware) {
          if (options.privateOutputs) {
            throw new Error('Hardware wallets do not support private outputs yet.');
          }
          if (availableNotes.length === 0) {
            throw new Error(
              'No notes available for transaction (all notes are reserved by pending transactions)'
            );
          }
          const hardwareResult = await signHardwareTransaction(
            walletAddress,
            availableNotes as NoteV1[],
            recipients
          );
          result = {
            success: true,
            signedTx: hardwareResult.signedTx,
            feePaid: hardwareResult.feePaid,
            txId: hardwareResult.txId,
            inputNotes: hardwareResult.inputNotes
          };
        } else if (prebuiltTx) {
          const availableIds = new Set(availableNotes.map(noteNameToId).filter(Boolean));
          if (
            !prebuiltTx.inputNotes?.length ||
            prebuiltTx.inputNotes.some(id => !availableIds.has(id))
          ) {
            throw new Error('Transaction inputs changed; refresh the fee preview');
          }

          // Use the pre-built transaction (from fee preview)
          result = {
            success: true,
            signedTx: prebuiltTx.signedTx,
            feePaid: prebuiltTx.feePaid,
            txId: prebuiltTx.txId,
            inputNotes: prebuiltTx.inputNotes
          };
        } else {
          // Build the transaction from scratch
          const { createSignedTransaction, getVaultNickname } =
            await import('../utils/vaultBridge');

          if (availableNotes.length === 0) {
            throw new Error(
              'No notes available for transaction (all notes are reserved by pending transactions)'
            );
          }

          // Map recipients to vault format (amount in nicks as 'gift')
          // Include bridgeEvmAddress for bridge transactions
          const vaultRecipients = recipients.map(r => ({
            address: r.address,
            gift: r.amount, // amount is already in nicks
            ...(r.bridgeEvmAddress ? { bridgeEvmAddress: r.bridgeEvmAddress } : {})
          }));

          // Notes are already in the correct format from the API
          const vaultNotes = availableNotes as unknown as Array<Record<string, unknown>>;

          const nickname = getVaultNickname(wallet);
          result = await createSignedTransaction(
            nickname,
            vaultNotes,
            vaultRecipients,
            walletAddress,
            options
          );
        }

        if (!result.success || !result.signedTx) {
          throw new Error(result.error || 'Failed to sign transaction');
        }

        console.log('Signed transaction:', {
          txId: result.txId,
          feePaid: result.feePaid,
          signedTxLength: result.signedTx.length
        });

        const txId = result.txId;
        if (!txId || !result.inputNotes?.length)
          throw new Error('Signed transaction metadata is incomplete');
        const feePaid = result.feePaid || 0;
        if (options.expectedNetworkFee !== undefined && feePaid !== options.expectedNetworkFee)
          throw new Error('Network fee changed; review the bridge again');
        await verifyBridgeTransaction(result.signedTx, walletAddress, recipients, feePaid);

        // Add pending transaction to wallet for optimistic UI update
        if (txId) {
          const now = Date.now();
          const totalAmount = recipients.reduce((sum, r) => sum + r.amount, 0);
          const pendingTx = {
            bridge,
            txId,
            recipients,
            totalAmount,
            fee: feePaid,
            timestamp: now,
            fromAddress: walletAddress,
            signedTx: result.signedTx,
            inputNotes: result.inputNotes,
            submitAttempts: 0,
            lastSubmitError: null
          };

          update(state => ({
            ...state,
            wallets: state.wallets.map(w =>
              w.id === wallet.id
                ? {
                    ...w,
                    pendingTransactions: [...(w.pendingTransactions || []), pendingTx]
                  }
                : w
            ),
            activeWallet:
              state.activeWallet?.id === wallet.id
                ? {
                    ...state.activeWallet,
                    pendingTransactions: [
                      ...(state.activeWallet.pendingTransactions || []),
                      pendingTx
                    ]
                  }
                : state.activeWallet
          }));

          // Reserve inputs and persist the signed transaction before broadcasting.
          await walletStore.saveToStorage();
          await walletStore.resubmitPendingTransaction(wallet.id, txId);

          // Refresh notes and balance to remove spent notes from local state
          try {
            await walletStore.fetchNotes(wallet.id, true);
            await walletStore.fetchBalance(wallet.id, true);
          } catch (refreshErr) {
            console.warn('Failed to refresh notes after send:', refreshErr);
          }
        }

        return {
          signedTx: result.signedTx,
          feePaid,
          txId
        };
      } finally {
        sendingWallets.delete(wallet.id);
      }
    },

    refreshSubmissionStatus: async (walletId: string) => {
      const wallet = get(walletStore).wallets.find(w => w.id === walletId);
      for (const pending of wallet?.pendingTransactions ?? []) {
        const rpc = getRPCClient();
        const [submission, mempool] = await Promise.allSettled([
          rpc.getTransactionSubmission(pending.txId),
          rpc.isTransactionInMempool(pending.txId)
        ]);
        const checkedAt = Date.now();
        update(state => {
          const apply = (wallet: Wallet) =>
            wallet.id !== walletId
              ? wallet
              : {
                  ...wallet,
                  pendingTransactions: wallet.pendingTransactions?.map(tx =>
                    tx.txId !== pending.txId
                      ? tx
                      : {
                          ...tx,
                          submissionStatus: submission.status === 'fulfilled' ? submission.value.status : tx.submissionStatus,
                          mempoolPresent: mempool.status === 'fulfilled' ? mempool.value : undefined,
                          mempoolCheckedAt: checkedAt,
                          lastSubmitError: submission.status === 'fulfilled' && ['acknowledged', 'accepted'].includes(submission.value.status)
                            ? null
                            : tx.lastSubmitError
                        }
                  )
                };
          return {
            ...state,
            wallets: state.wallets.map(apply),
            activeWallet: state.activeWallet ? apply(state.activeWallet) : null
          };
        });
      }
      await walletStore.saveToStorage();
    },

    resubmitPendingTransaction: async (walletId: string, txId: string) => {
      const state = get(walletStore);
      const wallet = state.wallets.find(w => w.id === walletId);
      if (!wallet) throw new Error('Wallet not found');

      const pendingTx = (wallet.pendingTransactions || []).find(tx => tx.txId === txId);
      if (!pendingTx) throw new Error('Pending transaction not found');
      if (!pendingTx.signedTx) throw new Error('Pending transaction is missing signed payload');

      await verifyBridgeTransaction(pendingTx.signedTx, pendingTx.fromAddress, pendingTx.recipients, pendingTx.fee);
      const rpc = getRPCClient();
      let submitError: string | null = null;
      let submissionStatus = 'unknown';
      try {
        const result = await rpc.submitTransaction(pendingTx.signedTx);
        if (result !== txId) throw new Error('Submission returned a different transaction ID');
        submissionStatus = 'acknowledged';
      } catch (error) {
        submitError = error instanceof Error ? error.message : String(error);
        throw error;
      } finally {
        try {
          const checked = await rpc.getTransactionSubmission(txId);
          if (checked.tracked && !checked.checkFailed) submissionStatus = checked.status;
        } catch {
          // Retain uncertainty when the acceptance query cannot complete.
        }
        const now = Date.now();
        update(state => ({
          ...state,
          wallets: state.wallets.map(w => {
            if (w.id !== walletId) return w;
            return {
              ...w,
              pendingTransactions: (w.pendingTransactions || []).map(tx => {
                if (tx.txId !== txId) return tx;
                return {
                  ...tx,
                  submitAttempts: (tx.submitAttempts || 0) + 1,
                  lastSubmittedAt: now,
                  submissionStatus,
                  lastSubmitError: submitError
                };
              })
            };
          }),
          activeWallet:
            state.activeWallet?.id === walletId
              ? {
                  ...state.activeWallet,
                  pendingTransactions: (state.activeWallet.pendingTransactions || []).map(tx => {
                    if (tx.txId !== txId) return tx;
                    return {
                      ...tx,
                      submitAttempts: (tx.submitAttempts || 0) + 1,
                      lastSubmittedAt: now,
                      submissionStatus,
                      lastSubmitError: submitError
                    };
                  })
                }
              : state.activeWallet
        }));
        await walletStore.saveToStorage();
      }
    },

    deletePendingTransaction: (walletId: string, txId: string) => {
      update(state => ({
        ...state,
        wallets: state.wallets.map(w =>
          w.id === walletId
            ? {
                ...w,
                pendingTransactions: (w.pendingTransactions || []).filter(tx => tx.txId !== txId)
              }
            : w
        ),
        activeWallet:
          state.activeWallet?.id === walletId
            ? {
                ...state.activeWallet,
                pendingTransactions: (state.activeWallet.pendingTransactions || []).filter(
                  tx => tx.txId !== txId
                )
              }
            : state.activeWallet
      }));

      walletStore.saveToStorage();
    },

    // Load wallets from Chrome storage
    loadFromStorage: async (synchronize: boolean = false) => {
      const applyStoredState = (stored: WalletState) => {
        if (!synchronize) {
          set(stored);
          return;
        }
        update(current => synchronizeWalletState(current, stored));
      };
      return new Promise<void>(async (resolve, reject) => {
        const processWalletState = async (walletState: any) => {
          if (!walletState) return walletState;

          // Convert blobArray back to Uint8Array for each wallet
          const processedWallets = await Promise.all(
            (walletState.wallets || []).map(async (w: any) => {
              const wallet = { ...w };

              // Convert blobArray back to Uint8Array
              if (wallet.blobArray && Array.isArray(wallet.blobArray)) {
                wallet.blob = new Uint8Array(wallet.blobArray);
                delete wallet.blobArray;
              }

              return wallet;
            })
          );

          // Process active wallet similarly
          let processedActiveWallet = walletState.activeWallet;
          if (processedActiveWallet && (processedActiveWallet as any).blobArray) {
            processedActiveWallet = { ...processedActiveWallet };
            if (Array.isArray((processedActiveWallet as any).blobArray)) {
              processedActiveWallet.blob = new Uint8Array((processedActiveWallet as any).blobArray);
              delete (processedActiveWallet as any).blobArray;
            }
          }

          const migratedWallets = migrateHardwareWallets(processedWallets);
          return {
            ...walletState,
            wallets: migratedWallets,
            activeWallet: migratedWallets.find(w => w.id === processedActiveWallet?.id) ?? null,
            // NOTE: Password is NOT loaded - vault handles authentication securely
            lastActivityTime: walletState.lastActivityTime || Date.now(), // Load last activity time
            isLocked: true, // Always load as locked; App.svelte will auto-unlock if within timeout
            isAuthenticated: false
          };
        };

        if (['mobile', 'desktop', 'urbit'].includes(import.meta.env.MODE)) {
          try {
            const result = await vaultStorage.get(['walletState']);
            if (result.walletState) applyStoredState(await processWalletState(result.walletState));
            resolve();
          } catch (error) {
            reject(error);
          }
        } else if (typeof chrome !== 'undefined' && chrome.storage) {
          chrome.storage.local.get(['walletState'], async result => {
            if (result.walletState) {
              const processed = await processWalletState(result.walletState);
              applyStoredState(processed);
            }
            resolve();
          });
        } else {
          const stored = localStorage.getItem('walletState');
          if (stored) {
            try {
              const parsed = JSON.parse(stored);
              const processed = await processWalletState(parsed);
              applyStoredState(processed);
            } catch (e) {
              console.error('Failed to parse wallet state:', e);
            }
          }
          resolve();
        }
      });
    },

    // Save wallets to Chrome storage
    saveToStorage: () => {
      let saved: Promise<void> = Promise.resolve();
      update(state => {
        // Convert wallets to storage format with blobs
        const walletsToSave = state.wallets.map(w => {
          const walletCopy = { ...w };

          // If wallet has blob, convert it to array for storage
          if (walletCopy.blob) {
            (walletCopy as any).blobArray = Array.from(walletCopy.blob);
            delete walletCopy.blob; // Remove Uint8Array
          }

          // Signing secrets live in the encrypted vault, including during onboarding.
          delete walletCopy.seedPhrase;
          delete walletCopy.privateKey;

          return walletCopy;
        });

        // Convert active wallet similarly
        let activeWalletToSave = state.activeWallet;
        if (activeWalletToSave) {
          activeWalletToSave = { ...activeWalletToSave };
          if (activeWalletToSave.blob) {
            (activeWalletToSave as any).blobArray = Array.from(activeWalletToSave.blob);
            delete activeWalletToSave.blob;
          }
          delete activeWalletToSave.seedPhrase;
          delete activeWalletToSave.privateKey;
        }

        const stateToSave = {
          wallets: walletsToSave,
          activeWallet: activeWalletToSave,
          // NOTE: Password is NOT stored - vault handles authentication securely
          lastActivityTime: state.lastActivityTime // Save last activity time
        };

        if (['mobile', 'desktop', 'urbit'].includes(import.meta.env.MODE)) {
          saved = vaultStorage.set({ walletState: stateToSave });
        } else if (typeof chrome !== 'undefined' && chrome.storage) {
          saved = chrome.storage.local.set({ walletState: stateToSave });
        } else {
          localStorage.setItem('walletState', JSON.stringify(stateToSave));
        }

        return state;
      });
      return saved;
    },

    startStorageSync: () => {
      if (typeof chrome === 'undefined' || !chrome.storage || !chrome.storage.onChanged) {
        return () => {};
      }

      if (storageListener) {
        return () => {};
      }

      storageListener = (changes, area) => {
        if (area !== 'local') return;
        if (!changes.walletState) return;
        walletStore.loadFromStorage(true).catch(err => {
          console.warn('Failed to sync wallet state from storage:', err);
        });
      };

      chrome.storage.onChanged.addListener(storageListener);

      return () => {
        if (!storageListener) return;
        chrome.storage.onChanged.removeListener(storageListener);
        storageListener = null;
      };
    },

    setCreatingAdditionalWallet: (value: boolean) => {
      update(state => ({
        ...state,
        isCreatingAdditionalWallet: value
      }));
    },

    // Set wallets array directly (used for vault sync)
    setWallets: (wallets: Wallet[]) => {
      update(state => {
        // Try to preserve the active wallet if it still exists
        let activeWallet = state.activeWallet;
        if (activeWallet) {
          const stillExists = wallets.find(w => w.id === activeWallet?.id);
          if (stillExists) {
            activeWallet = stillExists;
          } else {
            activeWallet = wallets.length > 0 ? wallets[0] : null;
          }
        }
        return {
          ...state,
          wallets,
          activeWallet
        };
      });
    },

    updateLastActivity: () => {
      update(state => ({
        ...state,
        lastActivityTime: Date.now()
      }));
    },

    checkAutoLock: (autoLockTimeout: number): boolean => {
      const state = get(walletStore);
      if (autoLockTimeout === 0) return false; // Auto-lock disabled

      const timeSinceLastActivity = Date.now() - (state.lastActivityTime || 0);
      const timeoutMs = autoLockTimeout * 60 * 1000; // Convert minutes to ms

      if (timeSinceLastActivity > timeoutMs && !state.isLocked) {
        console.log('Auto-locking wallet due to inactivity');
        walletStore.lock();
        return true; // Indicates lock was triggered
      }
      return false;
    },

    // Check if within auto-lock timeout
    isWithinAutoLockTimeout: (autoLockTimeout: number): boolean => {
      if (autoLockTimeout === 0) return true; // Auto-lock disabled means always "within timeout"

      const state = get(walletStore);
      const timeSinceLastActivity = Date.now() - (state.lastActivityTime || 0);
      const timeoutMs = autoLockTimeout * 60 * 1000;

      return timeSinceLastActivity < timeoutMs;
    }
  };
}

export const walletStore = createWalletStore();

// Derived store for the active wallet
export const activeWallet: Readable<Wallet | null> = derived(
  walletStore,
  $walletStore => $walletStore.activeWallet
);
