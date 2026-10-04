import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseNocksInput, fromNicks, toNicks } from '../packages/wallet/src/lib/utils/nicks.ts';

test('decimal parsing preserves nick precision and grouping', () => {
  assert.equal(parseNocksInput('1'), 65536n);
  assert.equal(parseNocksInput('0.0000152587890625'), 1n);
  assert.equal(parseNocksInput('0.0000076293945313'), 1n);
  assert.equal(parseNocksInput('0.0000076293945312'), 0n);
  assert.equal(parseNocksInput('100,000.5'), 6553632768n);
  assert.equal(parseNocksInput('137438953471.9999847412109375'), BigInt(Number.MAX_SAFE_INTEGER));
});
test('reject malformed and unsafe monetary values', () => {
  for (const input of [
    '',
    '1foo',
    '1e3',
    'Infinity',
    '-1',
    '1 2',
    '1,2',
    '.1',
    '137438953472',
    '1.00000000000000001'
  ]) {
    assert.equal(parseNocksInput(input), null, input);
  }
  for (const value of [NaN, Infinity, -1, 0.5, Number.MAX_SAFE_INTEGER + 1])
    assert.throws(() => toNicks(value));
  assert.throws(() => fromNicks(BigInt(Number.MAX_SAFE_INTEGER) + 1n));
});

test('nick input accepts exact safe integers and rejects fractional nicks', async () => {
  const { parseNicksInput } = await import('../packages/wallet/src/lib/utils/nicks.ts');
  assert.equal(parseNicksInput('1'), 1n);
  assert.equal(parseNicksInput('65,536'), 65536n);
  assert.equal(parseNicksInput('9007199254740991'), 9007199254740991n);
  for (const input of ['1.5', '-1', '1e3', '65,53', '9007199254740992', ''])
    assert.equal(parseNicksInput(input), null);
});
