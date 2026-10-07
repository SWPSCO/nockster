import { test, expect, type Page } from '@playwright/test';

const request = (page: Page, action: string, fields: Record<string, unknown> = {}) =>
  page.evaluate(async data => JSON.parse(await window.nocksterNative.dispatch(data)), {
    action,
    ...fields
  });

test.beforeEach(async ({ page }) => {
  await page.route('https://**', route => route.abort());
  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.nocksterNative));
});

test('native vanity recovery imports the mined address and survives vault lock', async ({
  page
}) => {
  test.setTimeout(120_000);
  const password = 'synthetic vanity test password';
  for (const kind of ['raw', 'mnemonic']) {
    const searchId = `recover-${kind}`;
    const started = await request(page, 'vanityStart', {
      searchId,
      vanity: { prefix: '2', keyMode: kind, backend: 'cpu', maxAttempts: 10000 }
    });
    expect(started.error).toBeUndefined();
    await expect
      .poll(async () => (await request(page, 'vanityStatus', { searchId })).vanity.status, {
        timeout: 50_000
      })
      .toBe('found');
    const status = await request(page, 'vanityStatus', { searchId });
    expect(status.candidate).toBeUndefined();
    expect(JSON.stringify(status)).not.toContain('secret_key');
    const { candidate, error } = await request(page, 'vanityTake', { searchId });
    expect(error).toBeUndefined();
    expect(candidate.address).toMatch(/^2/);
    if (kind === 'raw') expect(candidate.key).toMatch(/^[0-9a-f]{64}$/);
    else expect(candidate.key.split(' ')).toHaveLength(24);
    expect((await request(page, 'vanityTake', { searchId })).error).toBeTruthy();
    const imported = await request(page, 'import', { name: kind, password, key: candidate.key });
    expect(imported.error).toBeUndefined();
    expect(
      imported.state.wallets.find((wallet: { name: string }) => wallet.name === kind).address
    ).toBe(candidate.address);
    const storage = await page.evaluate(() => JSON.stringify(localStorage));
    expect(storage).not.toContain(candidate.key);
    await request(page, 'lock');
    const unlocked = await request(page, 'unlock', { password });
    expect(
      unlocked.state.wallets.find((wallet: { name: string }) => wallet.name === kind).address
    ).toBe(candidate.address);
  }
});

test('native vanity limits, cancellation, stale controls and locking discard candidates', async ({
  page
}) => {
  const vanity = { prefix: 'zzzzzzzzzzzzzz', keyMode: 'raw', backend: 'cpu', maxAttempts: 1 };
  expect(
    (
      await request(page, 'import', {
        name: 'Invalid',
        key: '00'.repeat(32),
        password: 'synthetic vanity password'
      })
    ).error
  ).toBeTruthy();
  expect((await request(page, 'status')).state.exists).toBe(false);
  expect((await request(page, 'vanityStart', { searchId: 'limit', vanity })).error).toBeUndefined();
  await expect
    .poll(async () => (await request(page, 'vanityStatus', { searchId: 'limit' })).vanity.status)
    .toBe('exhausted');
  expect((await request(page, 'vanityTake', { searchId: 'limit' })).error).toBeTruthy();
  await request(page, 'vanityStart', { searchId: 'cancel', vanity: { ...vanity, maxAttempts: 0 } });
  await request(page, 'vanityStop', { searchId: 'limit' });
  expect((await request(page, 'vanityStatus', { searchId: 'cancel' })).vanity.status).toBe(
    'mining'
  );
  await request(page, 'vanityStop', { searchId: 'cancel' });
  expect((await request(page, 'vanityTake', { searchId: 'cancel' })).error).toBeTruthy();
  await request(page, 'vanityStart', { searchId: 'lock', vanity: { ...vanity, maxAttempts: 0 } });
  await request(page, 'lock');
  expect((await request(page, 'vanityStatus', { searchId: 'lock' })).vanity.status).toBe('idle');
  expect((await request(page, 'vanityTake', { searchId: 'lock' })).error).toBeTruthy();
});

