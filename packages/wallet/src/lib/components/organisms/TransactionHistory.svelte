<script lang="ts">
  import { formatUsdEstimate } from '../../utils/usd';
  import Header from '../molecules/Header.svelte';
  import { nockPrice } from '../../stores/price';
  import type { TransactionData } from '../../utils/rpc';
  import { activeWallet } from '../../stores/wallet';
  import { formatNocksWithSeparator, nicksToNocks } from '../../utils/nicks';

  export let transactions: TransactionData[] = [];
  export let onBack: () => void = () => {};
  export let isLoading: boolean = false;

  let searchQuery = '';

  function formatUSD(nockAmount: number): string {
    return formatUsdEstimate(nockAmount, $nockPrice) ?? '';
  }

  function formatTimestamp(timestamp: number): string {
    const now = Date.now();
    const diff = now - timestamp;

    const seconds = Math.floor(diff / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);

    if (days > 0) return `${days} day${days > 1 ? 's' : ''} ago`;
    if (hours > 0) return `${hours} hour${hours > 1 ? 's' : ''} ago`;
    if (minutes > 0) return `${minutes} minute${minutes > 1 ? 's' : ''} ago`;
    return 'just now';
  }

  function formatAddress(address: string): string {
    if (!address || address.length < 10) return address;
    return `${address.slice(0, 6)}...${address.slice(-6)}`;
  }

  function getTransactionType(
    tx: TransactionData
  ): 'sent' | 'received' | 'coinbase' | 'consolidation' {
    if (tx.isCoinbase) return 'coinbase';
    // Use the type directly from the API response when available
    if (tx.type === 'received') return 'received';
    if (tx.type === 'self') return 'consolidation';
    if (tx.type === 'sent') return 'sent';
    // Fallback to address comparison for legacy data
    const currentAddress = $activeWallet?.addresses[$activeWallet.currentAddressIndex];
    if (tx.from === currentAddress && tx.to === currentAddress) return 'consolidation';
    return tx.to === currentAddress ? 'received' : 'sent';
  }
</script>

