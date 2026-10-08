import { test, expect, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import * as iris from '@nockbox/iris-wasm/iris_wasm.js';
import { desktopIO, createWallet, source, password } from './native.fixture';

const fixture = JSON.parse(
  readFileSync(new URL('../fixtures/nockster-signed.json', import.meta.url), 'utf8')
);

async function openWallet(page: Page, notes = [fixture.note]) {
  await desktopIO(page);
  await page.route('https://nockblocks.com/rpc{,/v1}', async route => {
    const { method, id } = route.request().postDataJSON();
    if (method === 'getNotesByAddress')
      return route.fulfill({ json: { jsonrpc: '2.0', id, result: notes } });
    if (method === 'getTip')
      return route.fulfill({ json: { jsonrpc: '2.0', id, result: { height: fixture.height } } });
    await route.fallback();
  });
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Create New Wallet', exact: true })).toBeVisible();
  await createWallet(page);
  await page.evaluate(async source => {
    const { router } = await import(`${source}/lib/stores/router.ts`);
    router.navigate('dashboard');
  }, source);
}

test('desktop payment scrolls at the window edge and keeps idle, loading, and quoted rows stable', async ({
  page
}) => {
  await page.setViewportSize({ width: 1440, height: 800 });
  await openWallet(page);
  await page
    .getByRole('navigation', { name: 'Main' })
    .getByRole('button', { name: 'Send', exact: true })
    .click();
  const payment = page.locator('.send-transaction');
  await expect(payment.locator('.fee-amount-nicks')).toHaveText('—');
  await expect(payment.locator('[class*="skeleton"]')).toHaveCount(0);
  const heights = () =>
    payment.evaluate(element =>
      ['.recipient-card', '.fee-section', '.summary-card'].map(
        selector => element.querySelector(selector)!.getBoundingClientRect().height
      )
    );
  const initial = await heights();
  let release!: () => void;
  const pending = new Promise<void>(resolve => {
    release = resolve;
  });
  await page.route('https://nockblocks.com/rpc{,/v1}', async route => {
    if (route.request().postDataJSON().method === 'getTip') await pending;
    await route.fallback();
  });
  await page.getByLabel('Address', { exact: true }).fill(fixture.recipients[0].address);
  await expect(payment.locator('.fee-amount-nicks')).toHaveText('—');
  await page.getByLabel('Amount', { exact: true }).fill('10');
  await expect(payment.locator('.fee-amount-nicks')).toHaveText('Calculating…');
  expect(await heights()).toEqual(initial);
  release();
  await expect(payment.locator('.fee-amount-nicks')).not.toHaveText(/Calculating|—/);
  expect(await heights()).toEqual(initial);
  await page.getByLabel('Amount', { exact: true }).fill('');
  await expect(payment.locator('.fee-amount-nicks')).toHaveText('—');
  expect(await heights()).toEqual(initial);
  const geometry = await page.evaluate(() => {
    const workspace = document.querySelector('.desktop-workspace')!;
    const content = document.querySelector('.send-transaction > .content')!;
    return {
      right: workspace.getBoundingClientRect().right,
      viewport: innerWidth,
      scrollable: workspace.scrollHeight > workspace.clientHeight,
      contentOverflow: getComputedStyle(content).overflowY,
      column: document.querySelector('.nockster-wallet')!.getBoundingClientRect().width
    };
  });
  expect(geometry).toMatchObject({
    right: 1440,
    viewport: 1440,
    scrollable: true,
    contentOverflow: 'visible',
    column: 620
  });
  await page.screenshot({ path: 'test-results/desktop-payment-layout.png', fullPage: true });
});

test('transaction file import rejects invalid nouns, reviews drafts, signs only on request, and downloads', async ({
  page
}) => {
  await openWallet(page);
  await page.getByRole('button', { name: 'Sign transaction', exact: true }).click();
  const picker = page.getByLabel('Transaction file', { exact: true });
  await expect(picker).toHaveAttribute('accept', /\.jam.*\.tx.*\.signed.*\.draft.*\.noun/);
  await picker.setInputFiles({
    name: 'empty.noun',
    mimeType: 'application/octet-stream',
    buffer: Buffer.alloc(0)
  });
  await expect(page.getByRole('alert')).toContainText('empty');
  await picker.setInputFiles({
    name: 'not-a-transaction.noun',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from([2])
  });
  await expect(page.getByRole('alert')).toContainText('not a supported transaction');
  await expect(page.getByRole('button', { name: 'Download Draft', exact: true })).toHaveCount(0);
  await picker.setInputFiles({
    name: 'payment.draft',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(fixture.draft, 'base64')
  });
  await expect(
    page.getByRole('heading', { name: 'Review transaction', exact: true })
  ).toBeVisible();
  await expect(page.getByText('Signatures needed', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Sign Transaction', exact: true })).toBeDisabled();
  await page.getByLabel('Wallet password', { exact: true }).fill(password);
  await page.getByRole('button', { name: 'Sign Transaction', exact: true }).click();
  await expect(page.getByText('Fully signed', { exact: true })).toBeVisible();
  await expect(
    page.locator('.review-output').filter({ hasText: fixture.recipients[0].address }).first()
  ).toContainText('10 NOCK');
  const downloading = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Download Transaction', exact: true }).click();
  const download = await downloading;
  expect(download.suggestedFilename()).toMatch(/\.tx$/);
  await picker.setInputFiles({
    name: 'bad.tx',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from([2])
  });
  await expect(page.getByRole('alert')).toBeVisible();
  await expect(page.getByText('Fully signed', { exact: true })).toHaveCount(0);
  await picker.setInputFiles({
    name: 'payment.signed',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(fixture.signedTx, 'base64')
  });
  await expect(page.getByText('Fully signed', { exact: true })).toBeVisible();
  await page.getByText('Combine multisig signatures', { exact: true }).click();
  const different = await page.evaluate(
    async ({ source, fixture }) => {
      const core = await import(`${source}/pkg/nockster_core.js`);
      return core.composeUnsignedTx(
        fixture.source,
        [fixture.note],
        [{ ...fixture.recipients[0], gift: 700000 }],
        fixture.height
      ).base64Tx;
    },
    { source, fixture }
  );
  await page.getByLabel('Signed copy', { exact: true }).setInputFiles({
    name: 'different.tx',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(different, 'base64')
  });
  await page.getByRole('button', { name: 'Combine Signatures', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('differs from the approved draft');
  await expect(page.getByText('Fully signed', { exact: true })).toBeVisible();
  await page.locator('.desktop-workspace').evaluate(element => {
    element.scrollTop = 0;
  });
  await page.screenshot({ path: 'test-results/desktop-transaction-review.png' });
});

test('composer selects real notes, builds and exports a draft, invalidates edits, and supports conditional outputs', async ({
  page
}) => {
  await openWallet(page);
  await page
    .getByRole('navigation', { name: 'Main' })
    .getByRole('button', { name: 'Send', exact: true })
    .click();
  await page.getByRole('tab', { name: 'Compose', exact: true }).click();
  await expect(page.locator('.note-row')).toHaveCount(1);
  await page.getByLabel('Recipient address', { exact: true }).fill(fixture.recipients[0].address);
  await page.getByLabel('Amount (NOCK)', { exact: true }).fill('10');
  await page.getByRole('button', { name: 'Build Draft', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Review & Sign', exact: true })).toBeVisible();
  await expect(page.locator('.composer .compact-draft')).toContainText('41.5 NOCK');
  const downloading = page.waitForEvent('download');
  await page
    .locator('.transaction-preview')
    .getByRole('button', { name: 'Download Draft', exact: true })
    .click();
  expect((await downloading).suggestedFilename()).toMatch(/\.psnt$/);
  await page.getByLabel('Amount (NOCK)', { exact: true }).fill('12');
  await expect(page.getByRole('button', { name: 'Review & Sign', exact: true })).toHaveCount(0);
  await page.getByText('Lock conditions & privacy', { exact: true }).click();
  await page
    .getByRole('combobox', { name: 'Spend condition', exact: true })
    .selectOption('timelock');
  await page.getByLabel('Unlock block height', { exact: true }).fill('60000');
  await page.getByRole('button', { name: 'Build Draft', exact: true }).click();
  await expect(page.getByText('From block 60000', { exact: false })).toBeVisible();
  await page.getByRole('combobox', { name: 'Amount units', exact: true }).selectOption('nicks');
  await expect(page.getByLabel('Amount (nicks)', { exact: true })).toHaveValue('786432');
  await page.getByRole('combobox', { name: 'Amount units', exact: true }).selectOption('NOCK');
  await expect(page.getByLabel('Amount (NOCK)', { exact: true })).toHaveValue('12');
  await page.getByRole('button', { name: 'Add output', exact: true }).click();
  await expect(page.locator('.output-card')).toHaveCount(2);
  await page.getByRole('button', { name: 'Remove output 2', exact: true }).click();
  await page.getByRole('tab', { name: 'Payment', exact: true }).click();
  await page.getByRole('tab', { name: 'Compose', exact: true }).click();
  await expect(page.getByLabel('Amount (NOCK)', { exact: true })).toHaveValue('12');
  await page.locator('.desktop-workspace').evaluate(element => {
    element.scrollTop = 0;
  });
  await page.screenshot({ path: 'test-results/desktop-composer.png' });
  await page.setViewportSize({ width: 800, height: 640 });
  expect(
    await page
      .locator('.desktop-workspace')
      .evaluate(element => element.scrollWidth <= element.clientWidth)
  ).toBe(true);
  await page.screenshot({ path: 'test-results/desktop-composer-800.png' });
});

test('composer picks contacts in place and previews the form in the selected units', async ({
  page
}) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await openWallet(page);
  await page.route('https://nockblocks.com/rpc{,/v1}', async route => {
    const { method, id } = route.request().postDataJSON();
    if (method === 'getAddressBook')
      return route.fulfill({
        json: {
          jsonrpc: '2.0',
          id,
          result: {
            entries: [
              {
                id: 'alice',
                alias: 'Alice',
                address: fixture.recipients[0].address,
                updatedAt: '2026-10-07'
              }
            ]
          }
        }
      });
    await route.fallback();
  });
  await page
    .getByRole('navigation', { name: 'Main' })
    .getByRole('button', { name: 'Send', exact: true })
    .click();
  const paymentHeading = await page.locator('.transaction-heading').boundingBox();
  await page.getByRole('tab', { name: 'Compose', exact: true }).click();
  const composeHeading = await page.locator('.transaction-heading').boundingBox();
  expect(composeHeading!.width).toBe(paymentHeading!.width);
  expect(composeHeading!.x).toBe(paymentHeading!.x);
  const preview = page.getByRole('region', { name: 'Transaction preview', exact: true });
  await expect(preview.locator('.input-node')).toContainText('Select input notes');
  await expect(preview.getByRole('button', { name: 'Build Draft', exact: true })).toBeVisible();
  await expect(preview.locator('.compact-draft')).toHaveCount(1);
  await expect(preview.getByText('Editing', { exact: true })).toBeVisible();
  expect(
    await page
      .locator('.nockster-wallet')
      .evaluate(element => element.getBoundingClientRect().width)
  ).toBeGreaterThan(1000);
  await page.getByRole('button', { name: 'Address book for output 1', exact: true }).click();
  await expect(page.getByRole('button', { name: /Savings My wallet/ })).toBeVisible();
  await page.getByRole('searchbox', { name: 'Search contacts', exact: true }).fill('Alice');
  await page.getByRole('button', { name: /Alice Contact/ }).click();
  await expect(page.getByLabel('Recipient address', { exact: true })).toHaveValue(
    fixture.recipients[0].address
  );
  await expect(preview.locator('[data-output-id="1"]')).toContainText('Alice');
  await page.getByLabel('Amount (NOCK)', { exact: true }).fill('10');
  await expect(preview.locator('[data-output-id="1"]')).toContainText('10 NOCK');
  await page.getByRole('button', { name: 'Build Draft', exact: true }).click();
  await expect(preview.getByText('Draft built', { exact: true })).toBeVisible();
  await page.getByRole('combobox', { name: 'Amount units', exact: true }).selectOption('nicks');
  await expect(page.locator('.note-amount')).toHaveText('6,553,600 nicks');
  await expect(preview.locator('.input-node')).toContainText('6,553,600 nicks');
  await expect(preview.locator('[data-output-id="1"]')).toContainText('655,360 nicks');
  await expect(preview.getByText('Draft built', { exact: true })).toBeVisible();
  await expect(page.getByLabel('Amount (nicks)', { exact: true })).toHaveValue('655360');
  await expect(page.getByLabel('Amount (nicks)', { exact: true })).toHaveAttribute(
    'placeholder',
    '0'
  );
  await expect(page.getByLabel('Amount (nicks)', { exact: true })).toHaveAttribute(
    'inputmode',
    'numeric'
  );
  await page.getByText('Add an input note manually', { exact: true }).click();
  await expect(page.getByLabel('Note amount (nicks)', { exact: true })).toHaveAttribute(
    'placeholder',
    '0'
  );
  await page.getByText('Add an input note manually', { exact: true }).click();
  await page.getByRole('button', { name: 'Add output', exact: true }).click();
  await expect(preview.locator('[data-output-id]')).toHaveCount(2);
  await expect(preview.getByText('Editing', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Remove output 2', exact: true }).click();
  await page.locator('.note-row input[type="checkbox"]').uncheck();
  await expect(preview.getByText('Select input notes', { exact: true })).toBeVisible();
  await page.locator('.note-row input[type="checkbox"]').check();
  await page.getByRole('combobox', { name: 'Amount units', exact: true }).selectOption('NOCK');
  await expect(page.locator('.note-amount')).toHaveText('100 NOCK');
  await page.locator('.desktop-workspace').evaluate(element => {
    element.scrollTop = 0;
  });
  await page.screenshot({ path: 'test-results/desktop-composer-preview-1440.png' });
  for (const width of [1120, 800]) {
    await page.setViewportSize({ width, height: 800 });
    expect(
      await page
        .locator('.desktop-workspace')
        .evaluate(element => element.scrollWidth <= element.clientWidth)
    ).toBe(true);
    await page.screenshot({ path: `test-results/desktop-composer-preview-${width}.png` });
  }
  await page.getByRole('tab', { name: 'Payment', exact: true }).click();
  expect(
    await page
      .locator('.nockster-wallet')
      .evaluate(element => element.getBoundingClientRect().width)
  ).toBeLessThanOrEqual(620);
});

test('thousands of notes stay virtualized, choose fewest inputs including fees, and keep preview below the form', async ({
  page
}) => {
  iris.initSync({
    module: readFileSync(
      new URL('../../node_modules/@nockbox/iris-wasm/iris_wasm_bg.wasm', import.meta.url)
    )
  });
  const notes = Array.from({ length: 5000 }, (_, index) => ({
    ...fixture.note,
    lastName: iris.hashU64(BigInt(index + 1000)),
    assets: (index + 1) * 65536
  }));
  await page.setViewportSize({ width: 1440, height: 1000 });
  await openWallet(page, notes);
  await page.evaluate(
    async ({ source, reserved }) => {
      const { walletStore } = await import(`${source}/lib/stores/wallet.ts`);
      let wallet;
      const unsubscribe = walletStore.subscribe(state => {
        wallet = state.wallets[0];
      });
      unsubscribe();
      wallet.pendingTransactions = [{ txId: 'reserved-test', inputNotes: [reserved] }];
      walletStore.updateBalance(wallet.id, wallet.balance);
    },
    { source, reserved: `${notes[4999].firstName}/${notes[4999].lastName}` }
  );
  await page
    .getByRole('navigation', { name: 'Main' })
    .getByRole('button', { name: 'Send', exact: true })
    .click();
  await page.getByRole('tab', { name: 'Compose', exact: true }).click();
  await expect(page.locator('.selection-count')).toContainText('0 of 5,000 selected');
  await expect(page.locator('.note-row').first()).toContainText('5,000 NOCK');
  await expect(page.locator('.note-row input').first()).toBeDisabled();
  expect(await page.locator('.note-row').count()).toBeLessThanOrEqual(14);
  const list = page.getByRole('region', { name: 'Available input notes', exact: true });
  await list.evaluate(element => {
    element.scrollTop = element.scrollHeight;
  });
  await expect(page.locator('.note-row').last().locator('.note-amount')).toHaveText('1 NOCK');
  expect(await page.locator('.note-row').count()).toBeLessThanOrEqual(14);
  await page
    .getByRole('searchbox', { name: 'Search input notes', exact: true })
    .fill(notes[4321].lastName);
  await expect(page.locator('.note-row')).toHaveCount(1);
  await expect(page.locator('.note-amount')).toHaveText('4,322 NOCK');
  await page.getByRole('searchbox', { name: 'Search input notes', exact: true }).fill('');
  await page.getByLabel('Recipient address', { exact: true }).fill(fixture.recipients[0].address);
  await page.getByLabel('Amount (NOCK)', { exact: true }).fill('10');
  await expect(page.getByRole('button', { name: 'Build Draft', exact: true })).toBeEnabled();
  await expect(page.locator('.selection-count')).toContainText('1 of 5,000 selected');
  await expect(page.locator('.note-row input').nth(1)).toBeChecked();
  await page.getByLabel('Amount (NOCK)', { exact: true }).fill('4999');
  await expect(page.getByRole('button', { name: 'Build Draft', exact: true })).toBeEnabled();
  await expect(page.locator('.selection-count')).toContainText('2 of 5,000 selected');
  await page.getByLabel('Amount (NOCK)', { exact: true }).fill('10');
  await expect(page.locator('.selection-count')).toContainText('1 of 5,000 selected');
  await page.getByLabel('Amount (NOCK)', { exact: true }).fill('');
  await expect(page.locator('.selection-count')).toContainText('0 of 5,000 selected');
  await expect(page.getByRole('button', { name: 'Build Draft', exact: true })).toBeDisabled();
  for (let index = 1; index <= 8; index++) await page.locator('.note-row input').nth(index).check();
  await expect(page.getByRole('combobox', { name: 'Input selection', exact: true })).toHaveValue(
    'manual'
  );
  await expect(page.locator('.selection-count')).toContainText('8 of 5,000 selected');
  const preview = page.getByRole('region', { name: 'Transaction preview', exact: true });
  await expect(preview.locator('.input-node')).toHaveCount(5);
  await expect(preview.locator('.grouped-notes')).toContainText('4 more notes');
  await page.getByRole('checkbox', { name: 'Selected only', exact: true }).check();
  await expect(page.locator('.note-row')).toHaveCount(8);
  expect(
    await page.evaluate(
      () =>
        document.querySelector('.transaction-preview')!.getBoundingClientRect().top >=
        document.querySelector('.composer-form')!.getBoundingClientRect().bottom
    )
  ).toBe(true);
  await page.locator('.desktop-workspace').evaluate(element => {
    element.scrollTop = 0;
  });
  await page.screenshot({ path: 'test-results/desktop-composer-many-notes.png' });
  await preview.scrollIntoViewIfNeeded();
  await page.screenshot({ path: 'test-results/desktop-composer-grouped-preview.png' });
  await page.getByRole('button', { name: 'Clear selection', exact: true }).click();
  await expect(page.locator('.selection-count')).toContainText('0 of 5,000 selected');
});
