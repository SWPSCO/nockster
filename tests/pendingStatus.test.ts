import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  pendingStatus,
  pendingWithoutConfirmed,
  PENDING_FAILURE_DELAY,
  MEMPOOL_OBSERVATION_TTL
} from '../packages/wallet/src/lib/utils/pendingStatus.ts';
import type { PendingTransaction } from '../packages/wallet/src/lib/types/wallet';

const tx: PendingTransaction = {
  txId: 'pending',
  recipients: [],
  totalAmount: 1,
  fee: 1,
  timestamp: 1_000_000,
  fromAddress: 'test',
  submissionStatus: 'acknowledged'
};
test('delayed submissions are uncertain, while recent mempool sightings take precedence', () => {
  const now = tx.timestamp + PENDING_FAILURE_DELAY;
  assert.equal(pendingStatus(tx, now - 1).label, 'Submitted');
  assert.equal(pendingStatus(tx, now).label, 'Failed?');
  assert.equal(
    pendingStatus({ ...tx, mempoolPresent: true, mempoolCheckedAt: now }, now).label,
    'In mempool'
  );
  assert.equal(
    pendingStatus(
      { ...tx, mempoolPresent: true, mempoolCheckedAt: now - MEMPOOL_OBSERVATION_TTL - 1 },
      now
    ).label,
    'Failed?'
  );
  assert.equal(
    pendingStatus({ ...tx, mempoolPresent: false, mempoolCheckedAt: now }, now).label,
    'Failed?'
  );
  assert.equal(pendingStatus({ ...tx, lastSubmittedAt: now }, now).label, 'Submitted');
});
test('confirmed history wins by transaction ID without dropping unrelated pending transactions', () => {
  const other = { ...tx, txId: 'other' };
  assert.deepEqual(pendingWithoutConfirmed([tx, other], [{ txId: tx.txId, status: 'confirmed' }]), [
    other
  ]);
  assert.deepEqual(pendingWithoutConfirmed([tx], [{ txId: tx.txId, blockHeight: 153881 }]), []);
  assert.deepEqual(pendingWithoutConfirmed([tx], [{ txId: tx.txId, status: 'pending' }]), [tx]);
});

test('explicit non-acceptance stays distinct from delayed uncertainty', () => {
  const rejected = { ...tx, submissionStatus: 'rejected' };
  assert.equal(pendingStatus(rejected, tx.timestamp).label, 'Not accepted');
  assert.equal(pendingStatus(rejected, tx.timestamp + PENDING_FAILURE_DELAY).label, 'Not accepted');
});
