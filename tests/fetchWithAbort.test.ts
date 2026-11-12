import test from 'node:test';
import assert from 'node:assert/strict';
import { fetchWithAbort } from '../packages/wallet/src/platform/fetchWithAbort.ts';

test('an ignored native abort still releases the caller, and consumes late rejection', async () => {
  const controller = new AbortController();
  let rejectTransport!: (reason: Error) => void;
  const transport: typeof fetch = () =>
    new Promise((_, reject) => {
      rejectTransport = reject;
    });
  const pending = fetchWithAbort('https://example.test', { signal: controller.signal }, transport);
  const reason = new Error('Session ended');
  controller.abort(reason);
  await assert.rejects(pending, error => error === reason);
  rejectTransport(new Error('Late native failure'));
});

test('an already aborted request never starts native work', async () => {
  const controller = new AbortController();
  controller.abort();
  let calls = 0;
  const transport: typeof fetch = async () => {
    calls++;
    return new Response();
  };
  await assert.rejects(
    fetchWithAbort(
      new Request('https://example.test', {
        signal: controller.signal
      }),
      undefined,
      transport
    ),
    { name: 'AbortError' }
  );
  assert.equal(calls, 0);
});

test('successful and failed native responses retain their normal behavior', async () => {
  const controller = new AbortController();
  const response = new Response('ok');
  assert.equal(
    await fetchWithAbort(
      'https://example.test',
      { signal: controller.signal },
      async () => response
    ),
    response
  );
  const failure = new Error('Native network failure');
  await assert.rejects(
    fetchWithAbort('https://example.test', { signal: controller.signal }, async () => {
      throw failure;
    }),
    error => error === failure
  );
  controller.abort();
});
