import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  expectedVanityAttempts,
  formatVanityDuration
} from '../packages/wallet/src/lib/utils/vanityEstimate.ts';

const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
const space = 0xffffffff00000001n ** 5n;
const probability = (prefix: string, insensitive = false) => {
  const trials = expectedVanityAttempts(prefix, insensitive);
  return trials === null ? 0 : 1 / trials;
};
const close = (a: number, b: number) =>
  assert.ok(Math.abs(a - b) <= Math.max(a, b) * 1e-12, `${a} != ${b}`);

test('canonical address prefixes partition the whole PKH space', () => {
  close(
    [...alphabet].reduce((sum, prefix) => sum + probability(prefix), 0),
    1
  );
  assert.equal(expectedVanityAttempts('1'), Number(space)); // Only zero encodes as 1.
  assert.equal(expectedVanityAttempts('11'), null); // No padded leading zero digits.
  assert.ok(probability('2') > probability('z'));
  // Extending a prefix partitions its matches except the prefix itself.
  close(
    [...alphabet].reduce((sum, digit) => sum + probability(`n${digit}`), 0),
    probability('n') - 1 / Number(space)
  );
});

test('full-length prefixes respect the PKH upper bound', () => {
  let value = space - 1n;
  let maximum = '';
  do {
    maximum = alphabet[Number(value % 58n)] + maximum;
    value /= 58n;
  } while (value);
  assert.equal(expectedVanityAttempts(maximum), Number(space));
  assert.equal(expectedVanityAttempts('z'.repeat(55)), null);
  assert.equal(expectedVanityAttempts('2'.repeat(56)), null);
  assert.equal(expectedVanityAttempts(''), null);
  assert.equal(expectedVanityAttempts('O'), null);
});

test('case and digit alternatives affect the expected rate without merging i and l', () => {
  close(probability('a', true), probability('a') + probability('A') + probability('4'));
  close(probability('i', true), probability('i') + probability('1'));
  close(probability('l', true), probability('L') + probability('1'));
  close(probability('1', true), probability('i') + probability('L') + probability('1'));
  close(probability('O', true), probability('o'));
  assert.equal(expectedVanityAttempts(' nock '), expectedVanityAttempts('nock'));
  assert.ok(expectedVanityAttempts('nock', true)! < expectedVanityAttempts('nock')!);
});

test('time estimates scale with measured speed and format short and very long searches', () => {
  const attempts = expectedVanityAttempts('nock')!;
  close(attempts / 2000, attempts / 1000 / 2);
  assert.equal(formatVanityDuration(0.2), 'less than a second');
  assert.equal(formatVanityDuration(90), 'about 1.5 minutes');
  assert.equal(formatVanityDuration(7200), 'about 2 hours');
  assert.equal(formatVanityDuration(86400), 'about 1 day');
  assert.match(formatVanityDuration(1e20)!, /years$/);
  assert.equal(formatVanityDuration(Infinity), null);
  assert.equal(formatVanityDuration(NaN), null);
});
