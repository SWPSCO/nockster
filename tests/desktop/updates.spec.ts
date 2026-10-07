import { test, expect } from '@playwright/test';
import { createWallet, desktopIO, source } from './native.fixture';

test('downloads automatically and locks the wallet only when restart is requested', async ({
  page
}) => {
  let installs = 0;
  let checks = 0;
  await desktopIO(page, undefined, async command => {
    if (command === 'desktop_update_install') {
      installs++;
      return;
    }
    if (command === 'desktop_update_check') checks++;
    return { phase: 'ready', version: '9.8.7', downloaded: 100, total: 100 };
  });
  await page.goto('/');
  await createWallet(page);
  await expect(page.getByText('Version 9.8.7 is ready.')).toBeVisible();
  expect(checks).toBe(1);
  expect(installs).toBe(0);
  await page.getByRole('button', { name: 'Restart to update' }).click();
  await expect(page.getByText('Installing update…', { exact: true })).toBeVisible();
  await expect.poll(() => installs).toBe(1);
  const locked = await page.evaluate(async source => {
    const { walletStore } = await import(`${source}/lib/stores/wallet.ts`);
    let value = false;
    walletStore.subscribe(state => (value = state.isLocked))();
    return value;
  }, source);
  expect(locked).toBe(true);
});

test('manual releases link to the trusted download site without installing', async ({ page }) => {
  await desktopIO(page, undefined, async command => {
    if (command === 'desktop_update_install') throw new Error('Must not install a manual release');
    return { phase: 'manual', version: '9.8.7', downloaded: 0 };
  });
  await page.goto('/');
  await expect(page.getByText('Version 9.8.7 needs a new installer.')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Restart to update' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Download from nockster.com' }).click();
  await expect
    .poll(() => page.evaluate(() => (window as any).__openedUrl))
    .toBe('https://nockster.com/');
});

test('failed checks offer retry and a manual download without blocking the wallet', async ({
  page
}) => {
  let calls = 0;
  await desktopIO(page, undefined, async () => {
    calls++;
    return calls === 1
      ? { phase: 'error', error: 'Network unavailable', downloaded: 0 }
      : { phase: 'current', downloaded: 0 };
  });
  await page.goto('/');
  await expect(page.getByText('Update check failed. Try again later.')).toBeVisible();
  await page.getByRole('button', { name: 'Download from nockster.com' }).click();
  await expect
    .poll(() => page.evaluate(() => (window as any).__openedUrl))
    .toBe('https://nockster.com/');
  await page.getByRole('button', { name: 'Check for updates' }).click();
  await expect(page.getByText('You’re up to date.')).toBeVisible();
});

test('installation errors retain a manual installer link and release the overlay', async ({
  page
}) => {
  await desktopIO(page, undefined, async command => {
    if (command === 'desktop_update_install') throw new Error('Installer permission denied');
    return { phase: 'ready', version: '9.8.7', downloaded: 100 };
  });
  await page.goto('/');
  await createWallet(page);
  await page.getByRole('button', { name: 'Restart to update' }).click();
  await expect(page.getByRole('alert')).toContainText('Installer permission denied');
  await expect(page.getByText('Installing update…', { exact: true })).toHaveCount(0);
  await page.getByRole('button', { name: 'Download from nockster.com' }).click();
  await expect
    .poll(() => page.evaluate(() => (window as any).__openedUrl))
    .toBe('https://nockster.com/');
});

test('download progress and the installer link remain reachable at the minimum window size', async ({
  page
}) => {
  let finish: (value: unknown) => void;
  await desktopIO(page, undefined, async command => {
    if (command === 'desktop_update_check')
      return new Promise(resolve => {
        finish = resolve;
      });
    return { phase: 'downloading', version: '9.8.7', downloaded: 50, total: 100 };
  });
  await page.setViewportSize({ width: 800, height: 640 });
  await page.goto('/');
  await expect(page.getByRole('progressbar', { name: 'App update download' })).toHaveAttribute(
    'value',
    '50'
  );
  finish!({ phase: 'manual', version: '9.8.7', downloaded: 0 });
  const link = page.getByRole('button', { name: 'Download from nockster.com' });
  await link.scrollIntoViewIfNeeded();
  await expect(link).toBeInViewport();
  await page.screenshot({ path: '/tmp/nockster-updater-manual-800.png' });
});
