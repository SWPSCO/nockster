import { test, expect, type Page } from '@playwright/test';

async function render(page: Page, screen: 'import' | 'rename' | 'wallet') {
  await page.route('https://**', route => route.abort());
  await page.goto('/tests/extension/wallet-ui.html');
  await page.evaluate(async screen => {
    const path = '/tests/extension/wallet-ui.fixture.ts';
    await (await import(/* @vite-ignore */ path)).render(screen);
  }, screen);
}

test('extension import suggests the wallet count and keeps duplicate names editable', async ({
  page
}) => {
  await render(page, 'import');
  const name = page.getByLabel('Wallet Name', { exact: true });
  await expect(name).toHaveValue('My Wallet 3');
  await name.fill('Savings');
  await expect(name).toHaveAttribute('aria-invalid', 'true');
  await expect(name).toHaveClass(/duplicate-name/);
  await expect(name).toBeEnabled();
  await expect(page.getByRole('button', { name: 'Import Wallet', exact: true })).toBeDisabled();
  await name.fill('Travel');
  await expect(name).toHaveAttribute('aria-invalid', 'false');
  await expect(page.getByRole('button', { name: 'Import Wallet', exact: true })).toBeEnabled();
});

test('extension rename blocks another wallet name but accepts an unused name', async ({ page }) => {
  await render(page, 'rename');
  const name = page.getByLabel('Wallet Name', { exact: true });
  await expect(name).toHaveAttribute('aria-invalid', 'false');
  await name.fill('Spending');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeDisabled();
  await expect(name).toHaveAttribute('aria-invalid', 'true');
  await name.fill('Travel');
  await expect(page.getByRole('button', { name: 'Save', exact: true })).toBeEnabled();
});

test('wallet shortcuts copy and open the address of their own wallet', async ({
  page,
  context
}) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await render(page, 'wallet');
  await expect(page.getByRole('link', { name: 'View address on Nockblocks' })).toHaveAttribute(
    'href',
    'https://nockblocks.com/address/test-address-1'
  );
  await page.getByRole('button', { name: 'Copy address', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Copied', exact: true })).toBeVisible();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('test-address-1');
});
