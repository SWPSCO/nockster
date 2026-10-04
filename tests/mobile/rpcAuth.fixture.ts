import { expect, type Page } from '@playwright/test';

export async function mockAccounts(page: Page, options: { anyPublicKey?: boolean } = {}) {
  const calls = { challenges: 0, logins: 0, keys: 0 };
  let issued: Record<string, string>;
  await page.route('https://nockblocks.com/auth/**', async route => {
    const path = new URL(route.request().url()).pathname;
    const body = route.request().postDataJSON();
    if (path === '/auth/iris/challenge') {
      calls.challenges++;
      const issued_at = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
      issued = {
        challenge_id: 'test-challenge',
        nonce: 'test-nonce',
        address: body.address,
        account_type: 'v1',
        issued_at,
        expires_at: new Date(Date.now() + 600_000).toISOString(),
        message: `Nockblocks Iris authentication\nAddress: ${body.address}\nOrigin: ${body.origin}\nChallenge: test-nonce\nIssued At: ${issued_at}`
      };
      return route.fulfill({ json: issued });
    }
    if (path === '/auth/iris/token') {
      calls.logins++;
      expect(body.message).toBe(issued.message);
      expect(body.address).toBe(issued.address);
      expect(body.link_account).toBe(false);
      if (!options.anyPublicKey)
        expect(body.public_key).toBe(
          '3xJFHTfejjizuWtp4fXwBcJLtQB1oSaQP16z67kaBSoFht8su5vbooTCr6Pz9NN3hm27VppoSqRBoVweoJzbBpKaSPVUT7jNxNiMxYvK1qyGby1KQMnije8iBXAj1AMhT1Hz'
        );
      expect(body.signature.c).toMatch(/^[0-9a-f]{64}$/);
      expect(body.signature.s).toMatch(/^[0-9a-f]{64}$/);
      return route.fulfill({
        json: { status: 'signed_in' },
        headers: { 'Set-Cookie': 'test-session=1; Path=/; SameSite=None; Secure' }
      });
    }
    if (path === '/auth/keys') {
      calls.keys++;
      expect(body.networks).toEqual(['mainnet']);
      return route.fulfill({
        json: {
          key: `ak_live_test_secret_${calls.keys}`,
          api_key: { id: `key-${calls.keys}`, expires_at: body.expires_at }
        }
      });
    }
    return route.abort();
  });
  return calls;
}
