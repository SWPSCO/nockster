import { test, expect } from '@playwright/test';
import { fileURLToPath } from 'node:url';
import { demoWallet, mockDemoWallet } from './demo-wallet.fixture';
import { mockAccounts } from './rpcAuth.fixture';

test('recorded public wallet responses load balance and activity entirely offline', async ({ page }) => {
  const rejected = await mockDemoWallet(page);
  await mockAccounts(page, { anyPublicKey: true });
  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.nocksterNative));
  // Authenticate a disposable random wallet; querying public chain data does not
  // require importing the demo wallet's recovery phrase into the test.
  await page.evaluate(async () => {
    const generated = JSON.parse(await window.nocksterNative.dispatch({ action: 'generate' }));
    const imported = JSON.parse(await window.nocksterNative.dispatch({
      action: 'import', name: 'Fixture reader', password: 'test-only-fixture-reader',
      key: generated.mnemonic.join(' ')
    }));
    if (imported.error) throw new Error(imported.error);
  });
  const modulePath = '/@fs' + fileURLToPath(new URL('../../packages/wallet/src/lib/utils/rpc.ts', import.meta.url));
  const result = await page.evaluate(async ({ modulePath, address }) => {
    const { NockchainRPC } = await import(modulePath);
    const rpc = new NockchainRPC({ retries: 1 });
    return {
      balance: await rpc.getBalance(address),
      history: await rpc.getTransactions(address, { limit: 20 }),
      secondPage: await rpc.getTransactions(address, { offset: 20, limit: 20 })
    };
  }, { modulePath, address: demoWallet.address });
  const notes = demoWallet.records.find((record: any) => record.method === 'getNotes').response.result;
  const history = demoWallet.records.find((record: any) => record.method === 'getTransactionsByAddress').response.result;
  expect(result.balance).toBe(notes.nicks);
  expect(result.history).toHaveLength(history.transactions.length);
  expect(result.history.length).toBeGreaterThan(0);
  for (const tx of result.history) {
    expect(tx.status).toBe('confirmed');
    expect(Number.isFinite(tx.amount)).toBe(true);
    expect(tx.timestamp).toBeGreaterThan(1_000_000_000_000);
  }
  expect(result.secondPage).toEqual([]);
  expect(rejected).toEqual([]);
});

test('demo replay rejects submission and unrecorded wallet addresses', async ({ page }) => {
  const rejected = await mockDemoWallet(page);
  await page.goto('/');
  const replies = await page.evaluate(async () => {
    const replies = [];
    for (const request of [
      { method: 'submitTransaction', params: [{ rawTransaction: 'not-a-transaction' }] },
      { method: 'getNotes', params: [{ address: 'not-the-demo-address' }] }
    ]) {
      const response = await fetch('https://nockblocks.com/rpc', {
        method: 'POST', body: JSON.stringify({ jsonrpc: '2.0', id: 1, ...request })
      });
      replies.push(await response.json());
    }
    return replies;
  });
  expect(replies.every(reply => reply.error?.code === -32601)).toBe(true);
  expect(rejected).toEqual(['v0/submitTransaction', 'v0/getNotes']);
});
