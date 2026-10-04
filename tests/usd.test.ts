import assert from 'node:assert/strict';
import { test } from 'node:test';
import { formatUsdEstimate } from '../packages/wallet/src/lib/utils/usd.ts';

test('USD subtitles use Nockblocks precision and grouping', () => {
  assert.equal(formatUsdEstimate(1, 0.116), '~0.116 USD');
  assert.equal(formatUsdEstimate(10, 0.116), '~1.16 USD');
  assert.equal(formatUsdEstimate(10000, 0.116), '~1,160.00 USD');
  assert.equal(formatUsdEstimate(1 / 65536, 0.116), '<0.001 USD');
  assert.equal(formatUsdEstimate(0, 0.116), '~0.000 USD');
});
test('missing prices and invalid amounts have no USD estimate', () => {
  for (const price of [undefined, null, 0, -1, NaN, Infinity]) assert.equal(formatUsdEstimate(1, price), null);
  for (const amount of [-1, NaN, Infinity]) assert.equal(formatUsdEstimate(amount, 0.116), null);
});
