import {
  test,
  expect,
  chromium,
  type BrowserContext,
  type Page,
  type Worker
} from '@playwright/test';
import { mkdtemp, cp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const password = 'synthetic popup restoration test password';

async function withWallet(
  run: (context: BrowserContext, worker: Worker, origin: string) => Promise<void>
) {
  const dir = await mkdtemp(join(tmpdir(), 'nockster-popup-restoration-'));
  const extension = join(dir, 'extension');
  await cp(resolve('apps/extension/ext'), extension, { recursive: true });
  await writeFile(
    join(extension, 'test.html'),
    '<!doctype html><title>Popup restoration test</title>'
  );
  const context = await chromium.launchPersistentContext(join(dir, 'profile'), {
    channel: 'chromium',
    headless: true,
    viewport: { width: 357, height: 600 },
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
        return Promise.reject(new Error('Network unavailable in popup restoration test'));
      };
    });
    const origin = `chrome-extension://${new URL(worker.url()).host}`;
    const setup = await context.newPage();
    await setup.goto(`${origin}/test.html`);
    await setup.evaluate(async password => {
      for (const message of [
        { type: 'vault:newVault', password },
        {
          type: 'vault:importWallet',
          nickname: 'Existing wallet',
          key: 'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float'
        }
      ]) {
        const result = await chrome.runtime.sendMessage(message);
        if (!result.success) throw new Error(result.error);
      }
    }, password);
    await setup.goto(`${origin}/dist/index.html`);
    await expect(
      setup.getByRole('button', { name: 'Manage wallets: Existing wallet', exact: true })
    ).toBeVisible();
    await setup.close();
    await run(context, worker, origin);
  } finally {
    await context.close();
    await rm(dir, { recursive: true, force: true });
  }
}

async function watchOnboarding(page: Page) {
  await page.addInitScript(() => {
    (window as any).sawOnboarding = false;
    new MutationObserver(() => {
      if (
        [...document.querySelectorAll('button')].some(
          button => button.textContent?.trim() === 'Create New Wallet'
        )
      )
        (window as any).sawOnboarding = true;
    }).observe(document, { childList: true, subtree: true });
  });
}

test('reopening during unlock waits for the existing wallet', async () => {
  await withWallet(async (context, worker, origin) => {
    const page = await context.newPage();
    await page.goto(`${origin}/test.html`);
    await page.evaluate(async () => {
      const result = await chrome.runtime.sendMessage({ type: 'vault:lock' });
      if (!result.success) throw new Error(result.error);
    });
    await page.goto(`${origin}/dist/index.html`);
    await expect(page.getByText('Wallet Locked', { exact: true })).toBeVisible();
    await worker.evaluate(() => {
      const original = chrome.storage.local.set.bind(chrome.storage.local);
      (globalThis as any).unlockWritePending = false;
      chrome.storage.local.set = (async (items: Record<string, unknown>) => {
        if (items.fletch_vault_v1) {
          chrome.storage.local.set = original;
          (globalThis as any).unlockWritePending = true;
          await new Promise<void>(resolve => {
            (globalThis as any).finishUnlockWrite = resolve;
          });
        }
        return original(items);
      }) as typeof chrome.storage.local.set;
    });
    await page.getByPlaceholder('Enter password', { exact: true }).fill(password);
    await page.getByRole('button', { name: 'Unlock', exact: true }).click();
    await expect
      .poll(() => worker.evaluate(() => (globalThis as any).unlockWritePending))
      .toBe(true);
    await page.close();

    const reopened = await context.newPage();
    await watchOnboarding(reopened);
    await reopened.goto(`${origin}/dist/index.html`);
    await expect(reopened.getByText('Loading your wallet...', { exact: true })).toBeVisible();
    await reopened.screenshot({ path: '/tmp/nockster-restoration-loading.png' });
    await worker.evaluate(() => (globalThis as any).finishUnlockWrite());
    await expect(
      reopened.getByRole('button', { name: 'Manage wallets: Existing wallet', exact: true })
    ).toBeVisible();
    expect(await reopened.evaluate(() => (window as any).sawOnboarding)).toBe(false);
    await reopened.reload();
    await expect(
      reopened.getByRole('button', { name: 'Manage wallets: Existing wallet', exact: true })
    ).toBeVisible();
  });
});

