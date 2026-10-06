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
  await page.getByRole('button', { name: 'Generate a custom address', exact: true }).click();
  await page.getByLabel('Enable custom address generation', { exact: true }).check();
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

test('mining requires disclosure and opt-in, warns on long prefixes, and estimates from live speed', async ({
  page
}) => {
  const { expectedVanityAttempts } =
    await import('../../packages/wallet/src/lib/utils/vanityEstimate');
  await desktopIO(page);
  await page.setViewportSize({ width: 800, height: 640 });
  await page.route('**/vanity/miner.js', route =>
    route.fulfill({
      contentType: 'text/javascript',
      body: `export function mineAddress({ onProgress, signal }) {
      window.updateMiningRate = rate => onProgress({ type: 'progress', attempts: 100, seconds: 1, rate, adapter: 'CPU' });
      return new Promise((resolve, reject) => signal.addEventListener('abort', () => reject(new DOMException('Stopped', 'AbortError'))));
    }`
    })
  );
  await page.goto('/');
  await page.getByRole('button', { name: 'Create New Wallet', exact: true }).click();
  const disclosure = page.getByRole('button', { name: 'Generate a custom address', exact: true });
  const enabled = page.getByLabel('Enable custom address generation', { exact: true });
  const prefix = page.getByLabel('Address starts with', { exact: true });
  await expect(disclosure).toHaveAttribute('aria-expanded', 'false');
  await expect(enabled).toBeHidden();
  await expect(page.getByRole('button', { name: 'Generate Wallet', exact: true })).toBeEnabled();
  await page.screenshot({ path: '/tmp/vanity-create-collapsed.png' });
  await disclosure.focus();
  await page.keyboard.press('Enter');
  await expect(enabled).not.toBeChecked();
  await expect(prefix).toBeDisabled();
  await enabled.check();
  await prefix.fill('nock');
  await expect(page.getByText('This might take a while!', { exact: true })).toBeHidden();
  await prefix.fill('nocky');
  await expect(page.getByText('This might take a while!', { exact: true })).toBeVisible();
  await page.screenshot({ path: '/tmp/vanity-create-expanded.png' });
  await page.getByRole('button', { name: 'Find Address', exact: true }).click();
  await expect(page.getByText('Measuring generation speed…', { exact: true })).toBeVisible();
  await page.waitForFunction(() => typeof (window as any).updateMiningRate === 'function');
  const attempts = expectedVanityAttempts('nocky')!;
  await page.evaluate(rate => (window as any).updateMiningRate(rate), attempts / 90);
  await expect(
    page.getByText('Estimated average: about 1.5 minutes', { exact: true })
  ).toBeVisible();
  await page.evaluate(rate => (window as any).updateMiningRate(rate), attempts / 45);
  await expect(
    page.getByText('Estimated average: about 45 seconds', { exact: true })
  ).toBeVisible();
  await page.screenshot({ path: '/tmp/vanity-create-estimate.png' });
  await page.getByRole('button', { name: 'Stop Search', exact: true }).click();
  await expect(page.locator('.estimate')).toBeHidden();
  await disclosure.click();
  await expect(page.getByRole('button', { name: 'Generate Wallet', exact: true })).toBeEnabled();
  await disclosure.click();
  await expect(enabled).not.toBeChecked();
  await expect(prefix).toBeDisabled();
});

test('import method options follow each wallet theme', async ({ page }) => {
  await desktopIO(page);
  await page.goto('/');
  await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
  for (const theme of ['light', 'dark', 'dark-blue', 'purple']) {
    await page.evaluate(
      async ({ source, theme }) => {
        const { currentTheme } = await import(`${source}/lib/stores/theme.ts`);
        currentTheme.setTheme(theme);
      },
      { source, theme }
    );
    await expect
      .poll(async () =>
        page.getByLabel('Import with', { exact: true }).evaluate(select => {
          const control = getComputedStyle(select);
          const option = getComputedStyle(select.querySelector('option')!);
          return {
            scheme: control.colorScheme,
            readable: option.color !== option.backgroundColor,
            sameSurface: option.backgroundColor === control.backgroundColor
          };
        })
      )
      .toEqual({ scheme: theme === 'light' ? 'light' : 'dark', readable: true, sameSurface: true });
    if (theme === 'light' || theme === 'dark')
      await page.screenshot({ path: `/tmp/vanity-import-${theme}.png` });
    await page.getByLabel('Import with', { exact: true }).selectOption('raw');
    await expect(page.getByLabel('Secret key · hex', { exact: true })).toBeVisible();
  }
});

test('first-run zprv import preserves the generated address through vault recovery', async ({
  page
}) => {
  const { generateExtendedKey } = await import('../vanity.fixture');
  await desktopIO(page);
  await mockAccounts(page, { anyPublicKey: true });
  await page.goto('/');
  const candidate = await generateExtendedKey(page);
  await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
  await page.getByLabel('Import with', { exact: true }).selectOption('extended');
  await page.getByLabel('Extended private key', { exact: true }).fill(candidate.key);
  await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
  const password = 'synthetic extended-key import password';
  await page.getByLabel('Password', { exact: true }).fill(password);
  await page.getByLabel('Confirm Password', { exact: true }).fill(password);
  await page.getByRole('checkbox', { name: 'Agree to terms' }).check();
  await page.getByRole('button', { name: 'Finish Setup', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Manage wallets: My Wallet', exact: true })
  ).toBeVisible();
  expect(await page.evaluate(() => JSON.stringify(localStorage))).not.toContain(candidate.key);
  await page.reload();
  await page.locator('input[type="password"]').fill(password);
  await page.getByRole('button', { name: 'Unlock', exact: true }).click();
  await expect
    .poll(async () =>
      page.evaluate(async source => {
        const { getWallets } = await import(`${source}/vaultController.ts`);
        return (await getWallets()).map((wallet: { publicKey: string }) => wallet.publicKey);
      }, source)
    )
    .toEqual([candidate.address]);
});
