import { test, expect, chromium } from '@playwright/test';
import { mkdtemp, cp, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

test('packaged extension generates BIP39 phrases from Web Crypto and rejects RNG failure', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'nockster-entropy-test-'));
  const extension = join(dir, 'extension');
  await cp(resolve('apps/extension/ext'), extension, { recursive: true });
  await writeFile(join(extension, 'test.html'), '<!doctype html><title>Entropy test</title>');
  const context = await chromium.launchPersistentContext(join(dir, 'profile'), {
    channel: 'chromium',
    headless: true,
    args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`]
  });
  try {
    await context.route('https://**', route => route.abort());
    const worker = context.serviceWorkers()[0] || (await context.waitForEvent('serviceworker'));
    const page = await context.newPage();
    await page.goto(`chrome-extension://${new URL(worker.url()).host}/test.html`);
    const generate = () =>
      page.evaluate(() => chrome.runtime.sendMessage({ type: 'vault:generateKey' }));
    expect((await generate()).success).toBe(true);
    await worker.evaluate(() => {
      const sizes: number[] = [];
      Object.assign(globalThis, { entropySizes: sizes });
      // BIP39's public all-zero 256-bit test vector in this disposable worker.
      crypto.getRandomValues = <T extends ArrayBufferView | null>(buffer: T): T => {
        if (!buffer) throw new Error('Missing entropy buffer');
        sizes.push(buffer.byteLength);
        new Uint8Array(buffer.buffer, buffer.byteOffset, buffer.byteLength).fill(0);
        return buffer;
      };
    });
    const generated = await generate();
    expect(generated.success).toBe(true);
    expect(generated.data.mnemonic).toEqual([...Array(23).fill('abandon'), 'art']);
    expect(await worker.evaluate(() => (globalThis as any).entropySizes)).toEqual([32]);
    await worker.evaluate(() => {
      crypto.getRandomValues = () => {
        throw new Error('Test RNG unavailable');
      };
    });
    const failed = await generate();
    expect(failed.success).toBe(false);
    expect(failed.error).toContain('entropy rng');
    expect(failed.data).toBeUndefined();
  } finally {
    await context.close();
    await rm(dir, { recursive: true, force: true });
  }
});
