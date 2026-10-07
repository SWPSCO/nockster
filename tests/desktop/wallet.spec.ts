import { test, expect } from '@playwright/test';
import release from '../../release-version.json' with { type: 'json' };
import { mockAccounts } from '../mobile/rpcAuth.fixture';
import { desktopIO, createWallet, password, mnemonic, source } from './native.fixture';

test.beforeEach(async ({ page }) => {
  await desktopIO(page);
});

test('desktop wallet persists ciphertext, unlocks, navigates, opens links externally, and locks', async ({
  page
}) => {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Create New Wallet', exact: true })).toBeVisible();
  await createWallet(page);
  await page.reload();
  await expect(page.getByText('Wallet Locked', { exact: true })).toBeVisible();
  await expect(
    page
      .getByRole('navigation', { name: 'Main' })
      .getByRole('button', { name: 'Send', exact: true })
  ).toBeDisabled();
  await page.locator('input[type="password"]').fill('incorrect password');
  await page.getByRole('button', { name: 'Unlock', exact: true }).click();
  await expect(page.getByText('Incorrect password', { exact: true })).toBeVisible();
  await page.locator('input[type="password"]').fill(password);
  await page.getByRole('button', { name: 'Unlock', exact: true }).click();
  const nav = page.getByRole('navigation', { name: 'Main' });
  await expect(nav.getByRole('button', { name: 'Overview' })).toBeEnabled();
  await expect(page.locator('.balance-amount')).toContainText('7,655.77');
  expect((await page.locator('.nockster-wallet').boundingBox())!.width).toBeGreaterThan(700);
  await expect(page.getByRole('button', { name: 'Settings menu' })).toBeHidden();
  await page.screenshot({ path: 'test-results/desktop-overview.png' });
  await nav.getByRole('button', { name: 'Receive', exact: true }).click();
  await expect(page.getByAltText('QR Code')).toBeVisible();
  await page.getByRole('link', { name: 'View address on Nockblocks' }).click();
  await expect
    .poll(() => page.evaluate(() => (window as any).__openedUrl))
    .toMatch(/^https:\/\/nockblocks.com\/address\//);
  await nav.getByRole('button', { name: 'Activity', exact: true }).click();
  await expect(page.getByText('Wallet Locked', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await expect(page.getByText('Pop Out Window', { exact: true })).toHaveCount(0);
  await expect(
    page.locator('.setting-item')
      .filter({ has: page.getByText('Version', { exact: true }) })
      .locator('.setting-value')
  ).toHaveText(process.env.NOCKSTER_VERSION || release.version);
  await page.keyboard.press('Control+Shift+L');
  await expect(page.getByText('Wallet Locked', { exact: true })).toBeVisible();
  const saved = await page.evaluate(() => localStorage.getItem('desktop-test-native-store'));
  expect(saved).toContain('fletch_vault_v1');
  expect(saved).not.toContain(mnemonic);
  expect(saved).not.toContain(password);
  expect(saved).not.toContain('ak_live_test_secret');
  expect(JSON.parse(saved!).fletch_vault_v1.kdf).toMatchObject({
    kdfType: 'Argon2id',
    m: 128 * 1024,
    t: 3,
    p: 1
  });
});

test('a copied wallet file unlocks in a fresh profile using only its password', async ({
  page,
  browser
}) => {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Create New Wallet', exact: true })).toBeVisible();
  await createWallet(page);
  const backup = await page.evaluate(() => localStorage.getItem('desktop-test-native-store'));
  const address = await page.evaluate(async source => {
    const { getWalletsFromVault } = await import(`${source}/lib/utils/vaultBridge.ts`);
    const result = await getWalletsFromVault();
    if (!result.success) throw new Error(result.error);
    return result.wallets[0].addresses[0];
  }, source);
  const fresh = await browser.newContext();
  try {
    const restored = await fresh.newPage();
    await desktopIO(restored);
    await restored.goto(page.url());
    await expect(
      restored.getByRole('button', { name: 'Create New Wallet', exact: true })
    ).toBeVisible();
    await restored.evaluate(
      value => localStorage.setItem('desktop-test-native-store', value!),
      backup
    );
    await restored.reload();
    await expect(restored.getByText('Wallet Locked', { exact: true })).toBeVisible();
    await restored.locator('input[type="password"]').fill(password);
    await restored.getByRole('button', { name: 'Unlock', exact: true }).click();
    await expect(
      restored.getByRole('navigation', { name: 'Main' }).getByRole('button', { name: 'Overview' })
    ).toBeEnabled();
    const restoredAddress = await restored.evaluate(async source => {
      const { getWalletsFromVault } = await import(`${source}/lib/utils/vaultBridge.ts`);
      const result = await getWalletsFromVault();
      if (!result.success) throw new Error(result.error);
      return result.wallets[0].addresses[0];
    }, source);
    expect(restoredAddress).toBe(address);
  } finally {
    await fresh.close();
  }
});

test('importing a distinct wallet preserves both wallets across restart', async ({ page }) => {
  await mockAccounts(page, { anyPublicKey: true });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Create New Wallet', exact: true })).toBeVisible();
  await createWallet(page);
  await page.reload();
  await page.locator('input[type="password"]').fill(password);
  await page.getByRole('button', { name: 'Unlock', exact: true }).click();
  const secondMnemonic = await page.evaluate(async source => {
    const { generateKey } = await import(`${source}/vaultController.ts`);
    return (await generateKey()).join(' ');
  }, source);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'Import →', exact: true }).click();
  await page.getByLabel('Wallet Name', { exact: true }).fill('Second wallet');
  await page.getByLabel('Seed phrase', { exact: true }).fill(secondMnemonic);
  await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
  await expect(page.locator('.desktop-wallets')).toContainText('Savings');
  await expect(page.locator('.desktop-wallets')).toContainText('Second wallet');
  await page.reload();
  await page.locator('input[type="password"]').fill(password);
  await page.getByRole('button', { name: 'Unlock', exact: true }).click();
  await expect(page.locator('.desktop-wallets')).toContainText('Savings');
  await expect(page.locator('.desktop-wallets')).toContainText('Second wallet');
  const names = await page.evaluate(async source => {
    const { getWallets } = await import(`${source}/vaultController.ts`);
    return (await getWallets()).map(wallet => wallet.nickname).sort();
  }, source);
  expect(names).toEqual(['Savings', 'Second wallet']);
});

test('onboarding never persists plaintext signing secrets and fits the minimum window', async ({
  page
}) => {
  await page.setViewportSize({ width: 800, height: 640 });
  await page.goto('/');
  await page.getByRole('button', { name: 'Create New Wallet', exact: true }).click();
  await page.getByRole('button', { name: 'Generate Wallet', exact: true }).click();
  await expect(
    page.getByText('Write down these 24 words in order.', { exact: false })
  ).toBeVisible();
  const seed = await page.evaluate(async source => {
    const { getPendingWallet } = await import(`${source}/lib/utils/vaultBridge.ts`);
    const { walletStore } = await import(`${source}/lib/stores/wallet.ts`);
    await walletStore.saveToStorage();
    return getPendingWallet().key.split(' ');
  }, source);
  expect(seed).toHaveLength(24);
  const saved = await page.evaluate(() => localStorage.getItem('desktop-test-native-store'));
  expect(saved).not.toContain('seedPhrase');
  expect(saved).not.toContain('privateKey');
  await page.screenshot({ path: 'test-results/desktop-onboarding.png' });
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(800);
  await page.reload();
  await expect(page.getByRole('button', { name: 'Create New Wallet', exact: true })).toBeVisible();
});

test('an unreadable vault offers retry without replacing the persisted data', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Create New Wallet', exact: true })).toBeVisible();
  const corrupt = JSON.stringify({ fletch_vault_v1: 'unreadable-vault' });
  await page.evaluate(value => localStorage.setItem('desktop-test-native-store', value), corrupt);
  await page.reload();
  await expect(page.getByRole('alert')).toContainText('Unable to load your wallet');
  await page.getByRole('button', { name: 'Try again' }).click();
  await expect(page.getByRole('alert')).toContainText('Unable to load your wallet');
  expect(await page.evaluate(() => localStorage.getItem('desktop-test-native-store'))).toBe(
    corrupt
  );
});

test('clearing wallet data requires confirmation and removes the native vault', async ({
  page
}) => {
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Create New Wallet', exact: true })).toBeVisible();
  await createWallet(page);
  await page.reload();
  await page.locator('input[type="password"]').fill(password);
  await page.getByRole('button', { name: 'Unlock', exact: true }).click();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  page.once('dialog', dialog => dialog.dismiss());
  await page.getByRole('button', { name: 'Clear All Data', exact: true }).click();
  expect(await page.evaluate(() => localStorage.getItem('desktop-test-native-store'))).toContain(
    'fletch_vault_v1'
  );
  page.once('dialog', dialog => dialog.accept());
  await page.getByRole('button', { name: 'Clear All Data', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Create New Wallet', exact: true })).toBeVisible();
  expect(await page.evaluate(() => localStorage.getItem('desktop-test-native-store'))).toBeNull();
  expect(
    await page.evaluate(() =>
      JSON.parse(sessionStorage.getItem('desktop-test-removed-keys') || '[]')
    )
  ).toEqual(expect.arrayContaining(['fletch_vault_v1', 'walletState']));
});