<div class="transaction-history">
  <Header title="History" showBack={true} on:click={onBack} />

  <div class="content">
    <div class="wallet-context"><strong>{$activeWallet?.name ?? 'Wallet'}</strong><span>{formatAddress($activeWallet?.addresses[$activeWallet.currentAddressIndex] ?? '')}</span></div>
    <div class="search-bar">
      <input
        type="text"
        class="input-field"
        placeholder="Search transactions..."
        bind:value={searchQuery}
      />
      <button class="filter-btn">Filter</button>
    </div>

    {#if isLoading}
      <div class="loading-state">Loading transactions...</div>
    {:else if transactions.length === 0}
      <div class="empty-state">
        <p>No transactions yet</p>
        <p class="empty-subtitle">Your transaction history will appear here</p>
      </div>
    {:else}
      <div class="transaction-count">Showing {transactions.length} transactions</div>

      <div class="transaction-list">
        {#each transactions as transaction}
          {@const txType = getTransactionType(transaction)}
          {@const counterpartyAddress = transaction.bridge?.destination ?? (txType === 'sent' ? transaction.to : transaction.from)}
          {@const amountNicks = BigInt(Math.trunc(transaction.amount || 0))}
          {@const feeNicks = BigInt(Math.trunc(transaction.fee || 0))}
          {@const amountNOCK = nicksToNocks(amountNicks)}
          {@const feeNOCK = nicksToNocks(feeNicks)}
          <div class="transaction-card">
            <div class="transaction-header">
              <div class="transaction-type-section">
                <span class="transaction-type">
                  {transaction.bridge ? 'Bridge to Base' : txType === 'coinbase'
                    ? 'Coinbase Reward'
                    : txType === 'sent'
                      ? 'Sent'
                      : txType === 'consolidation'
                        ? 'Consolidation'
                        : 'Received'}
                </span>
                <span class="transaction-date">{formatTimestamp(transaction.timestamp)}</span>
              </div>
              <div class="transaction-amounts">
                <span
                  class="transaction-amount {txType === 'sent'
                    ? 'sent'
                    : txType === 'consolidation'
                      ? 'consolidation'
                      : 'received'}"
                >
                  {txType === 'sent' ? '-' : txType === 'received' ? '+' : ''}{formatNocksWithSeparator(
                    amountNicks,
                    2
                  )} NOCK
                </span>
                {#if amountNOCK > 0}
                  <span class="transaction-usd">
                    {formatUSD(amountNOCK)}
                  </span>
                {/if}
              </div>
            </div>
            {#if counterpartyAddress && txType !== 'coinbase' && txType !== 'consolidation'}
              <div class="transaction-detail">
                {transaction.bridge ? 'To Base' : txType === 'sent' ? 'To' : 'From'}: {formatAddress(counterpartyAddress)}
              </div>
            {/if}
            {#if transaction.notes}
              <div class="transaction-detail">{transaction.notes}</div>
            {/if}
            {#if transaction.fee}
              <div class="transaction-detail">
                Fee: {formatNocksWithSeparator(feeNicks, 8)} NOCK ({formatUSD(feeNOCK)})
              </div>
            {/if}
            {#if transaction.blockHeight}
              <div class="transaction-detail">Block: {transaction.blockHeight.toLocaleString('en-US')}</div>
            {/if}
          </div>
        {/each}
      </div>

      <button class="btn-secondary">Export History</button>
    {/if}
  </div>
</div>

<style>
  .wallet-context { display: flex; gap: 12px; align-items: baseline; margin-bottom: 16px; flex-wrap: wrap; }
  .wallet-context span { font-size: 12px; color: var(--color-text-secondary); font-family: monospace; }
  .transaction-history {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-background);
  }

  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 16px;
    overflow-y: auto;
  }

  .search-bar {
    display: flex;
    gap: 8px;
    margin-bottom: 16px;
  }

  .input-field {
    flex: 1;
    padding: 16px;
    border: 1px solid var(--color-border) 394;
    border-radius: 8px;
    background: var(--color-surface);
    font-size: 14px;
    outline: none;
  }

  .filter-btn {
    padding: 0 16px;
    background: var(--color-surface);
    border: 1px solid var(--color-border) 609;
    border-radius: 8px;
    cursor: pointer;
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text);
  }

  .transaction-count {
    font-size: 14px;
    color: var(--color-text-secondary);
    margin-bottom: 16px;
  }

  .transaction-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 16px;
  }

  .transaction-card {
    background: var(--color-background);
    border: 1px solid var(--color-border) 1024;
    border-radius: 10px;
    padding: 16px;
    transition: all 0.15s ease;
  }

  .transaction-card:hover {
    border-color: var(--color-border);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.05);
  }

  .transaction-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 10px;
  }

  .transaction-type-section {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .transaction-type {
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text);
  }

  .transaction-date {
    font-size: 12px;
    color: var(--color-text-tertiary);
  }

  .transaction-amounts {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
  }

  .transaction-amount {
    font-size: 16px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--color-text);
  }

  .transaction-usd {
    font-size: 12px;
    color: var(--color-text-secondary);
    font-weight: 400;
  }

  .transaction-amount.sent {
    color: var(--color-text-secondary);
  }

  .transaction-amount.received {
    color: var(--color-success);
  }

  .transaction-amount.consolidation {
    color: var(--color-text);
  }

  .transaction-detail {
    font-size: 13px;
    color: var(--color-text-secondary);
    margin-bottom: 4px;
    line-height: 1.4;
  }

  .transaction-detail:last-child {
    margin-bottom: 0;
  }

  .btn-secondary {
    background: var(--color-background);
    color: var(--color-text);
    border: 2px solid var(--color-border) 2465;
    border-radius: 8px;
    padding: 12px 20px;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
    width: 100%;
    margin-top: 16px;
  }

  .btn-secondary:hover {
    background: var(--color-surface);
  }

  .loading-state,
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 48px 24px;
    text-align: center;
    color: var(--color-text-secondary);
  }

  .empty-state p {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text);
  }

  .empty-subtitle {
    font-size: 14px;
    font-weight: 400;
    color: var(--color-text-secondary);
    margin-top: 8px;
  }
</style>
