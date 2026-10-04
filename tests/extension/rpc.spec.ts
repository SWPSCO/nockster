import { test, expect, chromium } from '@playwright/test';
import { mkdtemp, cp, writeFile, rm, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

test('extension signs in when a software wallet is saved and submits authenticated Iris protobufs', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'nockster-extension-test-'));
  const extension = join(dir, 'extension');
  await cp(resolve('apps/extension/ext'), extension, { recursive: true });
  await writeFile(join(extension, 'test.html'), '<!doctype html><title>Nockster test</title>');
  const context = await chromium.launchPersistentContext(join(dir, 'profile'), {
    channel: 'chromium',
    headless: true,
    args: [`--disable-extensions-except=${extension}`, `--load-extension=${extension}`]
  });
  try {
    await context.route('https://**', route => route.abort());
    const worker = context.serviceWorkers()[0] || (await context.waitForEvent('serviceworker'));
    const id = new URL(worker.url()).host;
    await worker.evaluate(() => {
      const localFetch = globalThis.fetch;
      Object.assign(globalThis, { authCalls: 0, submissionCalls: 0 });
      globalThis.fetch = async (input, init) => {
        const request = new Request(input, init);
        if (request.url.startsWith('chrome-extension:')) return localFetch(input, init);
        const path = new URL(request.url).pathname;
        if (path === '/auth/iris/challenge') {
          const body = await request.json();
          const issued_at = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
          return Response.json({
            challenge_id: 'test',
            nonce: 'test',
            issued_at,
            expires_at: new Date(Date.now() + 600_000).toISOString(),
            address: body.address,
            account_type: 'v1',
            message: `Nockblocks Iris authentication\nAddress: ${body.address}\nOrigin: ${body.origin}\nChallenge: test\nIssued At: ${issued_at}`
          });
        }
        if (path === '/auth/iris/token') {
          const proof = await request.json();
          if (!proof.signature.c || !proof.public_key) throw new Error('Missing Iris proof');
          if (proof.link_account !== false || proof.link_token)
            throw new Error('Sign-in must resolve the address account');
          (globalThis as any).authCalls++;
          return Response.json({ status: 'signed_in' });
        }
        if (path === '/auth/keys')
          return Response.json({
            key: 'ak_test_extension_secret',
            api_key: { id: 'test', expires_at: new Date(Date.now() + 86400_000).toISOString() }
          });
        if (!request.headers.get('Authorization')?.startsWith('Bearer ak_test_'))
          throw new Error('Missing API key');
        if (path.endsWith('/WalletSendTransaction')) {
          const bytes = new Uint8Array(await request.arrayBuffer());
          if (bytes[0] !== 0 || new DataView(bytes.buffer).getUint32(1) !== bytes.length - 5)
            throw new Error('Invalid protobuf framing');
          (globalThis as any).submissionCalls++;
          const trailer = new TextEncoder().encode('grpc-status: 0\r\n');
          const frame = new Uint8Array(12 + trailer.length);
          frame.set([0, 0, 0, 0, 2, 10, 0, 128], 0);
          new DataView(frame.buffer).setUint32(8, trailer.length);
          frame.set(trailer, 12);
          return new Response(frame, { headers: { 'Content-Type': 'application/grpc-web+proto' } });
        }
        if (path === '/rpc/v1')
          return Response.json({ jsonrpc: '2.0', id: 1, result: { height: 1 } });
        throw new Error(`Unexpected test network path: ${path}`);
      };
    });
    const page = await context.newPage();
    const pageCount = context.pages().length;
    await page.goto(`chrome-extension://${id}/test.html`);
    const result = await page.evaluate(async () => {
      const send = (message: unknown) => chrome.runtime.sendMessage(message);
      await send({ type: 'vault:newVault', password: 'test-only-extension-password' });
      const imported = await send({
        type: 'vault:importWallet',
        nickname: 'Test',
        key: 'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float'
      });
      if (!imported.success) throw new Error(imported.error);
      await chrome.storage.local.set({
        walletState: {
          activeWallet: {
            id: 'vault-Test',
            name: 'Test',
            addresses: ['4Lu3cSW34WPwvDkTwKh7xB6yMZrvh3bFW7w26UDJVdboxADGkr2bTnL']
          }
        }
      });
      return { storage: JSON.stringify(await chrome.storage.local.get(null)) };
    });
    // No RPC call or account button initiates sign-in: selecting the saved wallet does.
    await expect.poll(() => worker.evaluate(() => (globalThis as any).authCalls)).toBe(1);
    const rpcResult = await page.evaluate(() =>
      chrome.runtime.sendMessage({
        type: 'rpcRequest',
        rpcUrl: 'https://nockblocks.com/rpc/v1',
        payload: { jsonrpc: '2.0', method: 'getTip', params: [], id: 1 }
      })
    );
    expect(rpcResult, JSON.stringify(rpcResult)).toMatchObject({ success: true });
    expect(context.pages()).toHaveLength(pageCount);
    expect(result.storage).not.toContain('ak_test_extension_secret');
    const fixture = JSON.parse(await readFile('tests/fixtures/nockster-signed.json', 'utf8'));
    // A captured, synthetic signed transaction is sent only to the in-worker mock.
    const signed = fixture.signedTx;
    expect(typeof signed).toBe('string');
    const submission = await page.evaluate(
      rawTransaction =>
        chrome.runtime.sendMessage({
          type: 'rpcRequest',
          rpcUrl: 'https://nockblocks.com/rpc',
          payload: {
            jsonrpc: '2.0',
            method: 'submitTransaction',
            params: [{ rawTransaction }],
            id: 2
          }
        }),
      signed
    );
    expect(submission, JSON.stringify(submission)).toMatchObject({ success: true });
    expect(submission.data.result).toMatch(/^[1-9A-HJ-NP-Za-km-z]{40,60}$/);
    expect(
      await worker.evaluate(() => ({
        auth: (globalThis as any).authCalls,
        submissions: (globalThis as any).submissionCalls
      }))
    ).toEqual({ auth: 1, submissions: 1 });
    await page.evaluate(() => chrome.runtime.sendMessage({ type: 'vault:lock' }));
    const locked = await page.evaluate(() =>
      chrome.runtime.sendMessage({
        type: 'rpcRequest',
        rpcUrl: 'https://nockblocks.com/rpc/v1',
        payload: { method: 'getTip', params: [] }
      })
    );
    expect(locked.success).toBe(false);
  } finally {
    await context.close();
    await rm(dir, { recursive: true, force: true });
  }
});
