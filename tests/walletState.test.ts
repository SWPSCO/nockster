import assert from 'node:assert/strict';
import { test } from 'node:test';
import { synchronizeWalletState } from '../packages/wallet/src/lib/utils/walletState.ts';
import type { Wallet, WalletState } from '../packages/wallet/src/lib/types/wallet';

test('storage synchronization neither locks an unlocked session nor unlocks a locked session', () => {
  const wallet: Wallet = {
    id: 'hardware-test',
    name: 'Test',
    addresses: ['address'],
    currentAddressIndex: 0,
    createdAt: 1,
    lastUsed: 1,
    hardware: { transport: 'hid' },
    watchOnly: true
  };
  for (const locked of [false, true]) {
    const current: WalletState = {
      wallets: [],
      activeWallet: null,
      isLocked: locked,
      isAuthenticated: !locked,
      isCreatingAdditionalWallet: true,
      lastActivityTime: 20
    };
    const stored: WalletState = {
      wallets: [wallet],
      activeWallet: wallet,
      isLocked: !locked,
      isAuthenticated: locked,
      lastActivityTime: 10
    };
    const updated = synchronizeWalletState(current, stored);
    assert.equal(updated.isLocked, locked);
    assert.equal(updated.isAuthenticated, !locked);
    assert.equal(updated.isCreatingAdditionalWallet, true);
    assert.equal(updated.lastActivityTime, 20);
    assert.deepEqual(updated.wallets, [wallet]);
    assert.equal(updated.activeWallet, wallet);
    assert.equal(
      synchronizeWalletState(updated, { ...stored, lastActivityTime: 30 }).lastActivityTime,
      30
    );
  }
});
