import { get } from 'svelte/store';
import { walletStore, activeWallet } from '../stores/wallet';
import { getRPCClient } from '../utils/rpc';

const POLL_INTERVAL = 10000; // 10 seconds
let checking = false;
let pollInterval: ReturnType<typeof setInterval> | null = null;

export function startPendingTransactionPoller() {
  if (pollInterval) {
    return; // Already running
  }

  pollInterval = setInterval(checkPendingTransactions, POLL_INTERVAL);

  // Also check immediately
  checkPendingTransactions();
}

export function stopPendingTransactionPoller() {
  if (pollInterval) {
    clearInterval(pollInterval);
    pollInterval = null;
  }
}

async function checkPendingTransactions() {
  if (checking) return;
  checking = true;
  try {
    await pollPendingTransactions();
  } catch {
    // Leave local reservations intact when the network or persistence is unavailable.
  } finally {
    checking = false;
  }
}

async function pollPendingTransactions() {
  const wallet = get(activeWallet);
  if (!wallet) {
    return;
  }

  const pendingTxs = wallet.pendingTransactions || [];
  if (pendingTxs.length === 0) {
    return;
  }

  await walletStore.refreshSubmissionStatus(wallet.id);
  const rpc = getRPCClient();
  const confirmedTxIds: string[] = [];

  for (const pendingTx of pendingTxs) {
    try {
      const txData = await rpc.getTransaction(pendingTx.txId);

      // If transaction is found and confirmed (has block height), mark it as confirmed
      if (txData && txData.blockHeight) {
        confirmedTxIds.push(pendingTx.txId);
      }
    } catch {
      // Transaction not found or RPC error - expected for pending txs, will retry next poll
    }
  }

  // Remove confirmed transactions from pending list and refresh wallet data
  if (confirmedTxIds.length > 0) {
    walletStore.removePendingTransactions(wallet.id, confirmedTxIds);

    // Force refresh wallet data to get the confirmed transaction
    await walletStore.fetchTransactions(wallet.id, true);
    await walletStore.fetchBalance(wallet.id, true);
    await walletStore.fetchNotes(wallet.id, true);
  }
}

// Export for manual trigger (e.g., when user pulls to refresh)
export { checkPendingTransactions };
