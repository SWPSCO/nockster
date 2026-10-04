import { test, expect, type Page } from '@playwright/test';
import { mockAccounts } from './rpcAuth.fixture';

const password = 'synthetic-wallet-password';
const mnemonic =
  'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float';
const address = '4Lu3cSW34WPwvDkTwKh7xB6yMZrvh3bFW7w26UDJVdboxADGkr2bTnL';
async function dispatch(page: Page, action: string, fields = {}) {
  return page.evaluate(async request => JSON.parse(await window.nocksterNative.dispatch(request)), {
    action,
    ...fields
  });
}
async function rpc(page: Page) {
  return page.evaluate(async moduleUrl => {
    const { fetchAuthenticatedRpc } = await import(moduleUrl);
    const response = await fetchAuthenticatedRpc(
      'https://nockblocks.com/rpc/v1',
      '{"method":"getTip"}',
      new AbortController().signal
    );
    return response.json();
  }, `/@fs${process.cwd()}/packages/wallet/src/platform/rpcAuth.ts`);
}
test.beforeEach(async ({ page }) => {
  await page.route('https://**', route => route.abort());
  await page.goto('/');
  await page.waitForFunction(() => Boolean(window.nocksterNative));
  expect(
    (await dispatch(page, 'import', { name: 'Test Wallet', password, key: mnemonic })).error
  ).toBeUndefined();
});

test('Iris V1 signs with the saved key and reuses an encrypted API key across locks and restarts', async ({
  page
}) => {
  const calls = await mockAccounts(page);
  let rejectKey = false;
  await page.route('https://nockblocks.com/rpc/v1', route => {
    const auth = route.request().headers().authorization;
    expect(auth).toMatch(/^Bearer ak_live_test_secret_[12]$/);
    if (rejectKey && auth.endsWith('_1')) return route.fulfill({ status: 401 });
    return route.fulfill({ json: { result: 'ok' } });
  });
  expect(await rpc(page)).toEqual({ result: 'ok' });
  await Promise.all([rpc(page), rpc(page)]);
  expect(calls).toEqual({ challenges: 1, logins: 1, keys: 1 });
  const storage = await page.evaluate(() => JSON.stringify(localStorage));
  expect(storage).not.toContain('ak_live_test_secret');
  expect(storage).not.toContain(mnemonic);
  expect(storage).toContain('fletch_rpc_iris_v1:');
  await dispatch(page, 'lock');
  await expect(rpc(page)).rejects.toThrow('Unlock');
  await page.reload();
  await page.waitForFunction(() => Boolean(window.nocksterNative));
  await dispatch(page, 'unlock', { password });
  expect((await dispatch(page, 'status')).state.wallets[0].address).toBe(address);
  await rpc(page);
  expect(calls.keys).toBe(1);
  rejectKey = true;
  await Promise.all([rpc(page), rpc(page), rpc(page)]);
  expect(calls).toEqual({ challenges: 2, logins: 2, keys: 2 });
});

test('a challenge cannot substitute another message or finish after lock', async ({ page }) => {
  const calls = await mockAccounts(page);
  await page.route('https://nockblocks.com/auth/iris/challenge', async route => {
    const body = route.request().postDataJSON();
    await route.fulfill({
      json: {
        challenge_id: 'x',
        nonce: 'x',
        address: body.address,
        account_type: 'v1',
        issued_at: new Date().toISOString(),
        expires_at: new Date(Date.now() + 60_000).toISOString(),
        message: 'Sign a transaction instead'
      }
    });
  });
  await expect(rpc(page)).rejects.toThrow('Invalid Iris V1 login message');
  expect(calls.keys).toBe(0);
  await page.unroute('https://nockblocks.com/auth/iris/challenge');
  let release!: () => void;
  const held = new Promise<void>(resolve => {
    release = resolve;
  });
  let reached!: () => void;
  const received = new Promise<void>(resolve => {
    reached = resolve;
  });
  await page.route('https://nockblocks.com/auth/iris/challenge', async route => {
    reached();
    await held;
    await route.abort();
  });
  const request = rpc(page).catch(error => String(error));
  await received;
  await dispatch(page, 'lock');
  release();
  expect(await request).not.toEqual({ result: 'ok' });
  expect(calls.logins).toBe(0);
  expect(calls.keys).toBe(0);
});

