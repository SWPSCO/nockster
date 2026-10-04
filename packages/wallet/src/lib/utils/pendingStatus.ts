import type { PendingTransaction } from '../types/wallet';
import { submissionStatusLabel } from './submissionStatus.ts';

export const PENDING_FAILURE_DELAY = 30 * 60_000;
export const MEMPOOL_OBSERVATION_TTL = 3 * 60_000;
export const CLEAR_PENDING_WARNING =
  'Clear this unconfirmed transaction from this wallet? This releases its reserved funds locally. It does not cancel the transaction on the network; it could still confirm. Check the explorer before sending again.';

export function pendingStatus(tx: PendingTransaction, now = Date.now()) {
  const inMempool =
    tx.mempoolPresent === true &&
    tx.mempoolCheckedAt !== undefined &&
    now - tx.mempoolCheckedAt <= MEMPOOL_OBSERVATION_TTL;
  const delayed = now - (tx.lastSubmittedAt ?? tx.timestamp) >= PENDING_FAILURE_DELAY;
  return {
    label: inMempool
      ? 'In mempool'
      : tx.submissionStatus === 'rejected'
        ? 'Not accepted'
        : delayed
          ? 'Failed?'
          : submissionStatusLabel(tx.submissionStatus),
    detail: inMempool
      ? 'Seen in the indexed mempool. Awaiting confirmation.'
      : tx.submissionStatus === 'rejected'
        ? 'The node did not accept this transaction. You can clear it locally and review a new transaction.'
        : delayed
          ? 'Still unconfirmed. It may have failed, but missing from the mempool does not prove rejection.'
          : 'Awaiting confirmation. Submission does not guarantee acceptance.',
    inMempool
  };
}

export function pendingWithoutConfirmed(
  pending: PendingTransaction[] = [],
  transactions: Array<{ txId: string; status?: string; blockHeight?: number }> = []
): PendingTransaction[] {
  const confirmed = new Set(
    transactions
      .filter(tx => tx.status === 'confirmed' || (tx.blockHeight ?? 0) > 0)
      .map(tx => tx.txId)
  );
  return pending.filter(tx => !confirmed.has(tx.txId));
}
