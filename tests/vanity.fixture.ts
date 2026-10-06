import type { Page } from '@playwright/test';

/** Generate fresh test-only recovery material through the shipped browser library. */
export async function generateExtendedKey(page: Page): Promise<{ key: string; address: string }> {
  return page.evaluate(async () => {
    const { mineAddress } = await import(new URL('vanity/miner.js', document.baseURI).href);
    const found = await mineAddress({ prefix: '2', backend: 'cpu', maxAttempts: 10000 });
    if (!found) throw new Error('Test address generation reached its limit.');
    try {
      const data = JSON.parse(new TextDecoder().decode(found.keyJson));
      if (typeof data.zprv !== 'string' || !data.zprv.startsWith('zprv'))
        throw new Error('The default generator must return an extended private key.');
      return { key: data.zprv, address: found.pkh };
    } finally {
      found.keyJson.fill(0);
    }
  });
}
