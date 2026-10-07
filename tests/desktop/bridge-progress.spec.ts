import { test, expect } from '@playwright/test';
import { desktopIO, createWallet, source, password } from './native.fixture';

test('bridge history meters advance from inclusion to 400 without claiming delivery', async ({
  page
}) => {
  await desktopIO(page);
  let tip = 200_000;
  await page.route('https://nockblocks.com/rpc/v1', async route => {
    const { method, id } = route.request().postDataJSON();
    if (method !== 'getTip') return route.fallback();
    return route.fulfill({
      json: { jsonrpc: '2.0', id, result: { height: tip, timestamp: Date.now() } }
    });
  });
  await page.goto('/');
  await createWallet(page);
  await page.reload();
  await page.locator('input[type="password"]').fill(password);
  await page.getByRole('button', { name: 'Unlock', exact: true }).click();
  await expect(page.locator('.balance-amount')).toBeVisible();
  await page.evaluate(async source => {
    const { walletStore } = await import(`${source}/lib/stores/wallet.ts`);
    const { createCachedData } = await import(`${source}/lib/utils/cache.ts`);
    let state: any;
    walletStore.subscribe((value: any) => (state = value))();
    const bridge = {
      destination: '0x' + '1'.repeat(40),
      amount: 6553600000,
      protocolFee: 19500000,
      expectedReceived: 6534100000
    };
    const transactions = [
      {
        txId: 'bridge-deposit',
        from: state.activeWallet.addresses[0],
        to: bridge.destination,
        amount: bridge.amount,
        fee: 100,
        timestamp: Date.now(),
        type: 'sent',
        status: 'confirmed',
        blockHeight: 200000,
        bridge
      }
    ];
    walletStore.setWallets(
      state.wallets.map((wallet: any) =>
        wallet.id === state.activeWallet.id
          ? {
              ...wallet,
              transactions,
              cachedData: {
                ...wallet.cachedData,
                transactions: createCachedData(transactions, 300000)
              }
            }
          : wallet
      )
    );
  }, source);
  await expect(page.getByText('0 / 400 blocks', { exact: true })).toBeVisible();
  for (const [height, value] of [
    [200123, 123],
    [200399, 399],
    [200400, 400],
    [200500, 400]
  ]) {
    tip = height;
    await page.evaluate(async source => {
      const { refreshChainTip } = await import(`${source}/lib/stores/chainTip.ts`);
      await refreshChainTip();
    }, source);
    await expect(page.getByRole('progressbar', { name: 'Bridge block wait' })).toHaveAttribute(
      'value',
      String(value)
    );
  }
  await expect(
    page.getByText('Delivery on Base requires bridge processing.', { exact: true })
  ).toBeVisible();
  await page
    .getByRole('navigation', { name: 'Main' })
    .getByRole('button', { name: 'Activity', exact: true })
    .click();
  await expect(page.getByRole('progressbar', { name: 'Bridge block wait' })).toHaveAttribute(
    'value',
    '400'
  );
  await page.screenshot({ path: 'test-results/desktop-bridge-progress.png' });
  tip = 200125;
  await page.evaluate(async source => {
    const { refreshChainTip } = await import(`${source}/lib/stores/chainTip.ts`);
    await refreshChainTip();
  }, source);
  await expect(page.getByText('125 / 400 blocks', { exact: true })).toBeVisible();
  await page.setViewportSize({ width: 800, height: 640 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(800);
});
