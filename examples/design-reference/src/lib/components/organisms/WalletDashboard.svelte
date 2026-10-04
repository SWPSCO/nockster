<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let balance: number = 1234.56;
  export let noteCount: number = 12;
  export let recentTransactions: any[] = [];
  export let activeTab: 'activity' | 'addressBook' = 'activity';
  export let walletName: string = 'Main Wallet';
  export let walletAddress: string = 'nc1q5xvftzgd8a4x8f6g3k4jw5a2j4k5j6k5j6k5j6';
  export let onSettings: () => void = () => {};
  export let onManageWallets: () => void = () => {};

  // Mock exchange rate - in production this would come from an API
  const NOCK_TO_USD = 8.45;

  function formatUSD(nockAmount: number): string {
    const usdAmount = nockAmount * NOCK_TO_USD;
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: 'USD',
      minimumFractionDigits: 2,
      maximumFractionDigits: 2
    }).format(usdAmount);
  }

  function formatWalletAddress(address: string): string {
    if (!address || address.length <= 8) return address;
    return `${address.slice(0, 4)}...${address.slice(-4)}`;
  }

  const dispatch = createEventDispatcher();

  function generateMockTransactions() {
    return [
      {
        id: 'tx-1',
        type: 'sent',
        amount: 50.00,
        date: '2 hours ago',
        address: 'nc1q5xvftzgd8a4x8f6g3k4jw5a2j4k5j6k5j6k5j6',
        fee: 0.02,
        confirmations: 12,
        status: 'confirmed'
      },
      {
        id: 'tx-2',
        type: 'received',
        amount: 125.50,
        date: '1 day ago',
        address: 'nc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq',
        fee: 0.02,
        confirmations: 144,
        status: 'confirmed'
      },
      {
        id: 'tx-3',
        type: 'sent',
        amount: 10.25,
        date: '2 days ago',
        address: 'nc1q7xvftzgd8a4x8f6g3k4jw5a2j4k5j6k5j6k5j6',
        fee: 0.01,
        confirmations: 288,
        status: 'confirmed'
      }
    ];
  }

  function handleTransactionClick(transaction: any) {
    dispatch('navigate', { route: 'transaction-details', data: transaction });
  }
  
  $: if (recentTransactions.length === 0) {
    recentTransactions = generateMockTransactions();
  }
</script>

