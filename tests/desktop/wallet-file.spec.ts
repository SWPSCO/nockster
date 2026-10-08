import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import * as iris from '@nockbox/iris-wasm/iris_wasm.js';
import { desktopIO, source } from './native.fixture';
import { mockAccounts } from '../mobile/rpcAuth.fixture';

iris.initSync({ module: readFileSync('node_modules/@nockbox/iris-wasm/iris_wasm_bg.wasm') });
const phrase =
  'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float';
const address = '4Lu3cSW34WPwvDkTwKh7xB6yMZrvh3bFW7w26UDJVdboxADGkr2bTnL';
const atom = (text: string) => Buffer.from(text).reverse().toString('hex');
function walletFile(phrases: string[], tag = 'seed') {
  let list: any = '0';
  for (const phrase of phrases.toReversed()) list = [['0', [atom(tag), atom(phrase)]], list];
  return {
    name: 'keys.export',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(iris.jam(list))
  };
}

test.beforeEach(async ({ page }) => {
  await desktopIO(page);
  await mockAccounts(page, { anyPublicKey: true });
  await page.goto('/');
  await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
  await page.getByLabel('Import with', { exact: true }).selectOption('file');
});

test('keys.export imports the seed wallet and restores it after locking', async ({ page }) => {
  await page.locator('#wallet-file').setInputFiles(walletFile([phrase]));
  await expect(page.getByText(`Wallet address: ${address}`, { exact: true })).toBeVisible();
  await page.screenshot({ path: 'test-results/desktop-wallet-file.png' });
  await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
  const password = 'synthetic wallet file password';
  await page.getByLabel('Password', { exact: true }).fill(password);
  await page.getByLabel('Confirm Password', { exact: true }).fill(password);
  await page.getByRole('checkbox', { name: 'Agree to terms' }).check();
  await page.getByRole('button', { name: 'Finish Setup', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Manage wallets: My Wallet', exact: true })
  ).toBeVisible();
  expect(await page.evaluate(() => JSON.stringify(localStorage))).not.toContain(phrase);
  await page.reload();
  await page.locator('input[type="password"]').fill(password);
  await page.getByRole('button', { name: 'Unlock', exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(async source => {
        const { getWallets } = await import(`${source}/vaultController.ts`);
        return (await getWallets()).map((wallet: { publicKey: string }) => wallet.publicKey);
      }, source)
    )
    .toEqual([address]);
});

test('multiple seeds require a choice and invalid files clear the selected wallet', async ({
  page
}) => {
  await page
    .locator('#wallet-file')
    .setInputFiles(walletFile([phrase, phrase, 'abandon '.repeat(23) + 'art']));
  await expect(page.locator('#file-wallet option')).toHaveCount(3);
  await expect(page.getByRole('button', { name: 'Import Wallet', exact: true })).toBeDisabled();
  await page.getByLabel('Wallet to import', { exact: true }).selectOption('0');
  await expect(page.getByRole('button', { name: 'Import Wallet', exact: true })).toBeEnabled();
  await page
    .locator('#wallet-file')
    .setInputFiles({
      name: 'bad.export',
      mimeType: 'application/octet-stream',
      buffer: Buffer.from('not a wallet')
    });
  await expect(page.locator('.error-message')).toBeVisible();
  await expect(page.locator('#file-wallet')).toBeHidden();
  await expect(page.getByRole('button', { name: 'Import Wallet', exact: true })).toBeDisabled();
  await page
    .locator('#wallet-file')
    .setInputFiles(walletFile(['synthetic public wallet'], 'label'));
  await expect(
    page.getByText('This wallet file has no seed phrase.', { exact: false })
  ).toBeVisible();
  await page.locator('#wallet-file').setInputFiles(walletFile(['invalid seed']));
  await expect(
    page.getByText('The wallet file must contain a 24-word seed phrase.', { exact: true })
  ).toBeVisible();
  await page
    .locator('#wallet-file')
    .setInputFiles({
      name: 'large.export',
      mimeType: 'application/octet-stream',
      buffer: Buffer.alloc(1024 * 1024 + 1)
    });
  await expect(
    page.getByText('Choose a nonempty Nockchain wallet file smaller than 1 MB.')
  ).toBeVisible();
  expect(
    await page.evaluate(() => localStorage.getItem('desktop-test-native-store'))
  ).not.toContain('fletch_vault_v1');
});
