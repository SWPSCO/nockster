import type { WalletState } from '../types/wallet';

export function synchronizeWalletState(current: WalletState, stored: WalletState): WalletState {
  return {
    ...current,
    wallets: stored.wallets,
    activeWallet: stored.activeWallet,
    lastActivityTime: Math.max(current.lastActivityTime ?? 0, stored.lastActivityTime ?? 0)
  };
}