<div class="wallet-dashboard fixed-screen">
  <div class="header">
    <div class="network-status">
      <span class="status-dot"></span>
      <span class="status-text">Connected</span>
    </div>
    <button class="wallet-selector" on:click={onManageWallets}>
      <div class="wallet-info">
        <div class="wallet-name">{walletName}</div>
        <div class="wallet-address">{formatWalletAddress(walletAddress)}</div>
      </div>
      <svg width="12" height="7" viewBox="0 0 12 7" fill="none" class="chevron">
        <path d="M1 1L6 6L11 1" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
    </button>
    <button class="menu-btn" on:click={onSettings}>
      <svg width="20" height="14" viewBox="0 0 20 14" fill="none">
        <path d="M0 1H20M0 7H20M0 13H20" stroke="currentColor" stroke-width="1.5"/>
      </svg>
    </button>
  </div>
  
  <div class="content">
    <div class="balance-display">
      <div class="balance-label">Total Balance</div>
      <div class="balance-amount">{balance.toFixed(2)} NOCK</div>
      <div class="balance-usd">{formatUSD(balance)}</div>
      <div class="balance-notes">{noteCount} unspent outputs</div>
    </div>
    
    <div class="quick-actions">
      <button class="quick-action-btn" on:click={() => dispatch('navigate', 'send')}>Send</button>
      <button class="quick-action-btn" on:click={() => dispatch('navigate', 'receive')}>Receive</button>
      <button class="quick-action-btn">Consolidate</button>
    </div>
    
    <div class="tabs">
      <button
        class="tab"
        class:active={activeTab === 'activity'}
        on:click={() => activeTab = 'activity'}
      >
        Activity
      </button>
      <button
        class="tab"
        class:active={activeTab === 'addressBook'}
        on:click={() => activeTab = 'addressBook'}
      >
        Address Book
      </button>
    </div>
    
    {#if activeTab === 'activity'}
      <div class="transaction-list">
        {#each recentTransactions as transaction}
          <button class="transaction-item" on:click={() => handleTransactionClick(transaction)}>
            <div class="transaction-info">
              <div class="transaction-date">{transaction.date}</div>
              <div class="transaction-usd">{formatUSD(transaction.amount)}</div>
            </div>
            <div class="transaction-amount {transaction.type}">{transaction.type === 'sent' ? '-' : '+'}{transaction.amount.toFixed(2)} NOCK</div>
          </button>
        {/each}
      </div>
    {/if}
    
    {#if activeTab === 'addressBook'}
      <div class="address-book">
        <p style="text-align: center; color: var(--color-text-secondary); padding: 16px;">Address book content here</p>
      </div>
    {/if}
    
    {#if activeTab === 'settings'}
      <div class="settings">
        <p style="text-align: center; color: var(--color-text-secondary); padding: 16px;">Settings content here</p>
      </div>
    {/if}
  </div>
</div>

<style>
  .wallet-dashboard {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-background);
  }
  
  .header {
    height: 60px;
    background: var(--color-background);
    border-bottom: 1px solid var(--color-border);
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    padding: 0 20px;
  }

  .network-status {
    display: flex;
    align-items: center;
    gap: 6px;
    position: relative;
    cursor: default;
  }

  .status-dot {
    width: 8px;
    height: 8px;
    background: var(--color-success);
    border-radius: 50%;
  }

  .status-text {
    font-size: 13px;
    color: var(--color-text-secondary);
    opacity: 0;
    transition: opacity 0.2s ease;
    position: absolute;
    left: 16px;
    white-space: nowrap;
  }

  .network-status:hover .status-text {
    opacity: 1;
  }

  .wallet-selector {
    font-size: 14px;
    font-weight: 500;
    color: var(--color-text);
    padding: 10px 14px 10px 16px;
    background: var(--color-surface);
    border: none;
    border-radius: 20px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
    justify-self: center;
    position: relative;
    white-space: nowrap;
  }

  .wallet-info {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
  }

  .wallet-name {
    font-size: 14px;
    font-weight: 500;
    color: var(--color-text);
    line-height: 1;
  }

  .wallet-address {
    font-size: 11px;
    font-weight: 400;
    color: var(--color-text-tertiary);
    line-height: 1;
    font-family: 'SF Mono', 'Monaco', 'Inconsolata', 'Roboto Mono', monospace;
  }

  /* Enhanced animations for light theme */
  :global([data-theme="light"]) .wallet-selector {
    background: rgba(0, 0, 0, 0.04);
    border: 1px solid rgba(0, 0, 0, 0.06);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.05),
                0 1px 2px rgba(0, 0, 0, 0.04);
  }

  :global([data-theme="light"]) .wallet-selector:hover {
    background: rgba(0, 0, 0, 0.06);
    border-color: rgba(0, 0, 0, 0.08);
    transform: translateY(-1px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.08),
                0 2px 4px rgba(0, 0, 0, 0.04);
  }

  :global([data-theme="light"]) .wallet-selector:active {
    background: rgba(0, 0, 0, 0.03);
    transform: scale(0.98) translateY(0);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.06);
  }

  /* Enhanced animations for dark theme */
  :global([data-theme="dark"]) .wallet-selector {
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.06);
    box-shadow: 0 2px 12px rgba(0, 0, 0, 0.2),
                inset 0 1px 1px rgba(255, 255, 255, 0.05);
  }

  :global([data-theme="dark"]) .wallet-selector:hover {
    background: rgba(255, 255, 255, 0.11);
    border-color: rgba(255, 255, 255, 0.08);
    transform: translateY(-1px);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3),
                inset 0 1px 1px rgba(255, 255, 255, 0.08);
  }

  :global([data-theme="dark"]) .wallet-selector:active {
    background: rgba(255, 255, 255, 0.06);
    transform: scale(0.98) translateY(0);
    box-shadow: 0 1px 6px rgba(0, 0, 0, 0.2);
  }


  .wallet-selector .chevron {
    opacity: 0.5;
    transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
    margin-left: 2px;
    transform: translateY(0);
  }

  /* Rotating chevron animation for all themes */
  :global([data-theme="light"]) .wallet-selector:hover .chevron,
  :global([data-theme="dark"]) .wallet-selector:hover .chevron {
    transform: rotate(180deg);
    opacity: 0.6;
  }

  :global([data-theme="light"]) .wallet-selector:active .chevron,
  :global([data-theme="dark"]) .wallet-selector:active .chevron {
    transform: rotate(0deg);
  }

  .menu-btn {
    background: none;
    border: none;
    cursor: pointer;
    padding: 8px;
    color: var(--color-text-secondary);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: color 0.15s ease;
    justify-self: end;
  }

  .menu-btn:hover {
    color: var(--color-text);
  }
  
  .content {
    flex: 1;
    overflow: hidden;
    padding: 0 20px 20px;
  }
  
  .balance-display {
    text-align: center;
    margin-bottom: 40px;
    padding-top: 24px;
  }

  .balance-label {
    font-size: 12px;
    color: var(--color-text-tertiary);
    text-transform: none;
    letter-spacing: 0;
    margin-bottom: 8px;
    font-weight: 400;
  }

  .balance-amount {
    font-size: 42px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 4px;
    line-height: 1;
    letter-spacing: -1px;
  }

  .balance-usd {
    font-size: 18px;
    color: var(--color-text-secondary);
    font-weight: 400;
    margin-bottom: 12px;
  }

  .balance-notes {
    font-size: 12px;
    color: var(--color-text-tertiary);
    font-weight: 400;
  }
  
  .quick-actions {
    display: flex;
    gap: 10px;
    margin-bottom: 32px;
  }

  .quick-action-btn {
    flex: 1;
    padding: 14px;
    background: var(--color-background);
    border: 1.5px solid var(--color-border);
    border-radius: 10px;
    font-size: 15px;
    font-weight: 500;
    color: var(--color-text);
    cursor: pointer;
    transition: all 150ms ease;
  }

  .quick-action-btn:hover {
    background: var(--color-surface);
    border-color: var(--color-border);
  }
  
  .tabs {
    display: flex;
    gap: 24px;
    border-bottom: 1px solid var(--color-surface);
    margin-bottom: 24px;
  }

  .tab {
    padding: 12px 4px;
    text-align: center;
    font-size: 14px;
    color: var(--color-text-tertiary);
    cursor: pointer;
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    transition: all 150ms ease;
    position: relative;
  }

  .tab.active {
    color: var(--color-text);
    font-weight: 500;
  }

  .tab.active::after {
    content: '';
    position: absolute;
    bottom: -1px;
    left: 0;
    right: 0;
    height: 2px;
    background: var(--color-text);
  }
  
  .transaction-list {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  
  .transaction-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 0;
    border-bottom: 1px solid var(--color-surface);
    background: none;
    border-left: none;
    border-right: none;
    border-top: none;
    width: 100%;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .transaction-item:hover {
    background: var(--color-surface);
    margin: 0 -20px;
    padding: 12px 20px;
  }

  .transaction-item:last-child {
    border-bottom: none;
  }

  .transaction-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .transaction-date {
    font-size: 14px;
    color: var(--color-text-secondary);
    font-weight: 400;
  }

  .transaction-usd {
    font-size: 12px;
    color: var(--color-text-tertiary);
    font-weight: 400;
  }

  .transaction-amount {
    font-size: 16px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }

  .transaction-amount.sent {
    color: var(--color-text-secondary);
  }

  .transaction-amount.received {
    color: var(--color-success);
  }
</style>