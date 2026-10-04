import { test, expect } from '@playwright/test';

test('native key generation uses 256-bit Web Crypto entropy and fails closed', async ({ page }) => {
  await page.route('https://**', route => route.abort());
  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.nocksterNative));
  const result = await page.evaluate(async () => {
    const generate = async () =>
      JSON.parse(await window.nocksterNative.dispatch({ action: 'generate' }));
    await generate();
    const original = crypto.getRandomValues;
    const sizes: number[] = [];
    try {
      // BIP39's public all-zero 256-bit test vector; never saved as a wallet.
      crypto.getRandomValues = <T extends ArrayBufferView | null>(buffer: T): T => {
        if (!buffer) throw new Error('Missing entropy buffer');
        sizes.push(buffer.byteLength);
        new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength).fill(0);
        return buffer;
      };
      const generated = await generate();
      crypto.getRandomValues = () => {
        throw new Error('Test RNG unavailable');
      };
      const failed = await generate();
      return { sizes, generated, failed };
    } finally {
      crypto.getRandomValues = original;
    }
  });
  expect(result.sizes).toEqual([32]);
  expect(result.generated.mnemonic).toEqual([...Array(23).fill('abandon'), 'art']);
  expect(result.failed.error).toContain('entropy rng');
  expect(result.failed.mnemonic).toBeUndefined();
});

test('generated recovery phrases remain valid after saving, reloading, and unlocking', async ({
  page
}) => {
  await page.route('https://**', route => route.abort());
  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.nocksterNative));
  const request = (action: string, fields: Record<string, unknown> = {}) =>
    page.evaluate(async data => JSON.parse(await window.nocksterNative.dispatch(data)), {
      action,
      ...fields
    });
  await request('status');
  const phrases: string[][] = [];
  for (let attempt = 0; attempt < 3; attempt++) {
    const reply = await request('generate');
    expect(reply.error).toBeUndefined();
    expect(reply.mnemonic).toHaveLength(24);
    expect(reply.mnemonic.every((word: string) => /^[a-z]+$/.test(word))).toBe(true);
    expect(reply.state.exists).toBe(false);
    phrases.push(reply.mnemonic);
  }
  expect(new Set(phrases.map(words => words.join(' '))).size).toBe(3);
  const password = 'synthetic generation test password';
  for (const [index, words] of phrases.entries()) {
    const reply = await request('import', {
      name: `Wallet ${index}`,
      key: words.join(' '),
      password
    });
    expect(reply.error).toBeUndefined();
    expect(reply.state.wallets).toHaveLength(index + 1);
  }
  const known = await request('import', {
    name: 'Known recovery phrase',
    key: 'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float'
  });
  expect(known.error).toBeUndefined();
  expect(
    known.state.wallets.find((wallet: { name: string }) => wallet.name === 'Known recovery phrase')
      .address
  ).toBe('4Lu3cSW34WPwvDkTwKh7xB6yMZrvh3bFW7w26UDJVdboxADGkr2bTnL');
  const wallets = (await request('status')).state.wallets;
  const storedVault = await page.evaluate(() =>
    JSON.parse(localStorage.getItem('fletch_vault_v1')!)
  );
  expect(storedVault.kdf).toMatchObject({ kdfType: 'Argon2id', m: 131072, t: 3, p: 1 });
  await request('lock');
  await page.reload();
  await page.waitForFunction(() => Boolean(window.nocksterNative));
  expect((await request('unlock', { password: 'incorrect' })).error).toBeTruthy();
  const recovered = await request('unlock', { password });
  expect(recovered.error).toBeUndefined();
  expect(
    recovered.state.wallets.map((wallet: { address: string }) => wallet.address).sort()
  ).toEqual(wallets.map((wallet: { address: string }) => wallet.address).sort());
  const storage = await page.evaluate(() => JSON.stringify(localStorage));
  for (const words of phrases) expect(storage).not.toContain(words.join(' '));
  expect(storage).not.toContain(password);
});
