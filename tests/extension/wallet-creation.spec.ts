import { test, expect, chromium } from '@playwright/test';
import { generateExtendedKey } from '../vanity.fixture';
import { mkdtemp, cp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

for (const recovery of ['mnemonic', 'extended']) {
  test(`${recovery} wallets appear immediately and remain selected after storage synchronization`, async () => {
    const dir = await mkdtemp(join(tmpdir(), 'nockster-wallet-creation-'));
    const extension = join(dir, 'extension');
    await cp(resolve('apps/extension/ext'), extension, { recursive: true });
    await writeFile(
      join(extension, 'test.html'),
      '<!doctype html><title>Wallet creation test</title>'
    );
    const context = await chromium.launchPersistentContext(join(dir, 'profile'), {
      channel: 'chromium',
      headless: true,
      args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`]
    });
    try {
      await context.route('https://**', route => route.abort());
      const worker = context.serviceWorkers()[0] || (await context.waitForEvent('serviceworker'));
      await worker.evaluate(() => {
        const fetchLocal = globalThis.fetch;
        globalThis.fetch = (input, init) => {
          if (new Request(input, init).url.startsWith('chrome-extension:'))
            return fetchLocal(input, init);
          return Promise.reject(new Error('Network unavailable in wallet creation test'));
        };
      });
      const origin = `chrome-extension://${new URL(worker.url()).host}`;
      const page = await context.newPage();
      await page.setViewportSize({ width: 360, height: 600 });
      await page.goto(`${origin}/test.html`);
      await page.evaluate(async () => {
        const created = await chrome.runtime.sendMessage({
          type: 'vault:newVault',
          password: 'synthetic wallet creation test password'
        });
        if (!created.success) throw new Error(created.error);
        const imported = await chrome.runtime.sendMessage({
          type: 'vault:importWallet',
          nickname: 'Existing wallet',
          key: 'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float'
        });
        if (!imported.success) throw new Error(imported.error);
      });
      await page.goto(`${origin}/dist/index.html`);
      await page
        .getByRole('button', { name: 'Manage wallets: Existing wallet', exact: true })
        .click();
      await page.getByRole('button', { name: '+ Add Wallet', exact: true }).click();
      await page
        .getByRole('button', {
          name: recovery === 'extended' ? 'Import Wallet' : 'Create New Wallet',
          exact: true
        })
        .click();
      if (recovery === 'extended') {
        const candidate = await generateExtendedKey(page);
        await page.getByLabel('Import with', { exact: true }).selectOption('extended');
        await page.getByLabel('Extended private key', { exact: true }).fill(candidate.key);
        await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
        await expect
          .poll(async () =>
            page.evaluate(async () => {
              const { data } = await chrome.runtime.sendMessage({ type: 'vault:getWallets' });
              return data.wallets.find(
                (wallet: { nickname: string }) => wallet.nickname === 'My Wallet 2'
              )?.publicKey;
            })
          )
          .toBe(candidate.address);
        expect(
          await page.evaluate(async () => JSON.stringify(await chrome.storage.local.get(null)))
        ).not.toContain(candidate.key);
      } else {
        await page.getByRole('button', { name: 'Generate Wallet', exact: true }).click();
        await expect(page.locator('.seed-word-text')).toHaveCount(24);
        const words = await page.locator('.seed-word-text').allTextContents();
        await page.getByRole('button', { name: "I've Written It Down", exact: true }).click();
        const question = await page.locator('.question-label').innerText();
        const index = Number(question.match(/#(\d+)/)![1]) - 1;
        await page.getByRole('button', { name: words[index], exact: true }).click();
        await page.getByRole('button', { name: 'Continue', exact: true }).click();
      }

      const savedWallets = () =>
        page.evaluate(async () => {
          const { walletState } = await chrome.storage.local.get('walletState');
          return {
            names: walletState?.wallets.map((wallet: { name: string }) => wallet.name),
            active: walletState?.activeWallet?.name
          };
        });
      await expect
        .poll(savedWallets)
        .toEqual({ names: ['Existing wallet', 'My Wallet 2'], active: 'My Wallet 2' });
      await page.getByRole('button', { name: 'Manage wallets: My Wallet 2', exact: true }).click();
      await expect(page.getByText('Existing wallet', { exact: true })).toBeVisible();
      await expect(page.getByText('My Wallet 2', { exact: true })).toBeVisible();
      await page.reload();
      await expect(page.getByText('Existing wallet', { exact: true })).toBeVisible();
      await expect(page.getByText('My Wallet 2', { exact: true })).toBeVisible();
    } finally {
      await context.close();
      await rm(dir, { recursive: true, force: true });
    }
  });
}
