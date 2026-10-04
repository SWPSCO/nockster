<script lang="ts">
  import Card from '../atoms/Card.svelte';
  import Icon from '../atoms/Icon.svelte';
  
  export let name: string;
  export let balance: number;
  export let address: string;
  export let selected: boolean = false;
  export let networkType: 'mainnet' | 'testnet' = 'mainnet';
  
  $: formattedBalance = balance.toFixed(8);
  $: shortAddress = `${address.slice(0, 8)}...${address.slice(-8)}`;
  $: networkColor = networkType === 'mainnet' ? 'var(--color-success)' : 'var(--color-warning)';
</script>

<Card 
  clickable={true} 
  {selected}
  padding="medium"
  on:click
>
  <div class="wallet-card">
    <div class="wallet-header">
      <div class="wallet-info">
        <h3 class="wallet-name">{name}</h3>
        <span class="wallet-network" style="color: {networkColor}">
          {networkType === 'mainnet' ? 'Mainnet' : 'Testnet'}
        </span>
      </div>
      {#if selected}
        <div class="wallet-selected">
          <Icon name="check" size="small" color="var(--color-success)" />
        </div>
      {/if}
    </div>
    
    <div class="wallet-balance">
      <span class="balance-amount">{formattedBalance}</span>
      <span class="balance-unit">BTC</span>
    </div>
    
    <div class="wallet-address">
      <Icon name="wallet" size="small" color="var(--color-text-tertiary)" />
      <span class="address-text">{shortAddress}</span>
    </div>
  </div>
</Card>

<style>
  .wallet-card {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-md);
  }
  
  .wallet-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
  }
  
  .wallet-info {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-xs);
  }
  
  .wallet-name {
    font-size: var(--font-lg);
    font-weight: var(--font-semibold);
    color: var(--color-text);
    margin: 0;
  }
  
  .wallet-network {
    font-size: var(--font-xs);
    font-weight: var(--font-medium);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  
  .wallet-selected {
    width: 24px;
    height: 24px;
    border-radius: var(--radius-full);
    background: rgba(16, 185, 129, 0.1);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  
  .wallet-balance {
    display: flex;
    align-items: baseline;
    gap: var(--spacing-xs);
  }
  
  .balance-amount {
    font-size: var(--font-3xl);
    font-weight: var(--font-bold);
    color: var(--color-text);
    line-height: 1;
  }
  
  .balance-unit {
    font-size: var(--font-lg);
    font-weight: var(--font-medium);
    color: var(--color-text-secondary);
  }
  
  .wallet-address {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    padding: var(--spacing-sm);
    background: var(--color-surface);
    border-radius: var(--radius-md);
  }
  
  .address-text {
    font-family: 'SF Mono', Monaco, monospace;
    font-size: var(--font-sm);
    color: var(--color-text-secondary);
  }
</style>