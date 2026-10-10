import { test, expect } from '@playwright/test';

test.skip(!process.env.NOCKSTER_TEST_URL, 'Requires an installed desk.');

const endpoint = '/nockster/accounts';
test('accounts relay requires authentication, limits request size, and restricts destinations', async ({
  request,
  playwright
}) => {
  await request.post('/~/login', { form: { password: process.env.NOCKSTER_TEST_CODE! } });
  const headers = { 'X-Nockster-Proxy': '1', 'Content-Type': 'application/json' };
  expect((await request.post(`${endpoint}/auth/iris/challenge`, { data: '{}' })).status()).toBe(
    403
  );
  expect((await request.get(`${endpoint}/auth/iris/challenge`)).status()).toBe(405);
  for (const path of [
    '/auth/keys/other',
    '/https://example.com',
    '/auth/iris/challenge?url=https://example.com'
  ]) {
    expect((await request.post(endpoint + path, { headers, data: '{}' })).status()).toBe(404);
  }
  expect(
    (
      await request.post(`${endpoint}/auth/iris/challenge`, { headers, data: 'x'.repeat(65537) })
    ).status()
  ).toBe(413);
  const anonymous = await playwright.request.newContext({ baseURL: process.env.NOCKSTER_TEST_URL });
  try {
    expect(
      (await anonymous.post(`${endpoint}/auth/iris/challenge`, { headers, data: '{}' })).status()
    ).toBeGreaterThanOrEqual(400);
  } finally {
    await anonymous.dispose();
  }
});

test('Iris signs in a disposable wallet and obtains a working RPC key with third-party cookies blocked', async ({
  playwright
}) => {
  test.skip(
    process.env.NOCKSTER_LIVE_ACCOUNTS !== '1',
    'Creates and revokes a disposable Nockblocks API key.'
  );
  const browser = await playwright.chromium.launch({
    args: ['--test-third-party-cookie-phaseout']
  });
  const context = await browser.newContext({ baseURL: process.env.NOCKSTER_TEST_URL });
  let session: string | undefined;
  let keyId: string | undefined;
  try {
    const page = await context.newPage();
    await context.request.post('/~/login', { form: { password: process.env.NOCKSTER_TEST_CODE! } });
    await page.route('https://**', route =>
      route.request().url().startsWith('https://nockblocks.com/') ? route.continue() : route.abort()
    );
    await page.route(`**${endpoint}/auth/iris/token`, async route => {
      const response = await route.fetch();
      expect(response.status()).toBe(200);
      session = response.headers()['x-nockster-session']?.split(';', 1)[0];
      expect(session).toBeTruthy();
      expect(response.headers()['set-cookie'] || '').not.toContain(session!);
      await route.fulfill({ response });
    });
    await page.route(`**${endpoint}/auth/keys`, async route => {
      const body = route.request().postDataJSON();
      body.name = 'Nockster disposable Iris relay test';
      body.expires_at = new Date(Date.now() + 5 * 60_000).toISOString();
      const response = await route.fetch({ postData: JSON.stringify(body) });
      expect(response.status()).toBe(200);
      keyId = (await response.json()).api_key.id;
      await route.fulfill({ response });
    });
    await page.goto('/apps/nockster/');
    await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
    await page.getByLabel('Wallet Name', { exact: true }).fill('Synthetic Iris relay test');
    await page
      .getByLabel('Seed phrase', { exact: true })
      .fill(
        'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float'
      );
    await page.getByRole('button', { name: 'Import Wallet', exact: true }).click();
    const password = 'synthetic Iris relay test password';
    await page.getByLabel('Password', { exact: true }).fill(password);
    await page.getByLabel('Confirm Password', { exact: true }).fill(password);
    await page.getByRole('checkbox', { name: 'Agree to terms' }).click();
    const rpc = page.waitForResponse(
      response =>
        response.url() === 'https://nockblocks.com/rpc/v1' &&
        response.request().headers().authorization?.startsWith('Bearer ak_') === true
    );
    await page.getByRole('button', { name: 'Finish Setup', exact: true }).click();
    expect((await rpc).status()).toBe(200);
    expect(keyId).toBeTruthy();
    expect(await context.cookies('https://nockblocks.com')).toEqual([]);
    expect(await page.evaluate(() => JSON.stringify(localStorage))).not.toContain(session);
  } finally {
    await browser.close();
    if (session && keyId) {
      const revoked = await fetch(`https://nockblocks.com/auth/keys/${encodeURIComponent(keyId)}`, {
        method: 'DELETE',
        headers: { Cookie: session }
      });
      expect(revoked.ok).toBe(true);
    }
    if (session)
      await fetch('https://nockblocks.com/logout', {
        method: 'POST',
        headers: { Cookie: session }
      });
  }
});
