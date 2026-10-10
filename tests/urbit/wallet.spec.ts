import { test, expect, type Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';

const password = 'synthetic Urbit wallet password';
const passphrase = 'synthetic encrypted backup passphrase';
const mnemonic =
  'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float';
const appPath = process.env.NOCKSTER_TEST_URL ? '/apps/nockster/' : '/';

async function setup(page: Page) {
  await page.route('https://**', route => route.abort());
  await page.route('**/nockster/accounts/**', route => route.abort());
  if (process.env.NOCKSTER_TEST_CODE) {
    await page.request.post('/~/login', { form: { password: process.env.NOCKSTER_TEST_CODE } });
  }
  await page.goto(appPath);
  await expect(page.getByRole('button', { name: 'Create New Wallet', exact: true })).toBeVisible();
}

async function importWallet(page: Page) {
  await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
  await page.getByLabel('Wallet Name', { exact: true }).fill('Synthetic wallet');
  await page.getByLabel('Seed phrase', { exact: true }).fill(mnemonic);
  await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
  await page.getByLabel('Password', { exact: true }).fill(password);
  await page.getByLabel('Confirm Password', { exact: true }).fill(password);
  await page.getByRole('checkbox', { name: 'Agree to terms' }).click();
  await page.getByRole('button', { name: 'Finish Setup', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Manage wallets: Synthetic wallet' })
  ).toBeVisible();
}

test('production WASM wallet imports, locks, and restores an encrypted backup in a fresh profile', async ({
  page,
  browser
}) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await setup(page);
  await page.evaluate(() => localStorage.setItem('another-app', 'preserve'));
  await importWallet(page);
  const stored = await page.evaluate(() => localStorage.getItem('nockster:fletch_vault_v1'));
  expect(stored).not.toContain(mnemonic);
  expect(stored).not.toContain(password);
  await page.getByRole('button', { name: 'Backups', exact: true }).click();
  await page
    .getByLabel('Backup passphrase (at least 16 characters)', { exact: true })
    .fill(passphrase);
  await page.getByLabel('Confirm passphrase when creating a backup').fill(passphrase);
  const downloading = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Download backup', exact: true }).click();
  const download = await downloading;
  const backup = await readFile((await download.path())!);
  expect(backup.toString()).not.toContain('Synthetic wallet');
  expect(backup.toString()).not.toContain(mnemonic);
  expect(backup.toString()).not.toContain(passphrase);
  await page
    .getByLabel('Backup passphrase (at least 16 characters)', { exact: true })
    .fill(passphrase);
  await page
    .getByLabel('Restore encrypted file')
    .setInputFiles({ name: 'backup.json', mimeType: 'application/json', buffer: backup });
  await expect(page.getByRole('status')).toContainText('already has a vault');
  expect(await page.evaluate(() => localStorage.getItem('nockster:fletch_vault_v1'))).toBe(stored);

  if (process.env.NOCKSTER_TEST_URL) {
    await page
      .getByLabel('Backup passphrase (at least 16 characters)', { exact: true })
      .fill(passphrase);
    await page.getByLabel('Confirm passphrase when creating a backup').fill(passphrase);
    page.once('dialog', dialog => dialog.accept());
    await page.getByRole('button', { name: 'Save to ship', exact: true }).click();
    await expect(page.getByRole('status')).toHaveText('Encrypted backup saved to your ship.');
    const shipDownloading = page.waitForEvent('download');
    await page.getByRole('button', { name: 'Download ship backup', exact: true }).click();
    const shipBackup = await readFile((await (await shipDownloading).path())!);
    expect(JSON.parse(shipBackup.toString()).format).toBe('nockster-backup-1');
    expect(shipBackup.toString()).not.toContain(mnemonic);
    expect(shipBackup.toString()).not.toContain(passphrase);
  }

  const context = await browser.newContext();
  try {
    const fresh = await context.newPage();
    await setup(fresh);
    await fresh.getByRole('button', { name: 'Backups', exact: true }).click();
    await fresh
      .getByLabel('Backup passphrase (at least 16 characters)', { exact: true })
      .fill('incorrect backup passphrase');
    await fresh
      .getByLabel('Restore encrypted file')
      .setInputFiles({ name: 'backup.json', mimeType: 'application/json', buffer: backup });
    await expect(fresh.getByRole('status')).toContainText('Incorrect backup passphrase');
    expect(await fresh.evaluate(() => localStorage.getItem('nockster:fletch_vault_v1'))).toBeNull();
    await fresh
      .getByLabel('Backup passphrase (at least 16 characters)', { exact: true })
      .fill(passphrase);
    if (process.env.NOCKSTER_TEST_URL) {
      await fresh.getByRole('button', { name: 'Restore from ship', exact: true }).click();
    } else {
      await fresh
        .getByLabel('Restore encrypted file')
        .setInputFiles({ name: 'backup.json', mimeType: 'application/json', buffer: backup });
    }
    await expect(fresh.getByText('Wallet Locked', { exact: true })).toBeVisible();
    await fresh.locator('input[type="password"]').fill(password);
    await fresh.getByRole('button', { name: 'Unlock', exact: true }).click();
    await expect(
      fresh.getByRole('button', { name: 'Manage wallets: Synthetic wallet' })
    ).toBeVisible();
    await fresh.screenshot({ path: 'test-results/urbit-wallet.png' });
    await fresh.reload();
    await expect(fresh.getByText('Wallet Locked', { exact: true })).toBeVisible();
  } finally {
    await context.close();
  }
  expect(errors).toEqual([]);
});

test('wallet creation loads the bundled vanity worker and WASM without external assets', async ({
  page
}) => {
  const failures: string[] = [];
  page.on('response', response => {
    if (response.status() >= 400 && response.url().startsWith(new URL(page.url()).origin))
      failures.push(response.url());
  });
  await setup(page);
  await page.getByRole('button', { name: 'Create New Wallet', exact: true }).click();
  await page.getByRole('button', { name: 'Generate Wallet', exact: true }).click();
  await expect(
    page.getByText('Write down these 24 words in order.', { exact: false })
  ).toBeVisible();
  expect(failures).toEqual([]);
  await page.reload();
  await expect(page.getByRole('button', { name: 'Create New Wallet', exact: true })).toBeVisible();
});
