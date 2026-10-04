import { test, expect, type Page } from '@playwright/test';
import { mockAccounts } from './rpcAuth.fixture';

const mnemonic =
  'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float';
const address = '4Lu3cSW34WPwvDkTwKh7xB6yMZrvh3bFW7w26UDJVdboxADGkr2bTnL';
const password = 'synthetic-wallet-password';

async function dispatch(page: Page, action: string, fields = {}) {
  return page.evaluate(async request => JSON.parse(await window.nocksterNative.dispatch(request)), {
    action,
    ...fields
  });
}

async function mockWalletData(page: Page) {
  await page.route('https://nockblocks.com/rpc{,/v1}', route => {
    const body = route.request().postDataJSON();
    const authorization = route.request().headers().authorization;
    let result: unknown;
    switch (body.method) {
      case 'getTip':
        return route.fulfill({ json: { price: 0.02, result: { height: 1 } } });
      case 'getNotes':
        result = { nicks: 65536, notes: [] };
        break;
      case 'getTransactionsByAddress':
        result = { transactions: [] };
        break;
      case 'getAddressBook':
        result = {
          entries: [
            {
              id: 'contact',
              address,
              notes: null,
              updatedAt: new Date().toISOString(),
              alias: authorization?.endsWith('_1')
                ? 'First account contact'
                : 'Second account contact'
            }
          ]
        };
        break;
      default:
        throw new Error(`Unexpected RPC method: ${body.method}`);
    }
    expect(authorization).toMatch(/^Bearer ak_live_test_secret_[12]$/);
    return route.fulfill({ json: { jsonrpc: '2.0', id: body.id, result } });
  });
}

test.beforeEach(async ({ page }) => {
  await page.route('https://**', route => route.abort());
  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.nocksterNative));
});

test('import signs in with its own address proof and loads account aliases without browser interaction', async ({
  page,
  context
}) => {
  await context.addCookies([
    {
      name: 'unrelated-browser-session',
      value: 'different-account',
      domain: 'nockblocks.com',
      path: '/',
      secure: true
    }
  ]);
  const calls = await mockAccounts(page);
  await mockWalletData(page);
  const result = await dispatch(page, 'import', { name: 'Main', key: mnemonic, password });
  expect(result.error).toBeUndefined();
  expect(result.state.networkError).toBeNull();
  expect(result.state.wallets[0].address).toBe(address);
  expect(result.state.contacts[0].alias).toBe('First account contact');
  expect(calls).toEqual({ challenges: 1, logins: 1, keys: 1 });
  expect(context.pages()).toHaveLength(1);
  const persisted = await page.evaluate(() => JSON.stringify(localStorage));
  expect(persisted).not.toContain('ak_live_test_secret');
  expect(persisted).not.toContain(mnemonic);
});

test('wallets use separate authenticated address books and reuse only their own encrypted credential', async ({
  page
}) => {
  const calls = await mockAccounts(page, { anyPublicKey: true });
  await mockWalletData(page);
  const first = await dispatch(page, 'import', { name: 'First', key: mnemonic, password });
  const secondKey = (await dispatch(page, 'generate')).mnemonic.join(' ');
  const second = await dispatch(page, 'import', { name: 'Second', key: secondKey });
  expect(second.error).toBeUndefined();
  expect(second.state.networkError).toBeNull();
  expect(second.state.contacts[0].alias).toBe('Second account contact');
  expect(calls.keys).toBe(2);
  const selected = await dispatch(page, 'select', { walletId: first.state.activeId });
  expect(selected.state.contacts).toEqual([]);
  const refreshed = await dispatch(page, 'refresh');
  expect(refreshed.state.networkError).toBeNull();
  expect(refreshed.state.contacts[0].alias).toBe('First account contact');
  expect(calls).toEqual({ challenges: 2, logins: 2, keys: 2 });
});

test('offline import preserves the wallet and signs in automatically when a refresh can connect', async ({
  page
}) => {
  const imported = await dispatch(page, 'import', { name: 'Offline', key: mnemonic, password });
  expect(imported.error).toBeUndefined();
  expect(imported.state.unlocked).toBe(true);
  expect(imported.state.wallets[0].address).toBe(address);
  expect(imported.state.networkError).toBeTruthy();
  const calls = await mockAccounts(page);
  await mockWalletData(page);
  const refreshed = await dispatch(page, 'refresh');
  expect(refreshed.error).toBeUndefined();
  expect(refreshed.state.networkError).toBeNull();
  expect(refreshed.state.contacts[0].alias).toBe('First account contact');
  expect(calls).toEqual({ challenges: 1, logins: 1, keys: 1 });
});
