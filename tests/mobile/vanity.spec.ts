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
