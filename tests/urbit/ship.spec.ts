import { test, expect } from '@playwright/test';
import { readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';

test.skip(!process.env.NOCKSTER_TEST_URL, 'Requires an installed desk on a disposable ship.');

test('Clay serves every bundled asset and backup writes require authentication and a matching revision', async ({
  request,
  playwright
}) => {
  await request.post('/~/login', { form: { password: process.env.NOCKSTER_TEST_CODE! } });
  async function verify(directory: string, relative = '') {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const file = join(directory, entry.name);
      const path = join(relative, entry.name);
      if (entry.isDirectory()) await verify(file, path);
      else {
        const response = await request.get(`/apps/nockster/${path}`);
        expect(response.status(), path).toBe(200);
        const expected = await readFile(file);
        if (path.endsWith('.txt')) {
          // Clay's text mark represents lines without a terminal newline.
          expect(await response.text(), path).toBe(expected.toString().replace(/\n$/, ''));
        } else {
          expect(await response.body(), path).toEqual(expected);
        }
        if (path.endsWith('.wasm'))
          expect(response.headers()['content-type']).toBe('application/wasm');
      }
    }
  }
  await verify('apps/urbit/dist');
  const before = await request.get('/nockster/backup');
  const revision = before.headers().etag;
  const headers = {
    'X-Nockster-Backup': '1',
    'If-Match': revision,
    'Content-Type': 'application/octet-stream'
  };
  // Opaque synthetic bytes test the server independently of client encryption.
  const ciphertext = 'synthetic encrypted backup transport fixture';
  expect((await request.put('/nockster/backup', { data: ciphertext, headers })).status()).toBe(204);
  const saved = await request.get('/nockster/backup');
  expect(await saved.text()).toBe(ciphertext);
  expect(saved.headers().etag).not.toBe(revision);
  expect((await request.put('/nockster/backup', { data: 'stale', headers })).status()).toBe(409);
  expect((await request.put('/nockster/backup', { data: 'missing header' })).status()).toBe(403);
  expect(
    (
      await request.put('/nockster/backup', {
        data: 'x'.repeat(2 * 1024 * 1024 + 1),
        headers: { ...headers, 'If-Match': saved.headers().etag }
      })
    ).status()
  ).toBe(413);
  expect((await request.post('/nockster/backup')).status()).toBe(405);
  const anonymous = await playwright.request.newContext({ baseURL: process.env.NOCKSTER_TEST_URL });
  try {
    const denied = await anonymous.get('/nockster/backup');
    expect(denied.status()).toBeGreaterThanOrEqual(400);
    expect(await denied.text()).not.toContain(ciphertext);
    expect(
      (await anonymous.put('/nockster/backup', { data: 'unauthorized', headers })).status()
    ).toBeGreaterThanOrEqual(400);
  } finally {
    await anonymous.dispose();
  }
  expect(await (await request.get('/nockster/backup')).text()).toBe(ciphertext);
});