test('credential envelopes reject context changes, tampering, and locked access', async ({
  page
}) => {
  const result = await page.evaluate(async moduleUrl => {
    const { handleVaultMessage: vault } = await import(moduleUrl);
    const sealed = await vault({ type: 'vault:sealSecret', context: 'test-a', value: 'secret' });
    const envelope = sealed.data as string;
    const good = await vault({ type: 'vault:openSecret', context: 'test-a', envelope });
    const wrong = await vault({ type: 'vault:openSecret', context: 'test-b', envelope });
    const tampered = await vault({
      type: 'vault:openSecret',
      context: 'test-a',
      envelope: (envelope[0] === 'A' ? 'B' : 'A') + envelope.slice(1)
    });
    await window.nocksterNative.dispatch({ action: 'lock' });
    const locked = await vault({ type: 'vault:openSecret', context: 'test-a', envelope });
    return { good, wrong, tampered, locked };
  }, `/@fs${process.cwd()}/packages/wallet/src/vault/engine.ts`);
  expect(result.good.data).toBe('secret');
  expect(result.wrong.success).toBe(false);
  expect(result.tampered.success).toBe(false);
  expect(result.locked.success).toBe(false);
});

test('native transport preserves every protobuf byte and gRPC trailer without text conversion', async ({
  page
}) => {
  const result = await page.evaluate(async url => {
    const { createNativeBinaryFetch } = await import(url);
    const bytes = Uint8Array.from({ length: 256 }, (_, index) => index);
    let call: any;
    const transport = createNativeBinaryFetch(async (options: any) => {
      call = options;
      return {
        status: 200,
        headers: { 'content-type': 'application/grpc-web+proto', 'grpc-status': '0' },
        data: options.data,
        url: options.url
      };
    });
    const response = await transport(
      'https://nockblocks.com/nockchain.public.v2.NockchainService/WalletSendTransaction',
      {
        method: 'POST',
        body: bytes,
        headers: { Authorization: 'Bearer synthetic-test' }
      }
    );
    const aborted = new AbortController();
    aborted.abort();
    let rejected = false;
    try {
      await transport('https://nockblocks.com/', { signal: aborted.signal });
    } catch {
      rejected = true;
    }
    return {
      body: Array.from(new Uint8Array(await response.arrayBuffer())),
      call,
      status: response.headers.get('grpc-status'),
      rejected
    };
  }, `/@fs${process.cwd()}/packages/wallet/src/platform/nativeBinaryFetch.ts`);
  expect(result.body).toEqual(Array.from({ length: 256 }, (_, index) => index));
  expect(result.call.dataType).toBe('file');
  expect(result.call.responseType).toBe('arraybuffer');
  expect(result.call.disableRedirects).toBe(true);
  expect(result.status).toBe('0');
  expect(result.rejected).toBe(true);
});

test('submission HTTP failures become readable gRPC errors without changing successful protobufs', async ({
  page
}) => {
  const result = await page.evaluate(async url => {
    const { grpcResponse } = await import(url);
    const failed = grpcResponse(
      new Response('proxy error', { status: 502, headers: { 'Content-Type': 'text/plain' } })
    );
    const bytes = new Uint8Array(await failed.arrayBuffer());
    const good = new Response(new Uint8Array([0, 0, 0, 0, 0]), {
      headers: { 'Content-Type': 'application/grpc-web+proto' }
    });
    return {
      type: failed.headers.get('content-type'),
      flag: bytes[0],
      length: new DataView(bytes.buffer).getUint32(1),
      trailer: new TextDecoder().decode(bytes.slice(5)),
      unchanged: grpcResponse(good) === good
    };
  }, `/@fs${process.cwd()}/packages/wallet/src/platform/submission.ts`);
  expect(result.type).toBe('application/grpc-web+proto');
  expect(result.flag).toBe(128);
  expect(result.length).toBe(result.trailer.length);
  expect(result.trailer).toContain('grpc-status: 14');
  expect(decodeURIComponent(result.trailer)).toContain(
    'Transaction service unavailable (HTTP 502). Check Activity before retrying.'
  );
  expect(result.unchanged).toBe(true);
});
