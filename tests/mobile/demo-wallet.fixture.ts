import { readFileSync } from 'node:fs';
import type { Page } from '@playwright/test';

/** Public chain data only; authentication and address-book responses are synthetic. */
export const demoWallet = JSON.parse(
  readFileSync(new URL('../fixtures/demo-wallet-rpc.json', import.meta.url), 'utf8')
);

export async function mockDemoWallet(page: Page) {
  const rejected: string[] = [];
  // Never let an unrecorded request fall through to a real gateway.
  await page.route('https://**', route => {
    rejected.push(new URL(route.request().url()).pathname);
    return route.abort();
  });
  await page.route('https://nockblocks.com/rpc{,/v1}', route => {
    const body = route.request().postDataJSON();
    const version = new URL(route.request().url()).pathname.endsWith('/v1') ? 'v1' : 'v0';
    if (body.method === 'getAddressBook') {
      return route.fulfill({ json: { jsonrpc: '2.0', id: body.id, result: { entries: [] } } });
    }
    const record = demoWallet.records.find(
      (entry: { version: string; method: string }) =>
        entry.version === version && entry.method === body.method
    );
    if (!record || (record.params[0]?.address && body.params?.[0]?.address !== demoWallet.address)) {
      rejected.push(`${version}/${body.method}`);
      return route.fulfill({ json: {
        jsonrpc: '2.0', id: body.id,
        error: { code: -32601, message: 'Request is outside this read-only demo snapshot' }
      } });
    }
    if (body.method === 'getTransactionsByAddress') {
      const offset = body.params?.[0]?.offset ?? 0;
      const limit = body.params?.[0]?.limit ?? 20;
      return route.fulfill({ json: { ...record.response, id: body.id, result: {
        ...record.response.result,
        transactions: record.response.result.transactions.slice(offset, offset + limit)
      } } });
    }
    return route.fulfill({ json: { ...record.response, id: body.id } });
  });
  return rejected;
}
