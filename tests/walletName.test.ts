import { test } from 'node:test';
import assert from 'node:assert/strict';
import { nextWalletName } from '../packages/wallet/src/lib/utils/walletName.ts';

test('wallet names start at the saved wallet count plus one, including renamed wallets', () => {
  assert.equal(nextWalletName([]), 'My Wallet');
  assert.equal(nextWalletName(['Savings', 'Spending']), 'My Wallet 3');
  assert.equal(nextWalletName(['My Wallet', 'My Wallet 3']), 'My Wallet 4');
  assert.equal(nextWalletName(['My Wallet 2']), 'My Wallet 3');
});
