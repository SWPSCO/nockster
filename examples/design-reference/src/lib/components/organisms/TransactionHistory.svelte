<script lang="ts">
  import Header from '../molecules/Header.svelte';

  export let transactions: any[] = [];
  export let onBack: () => void = () => {};

  // Mock exchange rate - in production this would come from an API
  const NOCK_TO_USD = 8.45;

  let searchQuery = '';

  function formatUSD(nockAmount: number): string {
    const usdAmount = nockAmount * NOCK_TO_USD;
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: 'USD',
      minimumFractionDigits: 2,
      maximumFractionDigits: 2
    }).format(usdAmount);
  }

  function generateMockTransactions() {
    return [
      {
        type: 'sent',
        amount: 50.00,
        address: 'nock1qvk...3n2p5m',
        fee: 0.02,
        date: '2 hours ago',
        confirmations: 12
      },
      {
        type: 'received',
        amount: 125.50,
        address: 'nock1abc...xyz',
        date: '1 day ago',
        confirmations: 144
      },
      {
        type: 'consolidation',
        amount: 0.00,
        notes: 'Combined 8 notes → 1 note',
        fee: 0.08,
        date: '2 days ago',
        confirmations: 288
      }
    ];
  }

  $: if (transactions.length === 0) {
    transactions = generateMockTransactions();
  }
</script>

<div class="transaction-history">
  <Header title="History" showBack={true} on:click={onBack} />
  
  <div class="content">
    <div class="search-bar">
      <input 
        type="text" 
        class="input-field" 
        placeholder="Search transactions..."
        bind:value={searchQuery}
      >
      <button class="filter-btn">Filter</button>
    </div>
    
    <div class="transaction-count">Showing {transactions.length} transactions</div>
    
    <div class="transaction-list">
      {#each transactions as transaction}
        <div class="transaction-card">
          <div class="transaction-header">
            <div class="transaction-type-section">
              <span class="transaction-type">
                {transaction.type === 'sent' ? 'Sent' :
                 transaction.type === 'received' ? 'Received' :
                 'Consolidation'}
              </span>
              <span class="transaction-date">{transaction.date}</span>
            </div>
            <div class="transaction-amounts">
              <span class="transaction-amount {transaction.type}">
                {transaction.type === 'sent' ? '-' :
                 transaction.type === 'received' ? '+' : ''}{transaction.amount.toFixed(2)} NOCK
              </span>
              {#if transaction.amount > 0}
                <span class="transaction-usd">
                  {formatUSD(transaction.amount)}
                </span>
              {/if}
            </div>
          </div>
          {#if transaction.address}
            <div class="transaction-detail">{transaction.type === 'sent' ? 'To' : 'From'}: {transaction.address}</div>
          {/if}
          {#if transaction.notes}
            <div class="transaction-detail">{transaction.notes}</div>
          {/if}
          {#if transaction.fee}
            <div class="transaction-detail">Fee: {transaction.fee} NOCK ({formatUSD(transaction.fee)})</div>
          {/if}
          <div class="transaction-detail">{transaction.confirmations} confirmations</div>
        </div>
      {/each}
    </div>
    
    <button class="btn-secondary">Export History</button>
  </div>
</div>

<style>
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
    border: 1px solid var(--color-border)394;
    border-radius: 8px;
    background: var(--color-surface);
    font-size: 14px;
    outline: none;
  }
  
  .filter-btn {
    padding: 0 16px;
    background: var(--color-surface);
    border: 1px solid var(--color-border)609;
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
    border: 1px solid var(--color-border)1024;
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
    border: 2px solid var(--color-border)2465;
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
</style>