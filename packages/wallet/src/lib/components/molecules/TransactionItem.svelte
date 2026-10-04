<script lang="ts">
  import Icon from '../atoms/Icon.svelte';
  import { formatNocksNumberWithSeparator } from '../../utils/nicks';

  type TransactionType = 'send' | 'receive';
  type TransactionStatus = 'pending' | 'confirmed' | 'failed';

  export let type: TransactionType;
  export let amount: number;
  export let address: string;
  export let date: Date;
  export let status: TransactionStatus = 'confirmed';
  export let confirmations: number = 0;

  $: formattedAmount = formatNocksNumberWithSeparator(amount, 8);
  $: formattedDate = new Intl.DateTimeFormat('en-US', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit'
  }).format(date);
  $: shortAddress = `${address.slice(0, 6)}...${address.slice(-4)}`;
  $: statusColor = {
    pending: 'var(--color-warning)',
    confirmed: 'var(--color-success)',
    failed: 'var(--color-error)'
  }[status];
</script>

<div class="transaction-item" on:click>
  <div class="transaction-icon">
    <Icon name={type} size="medium" />
  </div>

  <div class="transaction-details">
    <div class="transaction-header">
      <span class="transaction-type">
        {type === 'send' ? 'Sent' : 'Received'}
      </span>
      <span class="transaction-amount" class:sent={type === 'send'}>
        {type === 'send' ? '-' : '+'}{formattedAmount} NOCK
      </span>
    </div>

    <div class="transaction-meta">
      <span class="transaction-address">{shortAddress}</span>
      <span class="transaction-date">{formattedDate}</span>
    </div>

    {#if status !== 'confirmed'}
      <div class="transaction-status" style="color: {statusColor}">
        {#if status === 'pending'}
          <Icon name="refresh" size="small" />
          <span>Pending ({confirmations}/6)</span>
        {:else if status === 'failed'}
          <Icon name="error" size="small" />
          <span>Failed</span>
        {/if}
      </div>
    {/if}
  </div>

  <div class="transaction-arrow">
    <Icon name="chevronRight" size="small" color="var(--color-text-tertiary)" />
  </div>
</div>

<style>
  .transaction-item {
    display: flex;
    align-items: center;
    gap: var(--spacing-md);
    padding: var(--spacing-md);
    background: var(--color-background);
    border-bottom: 1px solid var(--color-border);
    cursor: pointer;
    transition: background var(--transition-fast);
  }

  .transaction-item:hover {
    background: var(--color-surface);
  }

  .transaction-item:active {
    background: var(--color-surface);
  }

  .transaction-icon {
    flex-shrink: 0;
  }

  .transaction-details {
    flex: 1;
    min-width: 0;
  }

  .transaction-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: var(--spacing-xs);
  }

  .transaction-type {
    font-size: var(--font-base);
    font-weight: var(--font-medium);
    color: var(--color-text);
  }

  .transaction-amount {
    font-size: var(--font-base);
    font-weight: var(--font-semibold);
    color: var(--color-text);
  }

  .transaction-amount.sent {
    color: var(--color-text);
  }

  .transaction-meta {
    display: flex;
    gap: var(--spacing-sm);
    font-size: var(--font-sm);
    color: var(--color-text-secondary);
  }

  .transaction-address {
    font-family: 'SF Mono', Monaco, monospace;
  }

  .transaction-status {
    display: flex;
    align-items: center;
    gap: var(--spacing-xs);
    margin-top: var(--spacing-xs);
    font-size: var(--font-sm);
  }

  .transaction-arrow {
    flex-shrink: 0;
  }
</style>
