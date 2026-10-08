import { test, expect } from '@playwright/test';
import { desktopIO, createWallet, source } from './native.fixture';
import { mockAccounts } from '../mobile/rpcAuth.fixture';

test('custom address uses seed backup and imports through desktop UI', async ({ page }) => {
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
  await page.getByText('Search settings', { exact: true }).click();
  await page.getByLabel('Compute with', { exact: true }).selectOption('cpu');
  await page.getByRole('button', { name: 'Find Address', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Use This Address', exact: true })).toBeVisible();
  const address = await page.locator('.search-status .address').textContent();
  expect(address).toMatch(/^2/);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(800);
  await page.screenshot({ path: 'test-results/desktop-vanity-found.png' });
  await page.getByRole('button', { name: 'Use This Address', exact: true }).click();
  await expect(page.locator('.seed-word-text')).toHaveCount(24);
  const words = await page.locator('.seed-word-text').allTextContents();
  expect(await page.evaluate(() => JSON.stringify(localStorage))).not.toContain(words.join(' '));
  await page.getByRole('button', { name: "I've Written It Down", exact: true }).click();
  const question = await page.locator('.question-label').innerText();
  const index = Number(question.match(/#(\d+)/)![1]) - 1;
  await page.getByRole('button', { name: words[index], exact: true }).click();
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

test('raw signing scalars are rejected before a vault is created', async ({ page }) => {
  await desktopIO(page);
  await page.goto('/');
  await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
  await expect(page.locator('#import-kind option')).toHaveText([
    '24-word seed phrase',
    'Extended private key · zprv',
    'Nockchain wallet file'
  ]);
  await page.getByLabel('Seed phrase', { exact: true }).fill('0'.repeat(63) + '1');
  await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
  await expect(page.getByText('Enter exactly 24 recovery words.')).toBeVisible();
  await page.getByLabel('Import with', { exact: true }).selectOption('extended');
  await page.getByLabel('Extended private key', { exact: true }).fill('0'.repeat(63) + '1');
  await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
  await expect(page.getByText('Enter an extended private key starting with zprv.')).toBeVisible();
  expect(
    await page.evaluate(() => localStorage.getItem('desktop-test-native-store'))
  ).not.toContain('fletch_vault_v1');
});

test('mining requires disclosure and opt-in, warns on long prefixes, and estimates from live speed', async ({
  page
}) => {
  const { expectedVanityAttempts } =
    await import('../../packages/wallet/src/lib/utils/vanityEstimate');
  await desktopIO(page);
  await page.setViewportSize({ width: 800, height: 640 });
  await page.addInitScript(() => {
    const NativeWorker = window.Worker;
    window.Worker = class extends NativeWorker {
      constructor(url: string | URL, options?: WorkerOptions) {
        super(url, options);
        (window as any).updateMiningRate = (rate: number) => this.postMessage({ rate });
      }
    };
  });
  await page.route('**/vanity/worker.js', route =>
    route.fulfill({
      contentType: 'text/javascript',
      body: `onmessage = ({data}) => {
      if (data.rate) postMessage({type:'progress',attempts:100,seconds:1,rate:data.rate,adapter:'CPU'});
    };`
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
  await page.getByRole('button', { name: 'Cancel Search', exact: true }).click();
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
    await page.getByLabel('Import with', { exact: true }).selectOption('extended');
    await expect(page.getByLabel('Extended private key', { exact: true })).toBeVisible();
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

test('vanity search survives navigation, cancels its worker, and can restart without reopening', async ({
  page
}) => {
  await desktopIO(page);
  await mockAccounts(page, { anyPublicKey: true });
  await page.addInitScript(() => {
    (window as any).liveMiners = 0;
    const NativeWorker = window.Worker;
    window.Worker = class extends NativeWorker {
      private mining = false;
      constructor(url: string | URL, options?: WorkerOptions) {
        super(url, options);
        this.mining = String(url).includes('vanity/worker.js');
        if (this.mining) (window as any).liveMiners++;
      }
      terminate() {
        if (this.mining) {
          (window as any).liveMiners--;
          this.mining = false;
        }
        super.terminate();
      }
    };
  });
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
  await page.getByLabel('Wallet name', { exact: true }).fill('Background wallet');
  await page.getByLabel('Address starts with', { exact: true }).fill('zzzzzzzzzzzzzz');
  await page.getByText('Search settings', { exact: true }).click();
  await page.getByLabel('Compute with', { exact: true }).selectOption('cpu');
  await page.getByRole('button', { name: 'Find Address', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Cancel Search', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Continue in Background', exact: true }).click();
  await expect(page.getByText('Vanity search running', { exact: true })).toBeVisible();
  const nav = page.getByRole('navigation', { name: 'Main' });
  await nav.getByRole('button', { name: 'Receive', exact: true }).click();
  await expect(page.getByAltText('QR Code')).toBeVisible();
  expect(await page.evaluate(() => (window as any).liveMiners)).toBe(1);
  await page.getByRole('button', { name: 'Return to Search', exact: true }).click();
  await expect(page.getByLabel('Wallet name', { exact: true })).toHaveValue('Background wallet');
  await expect(page.getByLabel('Address starts with', { exact: true })).toHaveValue(
    'zzzzzzzzzzzzzz'
  );
  await page.getByRole('button', { name: 'Cancel Search', exact: true }).click();
  expect(await page.evaluate(() => (window as any).liveMiners)).toBe(0);
  await expect(page.getByLabel('Address starts with', { exact: true })).toBeEnabled();
  await page.getByLabel('Address starts with', { exact: true }).fill('2');
  await page.getByRole('button', { name: 'Find Address', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Use This Address', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Continue in Background', exact: true }).click();
  await expect(page.getByText('Vanity address ready', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Return to Search', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Use This Address', exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Discard Address', exact: true }).click();
  await page.getByLabel('Address starts with', { exact: true }).fill('zzzzzzzzzzzzzz');
  await page.getByRole('button', { name: 'Find Address', exact: true }).click();
  await page.getByRole('button', { name: 'Continue in Background', exact: true }).click();
  await page.getByRole('button', { name: 'Cancel Search', exact: true }).click();
  await expect(page.locator('.vanity-banner')).toBeHidden();
  expect(await page.evaluate(() => (window as any).liveMiners)).toBe(0);
  await nav.getByRole('button', { name: 'Send', exact: true }).click();
  await expect(page.getByRole('tab', { name: 'Payment', exact: true })).toBeVisible();
  await page.evaluate(async source => {
    const { vanitySession } = await import(`${source}/lib/stores/vanitySession.ts`);
    vanitySession.start('Lock test', {
      prefix: 'zzzzzzzzzzzzzz',
      keyMode: 'mnemonic',
      backend: 'cpu'
    });
  }, source);
  await expect(page.getByText('Vanity search running', { exact: true })).toBeVisible();
  await page.keyboard.press('Control+Shift+L');
  await expect(page.getByText('Wallet Locked', { exact: true })).toBeVisible();
  expect(await page.evaluate(() => (window as any).liveMiners)).toBe(0);
  await expect(page.locator('.vanity-banner')).toBeHidden();
});
