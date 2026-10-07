import test from 'node:test';
import assert from 'node:assert/strict';
import { bridgeProgress } from '../packages/wallet/src/lib/utils/bridgeProgress.ts';

test('bridge block wait starts at inclusion and completes after 400 subsequent blocks', () => {
  for (const blocks of [0, 1, 200, 399, 400, 401, 1000]) {
    const progress = bridgeProgress(10_000, 10_000 + blocks);
    assert.equal(progress.blocks, Math.min(blocks, 400));
    assert.equal(progress.phase, blocks >= 400 ? 'ready' : 'confirming');
    assert.equal(progress.target, 400);
    assert.doesNotMatch(progress.label, /delivered|received/i);
  }
  assert.equal(bridgeProgress(0, 399).blocks, 399);
});

test('bridge progress handles unincluded, unknown, failed and reorganized transactions', () => {
  assert.equal(bridgeProgress(undefined, 20_000, 'pending').phase, 'inclusion');
  assert.equal(bridgeProgress(null, 20_000).blocks, 0);
  for (const height of [undefined, null, NaN, -1, 9_999, 10_000.5])
    assert.equal(bridgeProgress(10_000, height).blocks, null);
  assert.equal(bridgeProgress(10_000, 10_200, 'failed').phase, 'failed');
  assert.equal(bridgeProgress(10_100, 10_200).blocks, 100);
});
