import { test, expect } from '@playwright/test';
import { desktopIO, createWallet, source } from './native.fixture';
import { mockAccounts } from '../mobile/rpcAuth.fixture';

test('custom raw address uses secret-key backup and imports through desktop UI', async ({
  page
}) => {
  await desktopIO(page);
  await mockAccounts(page, { anyPublicKey: true });
  await page.setViewportSize({ width: 800, height: 640 });
  await page.goto('/');
  await createWallet(page);
  await page.reload();
  await page.locator('input[type="password"]').fill('synthetic desktop wallet password');
  await page.getByRole('button', { name: 'Unlock', exact: true }).click();
  await page.getByRole('button', { name: 'Manage wallets', exact: true }).click();
  await page.getByRole('button', { name: '+ Add Wallet', exact: true }).click();
  await page.getByRole('button', { name: 'Create New Wallet', exact: true }).click();
  await page.getByLabel('Custom address', { exact: false }).check();
  await page.getByLabel('Address starts with', { exact: true }).fill('2');
  await page.getByLabel('Recovery', { exact: true }).selectOption('raw');
  await page.getByText('Search settings', { exact: true }).click();
  await page.getByLabel('Compute with', { exact: true }).selectOption('cpu');
  await page.getByRole('button', { name: 'Find Address', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Use This Address', exact: true })).toBeVisible();
  const address = await page.locator('.search-status .address').textContent();
  expect(address).toMatch(/^2/);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(800);
  await page.screenshot({ path: 'test-results/desktop-vanity-found.png' });
  await page.getByRole('button', { name: 'Use This Address', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'Back Up Your Secret Key' })).toBeVisible();
  const secret = await page.getByLabel('Secret key · hex').inputValue();
  expect(secret).toMatch(/^[0-9a-f]{64}$/);
  expect(await page.evaluate(() => JSON.stringify(localStorage))).not.toContain(secret);
  await page.getByLabel('I saved my secret key somewhere private.').check();
  await page.getByRole('button', { name: 'Continue', exact: true }).click();
  await expect
    .poll(async () =>
      page.evaluate(async source => {
        const { getWallets } = await import(`${source}/vaultController.ts`);
        return (await getWallets()).map((wallet: { publicKey: string }) => wallet.publicKey);
      }, source)
    )
    .toContain(address);
});

test('first-run raw-key import validates before creating a vault', async ({ page }) => {
  await desktopIO(page);
  await mockAccounts(page, { anyPublicKey: true });
  await page.goto('/');
  await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
  await page.getByLabel('Import with', { exact: true }).selectOption('raw');
  await page.getByLabel('Secret key · hex', { exact: true }).fill('0'.repeat(64));
  await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
  await expect(page.getByText('Secret key must be nonzero', { exact: false })).toBeVisible();
  expect(
    await page.evaluate(() => localStorage.getItem('desktop-test-native-store'))
  ).not.toContain('fletch_vault_v1');
  await page.getByLabel('Secret key · hex', { exact: true }).fill('0'.repeat(63) + '1');
  await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
  await page.getByLabel('Password', { exact: true }).fill('synthetic raw import password');
  await page.getByLabel('Confirm Password', { exact: true }).fill('synthetic raw import password');
  await page.getByRole('checkbox', { name: 'Agree to terms' }).check();
  await page.getByRole('button', { name: 'Finish Setup', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Manage wallets: My Wallet', exact: true })
  ).toBeVisible();
  await expect
    .poll(async () =>
      page.evaluate(async source => {
        const { getWallets } = await import(`${source}/vaultController.ts`);
        return (await getWallets()).map((wallet: { publicKey: string }) => wallet.publicKey);
      }, source)
    )
    .toHaveLength(1);
});