test('reopening while wallet data loads retains the selected wallet', async () => {
  await withWallet(async (context, worker, origin) => {
    await worker.evaluate(() => {
      const fetchLocal = globalThis.fetch;
      (globalThis as any).networkPending = false;
      (globalThis as any).finishNetworkRequests = [];
      globalThis.fetch = (input, init) => {
        if (new Request(input, init).url.startsWith('chrome-extension:'))
          return fetchLocal(input, init);
        (globalThis as any).networkPending = true;
        return new Promise((_resolve, reject) => {
          (globalThis as any).finishNetworkRequests.push(() =>
            reject(new Error('Network unavailable'))
          );
        });
      };
    });
    const page = await context.newPage();
    await watchOnboarding(page);
    await page.goto(`${origin}/dist/index.html`);
    await expect(
      page.getByRole('button', { name: 'Manage wallets: Existing wallet', exact: true })
    ).toBeVisible();
    await expect(page.getByText('Loading wallet data...', { exact: true })).toBeVisible();
    await expect.poll(() => worker.evaluate(() => (globalThis as any).networkPending)).toBe(true);
    await page.close();
    const reopened = await context.newPage();
    await watchOnboarding(reopened);
    await reopened.goto(`${origin}/dist/index.html`);
    await expect(
      reopened.getByRole('button', { name: 'Manage wallets: Existing wallet', exact: true })
    ).toBeVisible();
    await expect(reopened.getByText('Loading wallet data...', { exact: true })).toBeVisible();
    expect(await reopened.evaluate(() => (window as any).sawOnboarding)).toBe(false);
    expect(
      await reopened.evaluate(async () => {
        const { walletState } = await chrome.storage.local.get('walletState');
        return {
          names: walletState.wallets.map((wallet: { name: string }) => wallet.name),
          active: walletState.activeWallet?.name
        };
      })
    ).toEqual({ names: ['Existing wallet'], active: 'Existing wallet' });
    await worker.evaluate(() =>
      (globalThis as any).finishNetworkRequests.forEach((finish: () => void) => finish())
    );
  });
});

for (const failingType of ['vault:status', 'vault:getWallets']) {
  test(`${failingType} failures preserve the wallet and offer retry`, async () => {
    await withWallet(async (context, _worker, origin) => {
      const page = await context.newPage();
      if (failingType === 'vault:getWallets') {
        await page.goto(`${origin}/test.html`);
        await page.evaluate(() => chrome.storage.local.set({ theme: 'dark' }));
      }
      await watchOnboarding(page);
      await page.addInitScript(failingType => {
        (window as any).failVaultRead = true;
        const send = chrome.runtime.sendMessage.bind(chrome.runtime);
        chrome.runtime.sendMessage = ((
          message: { type: string },
          callback: (response: unknown) => void
        ) => {
          if (message.type === failingType && (window as any).failVaultRead) {
            queueMicrotask(() =>
              callback({ success: false, error: 'Vault temporarily unavailable' })
            );
            return;
          }
          return send(message, callback);
        }) as typeof chrome.runtime.sendMessage;
      }, failingType);
      await page.goto(`${origin}/dist/index.html`);
      await expect(page.getByRole('heading', { name: 'Unable to load your wallet' })).toBeVisible();
      expect(await page.evaluate(() => (window as any).sawOnboarding)).toBe(false);
      const before = await page.evaluate(async () => {
        const stored = await chrome.storage.local.get([
          'fletch_vault_v1',
          'walletState',
          'routerState'
        ]);
        return JSON.stringify(stored);
      });
      await page.getByRole('button', { name: 'Try again', exact: true }).click();
      await expect(page.getByRole('heading', { name: 'Unable to load your wallet' })).toBeVisible();
      expect(
        await page.evaluate(async () =>
          JSON.stringify(
            await chrome.storage.local.get(['fletch_vault_v1', 'walletState', 'routerState'])
          )
        )
      ).toBe(before);
      await page.screenshot({ path: `/tmp/nockster-restoration-${failingType.slice(6)}.png` });
      await page.evaluate(() => {
        (window as any).failVaultRead = false;
      });
      await page.getByRole('button', { name: 'Try again', exact: true }).click();
      await expect(
        page.getByRole('button', { name: 'Manage wallets: Existing wallet', exact: true })
      ).toBeVisible();
      expect(await page.evaluate(() => (window as any).sawOnboarding)).toBe(false);
    });
  });
}
