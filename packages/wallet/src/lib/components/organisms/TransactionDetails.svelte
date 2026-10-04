<script lang="ts">
  import { onMount } from 'svelte';
  import { startPendingTransactionPoller, stopPendingTransactionPoller } from '../../services/pendingTransactionPoller';
  onMount(() => { startPendingTransactionPoller(); return stopPendingTransactionPoller; });
  import { BRIDGE_CONFIRMATION_BLOCKS } from '../../utils/bridge';
  import { formatUsdEstimate } from '../../utils/usd';
  import { pendingStatus, CLEAR_PENDING_WARNING } from '../../utils/pendingStatus';
  import PaymentPassword from '../molecules/PaymentPassword.svelte';
  let confirmingPayment = false;
  import Header from '../molecules/Header.svelte';
  import { nockPrice } from '../../stores/price';
  import { activeWallet, walletStore } from '../../stores/wallet';
  import type { Transaction } from '../../types/transaction';
  import { formatNocksWithSeparator, nicksToNocks, formatFeeInNicks } from '../../utils/nicks';
  import { inspectTxJam, toWalletTxJam } from '../../../vaultApi';
  import type { JamInspectResult } from '../../../vault/types';

  export let transaction: Transaction;
  export let onBack: () => void = () => {};

  let copiedField: string | null = null;
  let isResubmitting = false;
  let pendingActionError: string | null = null;

  // Get current wallet address for determining transaction type
  $: walletAddress = $activeWallet?.addresses?.[0] || '';

  // Derive values from transaction data
  // Use the type directly from the API when available, fallback to address comparison
  $: type = (() => {
    // Use API type directly when available
    if (transaction.type === 'received') return 'received';
    if (transaction.type === 'sent') return 'sent';
    if (transaction.type === 'self') return 'consolidation';

    // Fallback to address comparison for legacy data
    if (transaction.to === walletAddress) {
      return 'received';
    } else if (transaction.from === walletAddress) {
      return 'sent';
    } else if (transaction.from === transaction.to) {
      return 'consolidation';
    }
    // Default fallback
    return 'sent';
  })();

  // Convert amounts from nicks (API format) to NOCK (display format)
  $: amountNicks = BigInt(Math.trunc(transaction.amount || 0));
  $: amount = nicksToNocks(amountNicks);
  $: feeInNicks = BigInt(Math.trunc(transaction.fee || 0));
  $: fee = nicksToNocks(feeInNicks);
  $: totalNicks = amountNicks + feeInNicks;
  $: from = transaction.from || 'Unknown';
  $: to = transaction.to || 'Unknown';
  $: txHash = transaction.id || (transaction as any).txId || '';

  $: blockHeight = confirmedRecord?.blockHeight || transaction.blockHeight || 0;
  $: confirmedRecord = $activeWallet?.transactions?.find(tx => tx.txId === txHash && tx.status === 'confirmed');
  $: status = (confirmedRecord?.status ?? transaction.status) as 'pending' | 'confirmed' | 'failed';

  $: pendingRecord = $activeWallet?.pendingTransactions?.find(tx => tx.txId === txHash);
  $: pendingSignedTx = pendingRecord?.signedTx;
  $: pendingSubmitAttempts = pendingRecord?.submitAttempts || 0;
  $: pendingLastSubmitError = pendingRecord?.lastSubmitError;

  // Format timestamp
  $: timestamp = formatTimestamp(transaction.timestamp);

  function formatTimestamp(unixTimestamp: number): string {
    // API returns Unix timestamp in seconds, convert to milliseconds
    // If timestamp looks like it's already in milliseconds (> year 2000), use as-is
    const timestampMs = unixTimestamp > 946684800000 ? unixTimestamp : unixTimestamp * 1000;
    const date = new Date(timestampMs);

    // Check if date is valid
    if (isNaN(date.getTime())) {
      return 'Invalid date';
    }

    return date.toLocaleString('en-US', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false
    });
  }

  function formatUSD(nockAmount: number): string {
    return formatUsdEstimate(nockAmount, $nockPrice) ?? '';
  }

  function truncateAddress(address: string): string {
    if (address.length <= 16) return address;
    return `${address.slice(0, 8)}...${address.slice(-8)}`;
  }

  function copyToClipboard(text: string, field: string) {
    navigator.clipboard.writeText(text).then(() => {
      copiedField = field;
      setTimeout(() => {
        copiedField = null;
      }, 2000);
    });
  }

  function decodeBase64(base64: string): Uint8Array {
    const cleaned = base64
      .trim()
      .replace(/ /g, '+')
      .replace(/\s+/g, '')
      .replace(/-/g, '+')
      .replace(/_/g, '/');
    const remainder = cleaned.length % 4;
    if (remainder === 1) {
      throw new Error('Invalid base64 payload length.');
    }
    const padded = remainder === 0 ? cleaned : remainder === 2 ? `${cleaned}==` : `${cleaned}=`;
    const binary = atob(padded);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  }

  function isFullySigned(result: JamInspectResult | null): boolean {
    if (!result?.spends?.length) return false;
    return !result.spends.some(spend => spend.lock?.pkh && !spend.isFullySigned);
  }

  async function savePendingTransactionFile() {
    if (!pendingSignedTx) {
      pendingActionError = 'Missing wallet transaction payload for this pending entry.';
      return;
    }

    pendingActionError = null;
    try {
      const walletTxResult = await toWalletTxJam(pendingSignedTx);
      if (!walletTxResult.success || !walletTxResult.data) {
        throw new Error(walletTxResult.error || 'Failed to prepare wallet transaction file.');
      }

      const inspectResult = await inspectTxJam(walletTxResult.data);
      if (!inspectResult.success || !inspectResult.data) {
        throw new Error(inspectResult.error || 'Failed to inspect wallet transaction payload.');
      }

      const bytes = decodeBase64(walletTxResult.data);
      const blob = new Blob([new Uint8Array(bytes)], { type: 'application/octet-stream' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      const ext = isFullySigned(inspectResult.data) ? 'tx' : 'psnt';
      a.download = `${txHash || inspectResult.data.txId || 'transaction'}.${ext}`;
      a.style.display = 'none';
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      window.setTimeout(() => URL.revokeObjectURL(url), 10_000);
    } catch (error) {
      pendingActionError = error instanceof Error ? error.message : String(error);
    }
  }

  async function handleResubmit() {
    if (!$activeWallet) return;
    if (!txHash) return;
    if (!pendingSignedTx) {
      pendingActionError = 'Missing wallet transaction payload for this pending entry.';
      return;
    }

    pendingActionError = null;
    isResubmitting = true;
    try {
      await walletStore.resubmitPendingTransaction($activeWallet.id, txHash);
    } catch (error) {
      pendingActionError = error instanceof Error ? error.message : String(error);
    } finally {
      isResubmitting = false;
    }
  }

  function handleDeletePending() {
    if (!$activeWallet) return;
    if (!txHash) return;
    pendingActionError = null;
    const confirmed = confirm(
      CLEAR_PENDING_WARNING
    );
    if (!confirmed) return;
    walletStore.deletePendingTransaction($activeWallet.id, txHash);
    onBack();
  }

  function openExplorer() {
    if (txHash) {
      window.open(`https://nockblocks.com/tx/${txHash}`, '_blank');
    }
  }

  function openAddressExplorer(address: string) {
    if (address && address !== 'Unknown') {
      window.open(`https://nockblocks.com/address/${address}`, '_blank');
    }
  }

  function openTxExplorer() {
    if (txHash) {
      window.open(`https://nockblocks.com/tx/${txHash}`, '_blank');
    }
  }
</script>

<div class="transaction-details fixed-screen">
  <Header title="Transaction Details" showBack={true} on:click={onBack} />

  <div class="content">
    <!-- Status Section -->
    <div class="status-section">
      <div class="transaction-type">
        {transaction.bridge ? 'Bridge to Base' : type === 'sent' ? 'Sent' : type === 'received' ? 'Received' : 'Consolidation'}
      </div>
    </div>

    <!-- Amount Section -->
    <div class="amount-section">
      <div class="amount-value">
        {type === 'sent' ? '-' : type === 'received' ? '+' : ''}{formatNocksWithSeparator(
          amountNicks,
          8
        )} NOCK
      </div>
      <div class="amount-usd">{formatUSD(amount)}</div>
    </div>

    {#if transaction.bridge}
      <div class="detail-card">
        <p>Base destination</p>
        <a href={`https://basescan.org/address/${transaction.bridge.destination}`} target="_blank" rel="noopener noreferrer" style="overflow-wrap:anywhere">{transaction.bridge.destination}</a>
        <p>Bridge protocol fee (≈0.3%): {formatNocksWithSeparator(BigInt(transaction.bridge.protocolFee), 8)} NOCK</p>
        <p>Expected on Base: {formatNocksWithSeparator(BigInt(transaction.bridge.expectedReceived), 8)} NOCK</p>
        <p>L1 confirmation does not mean delivery on Base. The bridge waits {BRIDGE_CONFIRMATION_BLOCKS} blocks before processing.</p>
      </div>
    {/if}

    <!-- Details Cards -->
    <div class="details-cards">
      <!-- From/To Card -->
      <div class="detail-card">
        <div class="detail-row">
          <span class="detail-label">From</span>
          <div class="detail-value-group">
            <button
              class="address-link"
              on:click={() => openAddressExplorer(from)}
              title="View address on Nockblocks"
            >
              <span class="detail-value mono">{truncateAddress(from)}</span>
            </button>
            <button
              class="copy-btn"
              on:click={() => copyToClipboard(from, 'from')}
              title="Copy full address"
            >
              {copiedField === 'from' ? '✓' : '📋'}
            </button>
          </div>
        </div>

        <div class="detail-row">
          <span class="detail-label">To</span>
          <div class="detail-value-group">
            <button
              class="address-link"
              on:click={() => openAddressExplorer(to)}
              title="View address on Nockblocks"
            >
              <span class="detail-value mono">{truncateAddress(to)}</span>
            </button>
            <button
              class="copy-btn"
              on:click={() => copyToClipboard(to, 'to')}
              title="Copy full address"
            >
              {copiedField === 'to' ? '✓' : '📋'}
            </button>
          </div>
        </div>
      </div>

      <!-- Transaction Info Card -->
      <div class="detail-card">
        <div class="detail-row">
          <span class="detail-label">Transaction Hash</span>
          <div class="detail-value-group">
            <button
              class="address-link"
              on:click={openTxExplorer}
              title="View transaction on Nockblocks"
            >
              <span class="detail-value mono small">{truncateAddress(txHash)}</span>
            </button>
            <button
              class="copy-btn"
              on:click={() => copyToClipboard(txHash, 'hash')}
              title="Copy transaction hash"
            >
              {copiedField === 'hash' ? '✓' : '📋'}
            </button>
          </div>
        </div>

        <div class="detail-row">
          <span class="detail-label">Timestamp</span>
          <span class="detail-value">{timestamp}</span>
        </div>

        <div class="detail-row">
          <span class="detail-label">Block Height</span>
          <span class="detail-value">{blockHeight.toLocaleString()}</span>
        </div>
      </div>

      <!-- Fee Card -->
      <div class="detail-card">
        <div class="detail-row">
          <span class="detail-label">Network Fee</span>
          <div class="fee-values">
            {#if feeInNicks < 100n}
              <div class="fee-amount-group">
                <span class="detail-value">{formatFeeInNicks(feeInNicks)}</span>
                <span class="detail-value-secondary">
                  ({formatNocksWithSeparator(feeInNicks, 8)} NOCK)
                </span>
              </div>
            {:else}
              <div class="fee-amount-group">
                <span class="detail-value">{formatNocksWithSeparator(feeInNicks, 8)} NOCK</span>
                <span class="detail-value-usd">{formatUSD(fee)}</span>
              </div>
            {/if}
          </div>
        </div>

        <div class="detail-row">
          <span class="detail-label">Total</span>
          <div class="fee-values">
            <span class="detail-value bold">{formatNocksWithSeparator(totalNicks, 8)} NOCK</span>
            <span class="detail-value-usd">{formatUSD(amount + fee)}</span>
          </div>
        </div>
      </div>
    </div>

    {#if status === 'pending' && pendingRecord}
      <div class="pending-actions">
        <div class="pending-actions-header">
          <div class="pending-actions-title">{pendingStatus(pendingRecord).label}</div>
          <p>{pendingStatus(pendingRecord).detail}</p>
          <div class="pending-actions-subtitle">
            Submission: {pendingStatus(pendingRecord).label} • Attempts: {pendingSubmitAttempts}{#if pendingLastSubmitError}
              <span class="pending-error">• {pendingLastSubmitError}</span>
            {/if}
          </div>
        </div>

        {#if pendingActionError}
          <div class="pending-error-banner">{pendingActionError}</div>
        {/if}

        <div class="pending-actions-row">
          <button
            class="btn-secondary"
            type="button"
            on:click={async () => {
              try {
                if ($activeWallet) await walletStore.refreshSubmissionStatus($activeWallet.id);
              } catch (error) {
                pendingActionError = String(error);
              }
            }}>Check Submission Status</button
          >
          <button
            class="btn-secondary"
            type="button"
            disabled={!pendingSignedTx}
            on:click={savePendingTransactionFile}
          >
            Save file
          </button>
          <button
            class="btn-primary"
            type="button"
            disabled={isResubmitting}
            on:click={() => { confirmingPayment = true; }}
          >
            {isResubmitting ? 'Resubmitting…' : 'Resubmit'}
          </button>
        </div>

        <button class="btn-secondary danger" type="button" on:click={handleDeletePending}>
          Clear unconfirmed transaction
        </button>
      </div>
    {/if}

    <!-- Explorer Link -->
    <button class="explorer-link" on:click={openExplorer}>
      <span>View on NockBlocks</span>
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
        <path
          d="M6 3L11 8L6 13"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    </button>
  </div>
</div>

{#if confirmingPayment}
  <PaymentPassword onCancel={() => { confirmingPayment = false; }} onConfirm={async () => { confirmingPayment = false; await handleResubmit(); }} />
{/if}

<style>
  .transaction-details {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-background);
  }

  .content {
    flex: 1;
    padding: 20px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .status-section {
    display: flex;
    justify-content: flex-start;
    align-items: center;
  }

  .transaction-type {
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text);
  }

  .amount-section {
    text-align: center;
    padding: 24px 0;
    border-bottom: 1px solid var(--color-surface) 790;
  }

  .amount-value {
    font-size: 32px;
    font-weight: 600;
    color: var(--color-text);
    letter-spacing: -0.5px;
    margin-bottom: 4px;
    font-variant-numeric: tabular-nums;
  }

  .amount-usd {
    font-size: 18px;
    color: var(--color-text-secondary);
    font-weight: 400;
  }

  .details-cards {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .detail-card {
    background: var(--color-surface);
    border: 1px solid var(--color-border) 1269;
    border-radius: 10px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .detail-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .detail-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .detail-value-group {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .detail-value {
    font-size: 14px;
    color: var(--color-text);
    font-weight: 500;
  }

  .detail-value.mono {
    font-family: monospace;
    font-size: 13px;
  }

  .detail-value.small {
    font-size: 12px;
  }

  .detail-value.bold {
    font-weight: 600;
  }

  .detail-value-usd {
    font-size: 12px;
    color: var(--color-text-secondary);
    font-weight: 400;
  }

  .fee-values {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
  }

  .fee-amount-group {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
  }

  .detail-value-secondary {
    font-size: 12px;
    color: var(--color-text-secondary);
    font-weight: 400;
  }

  .address-link {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    text-decoration: underline;
    text-decoration-color: var(--color-text-tertiary);
    transition: all 0.15s ease;
  }

  .address-link:hover {
    text-decoration-color: var(--color-text);
  }

  .address-link .detail-value {
    transition: color 0.15s ease;
  }

  .address-link:hover .detail-value {
    color: var(--color-text);
  }

  .copy-btn {
    background: none;
    border: none;
    font-size: 14px;
    cursor: pointer;
    padding: 2px;
    opacity: 0.6;
    transition: opacity 0.15s ease;
  }

  .copy-btn:hover {
    opacity: 1;
  }

  .explorer-link {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    width: 100%;
    padding: 14px;
    background: var(--color-text);
    color: var(--color-background);
    border: none;
    border-radius: 10px;
    font-size: 15px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
    margin-top: auto;
  }

  .explorer-link:hover {
    background: #171717;
  }

  .explorer-link svg {
    transition: transform 0.15s ease;
  }

  .explorer-link:hover svg {
    transform: translateX(2px);
  }

  .pending-actions {
    padding: 16px;
    border: 1px solid var(--color-border);
    border-radius: 12px;
    background: var(--color-surface);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .pending-actions-header {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .pending-actions-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text);
  }

  .pending-actions-subtitle {
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  .pending-actions-row {
    display: flex;
    gap: 10px;
  }

  .pending-actions-row button {
    flex: 1;
  }

  .pending-error-banner {
    font-size: 12px;
    color: var(--color-error);
    background: rgba(240, 68, 56, 0.08);
    border: 1px solid rgba(240, 68, 56, 0.2);
    padding: 10px 12px;
    border-radius: 10px;
  }

  .pending-error {
    color: var(--color-error);
  }

  .btn-primary {
    padding: 12px 14px;
    border-radius: 10px;
    border: 1px solid transparent;
    font-weight: 600;
    cursor: pointer;
    background: var(--color-text);
    color: var(--color-background);
  }

  .btn-primary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .btn-secondary {
    padding: 12px 14px;
    border-radius: 10px;
    border: 1px solid var(--color-border);
    background: var(--color-background);
    color: var(--color-text);
    font-weight: 600;
    cursor: pointer;
  }

  .btn-secondary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .btn-secondary.danger {
    border-color: rgba(240, 68, 56, 0.35);
    color: var(--color-error);
  }

  .confirmations-badge {
    color: var(--color-success);
    font-weight: 500;
  }

  .pending-badge {
    color: var(--color-warning);
    font-weight: 500;
  }

  .failed-badge {
    color: var(--color-error);
    font-weight: 500;
  }
</style>