test('mobile bridge imports a generated zprv and restores its address after reload', async ({
  page
}) => {
  const { generateExtendedKey } = await import('../vanity.fixture');
  const candidate = await generateExtendedKey(page);
  const password = 'synthetic extended-key recovery password';
  const imported = await request(page, 'import', {
    name: 'Extended',
    password,
    key: candidate.key
  });
  expect(imported.error).toBeUndefined();
  expect(imported.state.wallets[0].address).toBe(candidate.address);
  expect(await page.evaluate(() => JSON.stringify(localStorage))).not.toContain(candidate.key);
  await request(page, 'lock');
  await page.reload();
  await page.waitForFunction(() => Boolean(window.nocksterNative));
  const restored = await request(page, 'unlock', { password });
  expect(restored.error).toBeUndefined();
  expect(restored.state.wallets[0].address).toBe(candidate.address);
});

test('native search session exposes resumable public state and remains usable after cancellation', async ({
  page
}) => {
  const vanity = { prefix: 'zzzzzzzzzzzzzz', keyMode: 'raw', backend: 'cpu', maxAttempts: 0 };
  await request(page, 'vanityStart', { searchId: 'background', name: 'Mined Wallet', vanity });
  const resumed = await request(page, 'status');
  expect(resumed.state.vanitySearch).toMatchObject({
    id: 'background',
    name: 'Mined Wallet',
    options: vanity,
    progress: { status: 'mining' }
  });
  // An unrelated screen action cannot dispose the search.
  await request(page, 'generate');
  expect((await request(page, 'status')).state.vanitySearch.id).toBe('background');
  const stopped = await request(page, 'vanityStop', { searchId: 'background' });
  expect(stopped.vanity.status).toBe('stopped');
  expect(stopped.state.vanitySearch).toBeNull();
  expect((await request(page, 'generate')).mnemonic).toHaveLength(24);
  await request(page, 'vanityStart', {
    searchId: 'restart',
    name: 'Restart',
    vanity: { ...vanity, prefix: '2' }
  });
  await expect
    .poll(async () => (await request(page, 'status')).state.vanitySearch.progress.status)
    .toBe('found');
  const ready = await request(page, 'status');
  expect(ready.state.vanitySearch.progress.address).toMatch(/^2/);
  expect(JSON.stringify(ready)).not.toMatch(/secret_key|mnemonic":\[/);
  const taken = await request(page, 'vanityTake', { searchId: 'restart' });
  expect(taken.candidate.address).toMatch(/^2/);
  expect(taken.state.vanitySearch).toBeNull();
});

test('cancelling a native search does not wait for an in-flight network refresh', async ({
  page
}) => {
  const { mockAccounts } = await import('./rpcAuth.fixture');
  await mockAccounts(page, { anyPublicKey: true });
  let hold = false;
  let waiting = false;
  let release!: () => void;
  const gate = new Promise<void>(resolve => (release = resolve));
  await page.route('https://nockblocks.com/rpc**', async route => {
    const { method, id } = route.request().postDataJSON();
    if (hold && method === 'getNotes') {
      waiting = true;
      await gate;
    }
    const result =
      method === 'getNotes'
        ? { nicks: 0, notes: [] }
        : method === 'getTransactionsByAddress'
          ? { transactions: [] }
          : method === 'getAddressBook'
            ? { entries: [] }
            : method === 'getTip'
              ? { height: 1000 }
              : [];
    await route.fulfill({ json: { jsonrpc: '2.0', id, result } });
  });
  expect(
    (
      await request(page, 'import', {
        name: 'Cancel test',
        password: 'synthetic cancel test password',
        key: '0'.repeat(63) + '1'
      })
    ).error
  ).toBeUndefined();
  await request(page, 'vanityStart', {
    searchId: 'cancel-busy',
    vanity: { prefix: 'zzzzzzzzzzzzzz', keyMode: 'raw', backend: 'cpu' }
  });
  hold = true;
  await page.evaluate(() => {
    (window as any).pendingRefresh = window.nocksterNative.dispatch({ action: 'refresh' });
  });
  try {
    await expect.poll(() => waiting).toBe(true);
    const stopped = await page.evaluate(async () => {
      return (await Promise.race([
        window.nocksterNative
          .dispatch({ action: 'vanityStop', searchId: 'cancel-busy' })
          .then(JSON.parse),
        new Promise((_, reject) =>
          setTimeout(() => reject(new Error('Cancel waited for the network')), 1500)
        )
      ])) as any;
    });
    expect(stopped.vanity.status).toBe('stopped');
    expect(stopped.state.vanitySearch).toBeNull();
  } finally {
    release();
  }
  await page.evaluate(() => (window as any).pendingRefresh);
  expect((await request(page, 'generate')).mnemonic).toHaveLength(24);
});
