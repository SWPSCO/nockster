<script lang="ts">
  import { pendingStatus, pendingWithoutConfirmed } from '../../utils/pendingStatus';
  import { formatUsdEstimate } from '../../utils/usd';
  import { onMount, onDestroy } from 'svelte';
  import { walletStore, activeWallet } from '../../stores/wallet';
  import { transactionStore } from '../../stores/transactions';
  import { getRPCClient } from '../../utils/rpc';
  import { nockPrice } from '../../stores/price';
  import { formatNocksWithSeparator, nicksToNocks, toNicks } from '../../utils/nicks';
  import type { Nicks } from '../../types/nicks';
  import ConnectionStatus from '../molecules/ConnectionStatus.svelte';
  import type { Route } from '../../stores/router';
  import {
    startPendingTransactionPoller,
    stopPendingTransactionPoller
  } from '../../services/pendingTransactionPoller';
  import type { PendingTransaction } from '../../types/wallet';

  export let balance: number = 0;
  export let noteCount: number = 0;
  export let recentTransactions: any[] = [];
  export let activeTab: 'activity' | 'tokens' | 'nfts' = 'activity';

  let isLoading = true;
  let error: string | null = null;
  export let walletName: string = 'Main Wallet';
  export let walletAddress: string = '';
  export let onSettings: () => void = () => {};
  export let onManageWallets: () => void = () => {};
  export let onNavigate: (route: Route, data?: any) => void = () => {};

  const priceFormatter = new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 2,
    maximumFractionDigits: 6
  });
  $: currentPrice = $nockPrice > 0 ? priceFormatter.format($nockPrice) : null;

  function formatUSD(nicksAmount: number): string {
    return formatUsdEstimate(nicksAmount / 65536, $nockPrice) ?? '';
  }

  function formatBalanceInNocks(nicksAmount: number): string {
    // Convert nicks to NOCKS for display
    const nockAmount = nicksToNocks(BigInt(Math.trunc(nicksAmount)));
    return formatNocksWithSeparator(BigInt(Math.trunc(nicksAmount)), 2);
  }

  function usdFromNicks(nicksAmount: number | bigint): string {
    return formatUsdEstimate(Number(nicksAmount) / 65536, $nockPrice) ?? '';
  }

  // Format USD when you already have nocks (decimal)
  function usdFromNocks(nocksAmount: number): string {
    return formatUsdEstimate(nocksAmount, $nockPrice) ?? '';
  }

  // Show balance in nocks when the source is nicks
  function nocksFromNicks(nicksAmount: number | bigint): string {
    const nicksBI = typeof nicksAmount === 'bigint' ? nicksAmount : BigInt(Math.trunc(nicksAmount));
    return formatNocksWithSeparator(nicksBI, 2);
  }

  // Fix: proper template string
  function formatWalletAddress(address: string): string {
    if (!address || address.length <= 11) return address;
    return `${address.slice(0, 6)}...${address.slice(-4)}`;
  }

  function formatTransactionTime(timestamp: number): string {
    const now = Date.now();
    const diff = now - timestamp;

    const seconds = Math.floor(diff / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);

    if (days > 7) {
      const date = new Date(timestamp);
      return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
    } else if (days > 0) {
      return `${days}d ago`;
    } else if (hours > 0) {
      return `${hours}h ago`;
    } else if (minutes > 0) {
      return `${minutes}m ago`;
    } else {
      return 'Just now';
    }
  }

  function getCounterpartyAddress(transaction: any): string {
    // For coinbase transactions, show "Coinbase Reward"
    if (transaction.isCoinbase) {
      return 'Coinbase Reward';
    }
    // For sent transactions, show the 'to' address
    // For received transactions, show the 'from' address
    if (transaction.type === 'sent') {
      return transaction.to || '';
    } else {
      return transaction.from || '';
    }
  }

  function getTransactionType(transaction: any): 'sent' | 'received' {
    if (transaction.isCoinbase) return 'received';
    // Pending transactions are always 'sent' (we don't track pending receives)
    if (transaction.isPending) return 'sent';
    return transaction.type || 'received';
  }

  // Convert pending transactions to display format
  function convertPendingToDisplayFormat(pending: PendingTransaction[]): any[] {
    return pending.map(tx => ({
      pendingStatusLabel: pendingStatus(tx).label,
      bridge: tx.bridge,
      txId: tx.txId,
      amount: tx.totalAmount,
      fee: tx.fee,
      timestamp: tx.timestamp,
      type: 'sent',
      from: tx.fromAddress,
      to: tx.bridge?.destination ?? tx.recipients[0]?.address ?? '', // Show first recipient
      isPending: true,
      status: 'pending'
    }));
  }

  // Merge pending transactions with confirmed ones
  // Pending transactions appear at the top, sorted by timestamp
  $: mergedTransactions = (() => {
    const pendingTxs = pendingWithoutConfirmed(
      $activeWallet?.pendingTransactions,
      $activeWallet?.transactions ?? recentTransactions
    );
    const confirmedTxs = ($activeWallet?.transactions ?? recentTransactions).filter(
      tx => !pendingTxs.some(p => p.txId === tx.txId)
    );
    const pendingDisplay = convertPendingToDisplayFormat(pendingTxs);
    return [...pendingDisplay, ...confirmedTxs].slice(0, 10);
  })();

  // Calculate total pending outgoing amount (amount + fee for each pending tx)
  $: totalPendingOutgoing = (() => {
    const pendingTxs = pendingWithoutConfirmed(
      $activeWallet?.pendingTransactions,
      $activeWallet?.transactions ?? recentTransactions
    );
    return pendingTxs.reduce((sum, tx) => sum + tx.totalAmount + (tx.fee || 0), 0);
  })();

  // Effective balance = RPC balance - pending outgoing
  // This provides immediate feedback even before RPC catches up
  $: effectiveBalance = Math.max(0, balance - totalPendingOutgoing);

  onMount(async () => {
    error = null;

    // Start polling for pending transaction confirmations
    startPendingTransactionPoller();

    try {
      if ($activeWallet) {
        // Set wallet info
        walletName = $activeWallet.name;
        const addressIndex = $activeWallet.currentAddressIndex || 0;
        walletAddress = $activeWallet.addresses?.[addressIndex] || '';

        // Debug logging
        console.log('[Dashboard] Wallet name:', walletName);
        console.log('[Dashboard] Address index:', addressIndex);
        console.log('[Dashboard] Address:', walletAddress);
        console.log('[Dashboard] All addresses:', $activeWallet.addresses);
        console.log('[Dashboard] MasterPublicKey:', $activeWallet.masterPublicKey);

        // Load cached data immediately (no loading state)
        balance = $activeWallet.balance || 0;
        noteCount = $activeWallet.notes?.length || 0;
        recentTransactions = $activeWallet.transactions?.slice(0, 10) || [];

        // Only fetch data if we have a valid address
        if (walletAddress) {
          // Fetch will use cache-first strategy and refresh in background if needed
          isLoading = true;

          // Fetch balance from RPC (cache-first)
          await walletStore.fetchBalance($activeWallet.id);

          // Fetch notes (cache-first)
          const notes = await walletStore.fetchNotes($activeWallet.id);
          noteCount = notes?.length || 0;

          // Fetch transactions (cache-first with merge)
          const txs = await walletStore.fetchTransactions($activeWallet.id);
          recentTransactions = txs?.slice(0, 10) || [];

          // Update balance from wallet store
          balance = $activeWallet.balance || 0;

          isLoading = false;
        } else {
          console.warn('No valid wallet address found');
          balance = 0;
          noteCount = 0;
          recentTransactions = [];
        }
      }
    } catch (err) {
      console.error('Failed to load wallet data:', err);
      error = err instanceof Error ? err.message : 'Failed to load wallet data';
      recentTransactions = [];
      isLoading = false;
    }
  });

  onDestroy(() => {
    stopPendingTransactionPoller();
  });

  async function refreshWallet() {
    if ($activeWallet) {
      isLoading = true;
      try {
        const addressIndex = $activeWallet.currentAddressIndex || 0;
        walletAddress = $activeWallet.addresses?.[addressIndex] || '';

        if (walletAddress) {
          // Force refresh (bypass cache)
          await walletStore.fetchBalance($activeWallet.id, true);
          const notes = await walletStore.fetchNotes($activeWallet.id, true);
          noteCount = notes?.length || 0;
          const txs = await walletStore.fetchTransactions($activeWallet.id, true);
          recentTransactions = txs?.slice(0, 10) || [];
          balance = $activeWallet.balance || 0;
        } else {
          console.warn('No valid wallet address for refresh');
          recentTransactions = [];
        }
      } catch (err) {
        console.error('Failed to refresh wallet:', err);
      } finally {
        isLoading = false;
      }
    }
  }

  function handleTransactionClick(transaction: any) {
    onNavigate('transaction-details', transaction);
  }

  // Comment out auto-mock data - show empty state instead
  // $: if (!isLoading && recentTransactions.length === 0 && !error) {
  //   recentTransactions = generateMockTransactions();
  // }
</script>

<div class="wallet-dashboard fixed-screen">
  <div class="header">
    <ConnectionStatus />
    <div class="wallet-identity">
      <button
        type="button"
        class="wallet-selector"
        aria-label={`Manage wallets: ${walletName}`}
        onclick={() => {
          console.log('🔍 Wallet selector clicked - calling onManageWallets');
          console.log('onManageWallets function:', onManageWallets);
          onManageWallets();
        }}
      >
        <span class="wallet-name">{walletName}</span>
        <svg width="12" height="7" viewBox="0 0 12 7" fill="none" class="chevron">
          <path
            d="M1 1L6 6L11 1"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </button>
      <div class="wallet-address-row">
        <span class="wallet-address" title={walletAddress}
          >{formatWalletAddress(walletAddress)}</span
        >
      </div>
    </div>
    <div class="header-actions">
      <div
        class="nock-price"
        title={currentPrice ? `1 NOCK = ${currentPrice} USD` : 'NOCK price unavailable'}
        aria-label={currentPrice ? `NOCK price: ${currentPrice} USD` : 'NOCK price unavailable'}
      >
        <span class="price-label">NOCK</span>
        <span class="price-value">{currentPrice ?? '—'}</span>
      </div>
      <button
        class="menu-btn"
        aria-label="Settings menu"
        onclick={() => {
          console.log('⚙️ Settings menu clicked - calling onSettings');
          console.log('onSettings function:', onSettings);
          onSettings();
        }}
      >
        <svg width="20" height="14" viewBox="0 0 20 14" fill="none">
          <path d="M0 1H20M0 7H20M0 13H20" stroke="currentColor" stroke-width="1.5" />
        </svg>
      </button>
    </div>
  </div>

  <div class="content">
    {#if isLoading}
      <div class="loading-container">
        <div class="spinner"></div>
        <p>Loading wallet data...</p>
      </div>
    {:else if error}
      <div class="error-container">
        <p class="error-message">⚠️ {error}</p>
        <button class="retry-btn" onclick={refreshWallet}>Retry</button>
      </div>
    {:else}
      <div class="balance-display">
        <div class="balance-amount">{nocksFromNicks(effectiveBalance)} NOCK</div>
        <div class="balance-usd">{usdFromNicks(effectiveBalance)}</div>
        {#if totalPendingOutgoing > 0}
          <div class="pending-indicator">
            <span class="pending-dot"></span>
            {nocksFromNicks(totalPendingOutgoing)} pending
          </div>
        {/if}
      </div>
    {/if}

    <div class="quick-actions">
      <button class="quick-action-btn" onclick={() => onNavigate('send')}>
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
          <path
            d="M8 2L8 14M8 2L3 7M8 2L13 7"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
        Send
      </button>
      <button class="quick-action-btn" onclick={() => onNavigate('receive')}>
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
          <path
            d="M8 14L8 2M8 14L3 9M8 14L13 9"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
        Receive
      </button>
    </div>

    {#if activeTab === 'activity'}
      {#if import.meta.env.MODE === 'desktop'}
        <div class="desktop-activity-heading">
          <h2>Recent activity</h2>
          <button onclick={() => onNavigate('history')}>View all</button>
        </div>
      {/if}
      <div class="transaction-list">
        {#if mergedTransactions.length === 0}
          <div class="empty-state">{import.meta.env.MODE === 'desktop' ? 'No transactions yet.' : 'no transactions'}</div>
        {:else}
          <div class="transaction-list-inner">
            {#each mergedTransactions as transaction}
              {@const txType = getTransactionType(transaction)}
              <button class="transaction-item" onclick={() => handleTransactionClick(transaction)}>
                <div class="transaction-left">
                  <div class="transaction-icon {txType}">
                    {#if txType === 'sent'}
                      <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                        <path
                          d="M8 14L8 2M8 2L3 7M8 2L13 7"
                          stroke="currentColor"
                          stroke-width="2"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                        />
                      </svg>
                    {:else}
                      <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                        <path
                          d="M8 2L8 14M8 14L3 9M8 14L13 9"
                          stroke="currentColor"
                          stroke-width="2"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                        />
                      </svg>
                    {/if}
                  </div>
                  <div class="transaction-details">
                    <div class="transaction-address">
                      {#if transaction.bridge || transaction.isCoinbase}
                        {transaction.bridge ? 'Bridge to Base' : 'Coinbase Reward'}
                      {:else}
                        <span
                          >{txType === 'sent'
                            ? 'To:'
                            : transaction.type === 'self'
                              ? 'Own:'
                              : 'From:'}</span
                        >
                        <span class="counterparty" title={getCounterpartyAddress(transaction)}
                          >{formatWalletAddress(getCounterpartyAddress(transaction))}</span
                        >
                      {/if}
                    </div>
                    <div class="transaction-time {transaction.isPending ? 'pending' : ''}">
                      {#if transaction.isPending}
                        {transaction.pendingStatusLabel}
                      {:else}
                        {formatTransactionTime(transaction.timestamp)}
                      {/if}
                    </div>
                  </div>
                </div>
                <div class="transaction-right">
                  <div class="transaction-amount {txType}">
                    <span class="transaction-number"
                      >{txType === 'sent' ? '−' : '+'}{formatNocksWithSeparator(
                        BigInt(
                          Math.trunc(
                            txType === 'sent'
                              ? transaction.amount + (transaction.fee || 0)
                              : transaction.amount
                          )
                        ),
                        2
                      )}</span
                    > NOCK
                  </div>
                  <div class="transaction-usd">
                    {usdFromNicks(
                      txType === 'sent'
                        ? transaction.amount + (transaction.fee || 0)
                        : transaction.amount
                    )}
                  </div>
                </div>
              </button>
            {/each}
          </div>
        {/if}
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
    flex-shrink: 0;
    min-height: 84px;
    background: var(--color-background);
    border-bottom: 1px solid var(--color-border);
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 12px;
    align-items: center;
    padding: 12px 20px;
  }
  .wallet-identity {
    min-width: 0;
  }
  .wallet-selector {
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: 100%;
    min-height: 28px;
    border: none;
    background: transparent;
    color: var(--color-text);
    text-align: left;
    cursor: pointer;
  }
  .wallet-selector:hover {
    color: var(--color-text-secondary);
  }
  .wallet-selector:focus-visible {
    outline: 2px solid var(--color-text);
    outline-offset: 2px;
    border-radius: 4px;
  }
  .wallet-name {
    font-size: 14px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chevron {
    flex-shrink: 0;
    color: var(--color-text-secondary);
  }
  .wallet-address-row {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }
  .wallet-address {
    font-size: 12px;
    color: var(--color-text-secondary);
    font-family: monospace;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    justify-self: end;
    grid-column: 3;
  }
  .nock-price {
    display: flex;
    flex-direction: column;
    gap: 4px;
    text-align: right;
    white-space: nowrap;
  }
  .price-label {
    font-size: 11px;
    color: var(--color-text-secondary);
  }
  .price-value {
    font-size: 13px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--color-text);
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
  }

  .menu-btn:hover {
    color: var(--color-text);
  }

  .content {
    flex: 1;
    overflow: hidden;
    padding: 0 20px 20px;
    display: flex;
    flex-direction: column;
  }

  .loading-container {
    padding: 40px 20px;
    text-align: center;
  }

  .spinner {
    width: 40px;
    height: 40px;
    border: 3px solid rgba(79, 195, 247, 0.1);
    border-top-color: #4fc3f7;
    border-radius: 50%;
    animation: spin 1s linear infinite;
    margin: 0 auto 16px;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .error-container {
    padding: 30px 20px;
    text-align: center;
  }

  .error-message {
    color: var(--color-error);
    margin-bottom: 16px;
  }

  .retry-btn {
    background: var(--color-primary);
    color: white;
    border: none;
    border-radius: 8px;
    padding: 8px 20px;
    cursor: pointer;
    font-size: 14px;
  }

  .retry-btn:hover {
    opacity: 0.9;
  }

  .balance-display {
    text-align: center;
    margin-bottom: 32px;
    padding-top: 42px;
    flex-shrink: 0;
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

  .pending-indicator {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    font-size: 13px;
    color: #f59e0b;
    font-weight: 500;
    margin-top: 8px;
  }

  .pending-dot {
    width: 8px;
    height: 8px;
    background: #f59e0b;
    border-radius: 50%;
    animation: pulse 1.5s ease-in-out infinite;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.4;
    }
  }

  .quick-actions {
    display: flex;
    gap: 10px;
    margin-bottom: 24px;
    flex-shrink: 0;
  }

  .quick-action-btn {
    flex: 1;
    padding: 14px;
    border-radius: 10px;
    font-size: 15px;
    font-weight: 400;
    color: var(--color-text);
    cursor: pointer;
    transition: all 0.2s ease;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }

  /* Dark mode - action button surface, borderless */
  :global([data-theme='dark']) .quick-action-btn {
    background: rgba(255, 255, 255, 0.06);
    border: none;
  }

  /* Light mode - subtle tinted surface with border */
  :global([data-theme='light']) .quick-action-btn {
    background: rgba(0, 0, 0, 0.02);
    border: 1px solid var(--color-border);
  }

  .quick-action-btn svg {
    width: 18px;
    height: 18px;
  }

  /* Dark mode hover - lift effect */
  :global([data-theme='dark']) .quick-action-btn:hover {
    background: rgba(255, 255, 255, 0.09);
    transform: translateY(-1px);
  }

  /* Light mode hover - lift effect */
  :global([data-theme='light']) .quick-action-btn:hover {
    background: rgba(0, 0, 0, 0.04);
    transform: translateY(-1px);
  }

  .quick-action-btn:active {
    transform: translateY(0);
    transition: all 0.1s ease;
  }

  .tabs {
    display: flex;
    gap: 24px;
    border-bottom: 1px solid var(--color-surface);
    margin-bottom: 12px;
    flex-shrink: 0;
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

  .tab.disabled {
    color: var(--color-text-tertiary);
    opacity: 0.5;
    cursor: default;
  }

  .transaction-list {
    display: flex;
    flex-direction: column;
    flex: 1;
    overflow-y: auto;
    min-height: 0; /* Important for Firefox */
    background: var(--color-surface);
    border-radius: 10px;
    padding-right: 4px;
    position: relative;
  }

  .transaction-list::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background-image: url("data:image/svg+xml,%3Csvg viewBox='0 0 200 200' xmlns='http://www.w3.org/2000/svg'%3E%3Cfilter id='noiseFilter'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='4' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23noiseFilter)' opacity='0.15'/%3E%3C/svg%3E");
    border-radius: 10px;
    pointer-events: none;
  }

  /* Dark mode - make transaction list background darker */
  :global([data-theme='dark']) .transaction-list {
    background: rgba(255, 255, 255, 0.008);
  }

  :global([data-theme='dark-blue']) .transaction-list {
    background: rgba(255, 255, 255, 0.008);
  }

  :global([data-theme='purple']) .transaction-list {
    background: rgba(255, 255, 255, 0.008);
  }

  /* Dark mode - reduce noise contrast */
  :global([data-theme='dark']) .transaction-list::before {
    opacity: 0.3;
  }

  :global([data-theme='dark-blue']) .transaction-list::before {
    opacity: 0.3;
  }

  :global([data-theme='purple']) .transaction-list::before {
    opacity: 0.3;
  }

  .transaction-list-inner {
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--color-background);
    border-radius: 10px;
    margin-right: -4px;
    position: relative;
    z-index: 1;
  }

  .empty-state {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: 1;
    color: var(--color-text-tertiary);
    font-size: 14px;
    font-weight: 400;
    position: relative;
    z-index: 1;
  }

  .transaction-item {
    display: grid;
    grid-template-columns: minmax(0, 1fr) fit-content(52%);
    gap: 12px;
    align-items: center;
    text-align: left;
    padding: 12px 4px 12px 0;
    background: var(--color-background);
    border: none;
    border-bottom: 1px solid var(--color-surface);
    width: 100%;
    cursor: pointer;
    transition: opacity 0.2s ease;
  }

  .transaction-item:last-child {
    border-bottom: none;
  }

  .transaction-item:hover {
    opacity: 0.7;
  }

  .transaction-left {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .transaction-icon {
    flex-shrink: 0;
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-surface);
    color: var(--color-text-secondary);
  }

  .transaction-icon.sent {
    color: var(--color-text-secondary);
  }

  .transaction-icon.received {
    color: var(--color-text-secondary);
  }

  .transaction-details {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    align-items: flex-start;
  }

  .transaction-address {
    display: flex;
    flex-wrap: wrap;
    column-gap: 0.5ch;
    max-width: 100%;
    font-size: 14px;
    font-weight: 400;
    color: var(--color-text-secondary);
    font-family: 'SF Mono', 'Monaco', 'Inconsolata', 'Roboto Mono', monospace;
  }

  .counterparty {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .transaction-time {
    font-size: 12px;
    color: var(--color-text-tertiary);
    font-weight: 400;
    text-align: left;
  }

  .transaction-time.pending {
    color: #f59e0b;
    font-weight: 500;
  }

  .transaction-right {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .transaction-number {
    white-space: nowrap;
  }

  .transaction-amount {
    font-size: 15px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    color: var(--color-text);
  }

  .transaction-amount.sent {
    color: var(--color-text-secondary);
  }

  .transaction-amount.received {
    color: var(--color-text);
  }

  .transaction-usd {
    font-size: 12px;
    color: var(--color-text-tertiary);
    font-weight: 400;
  }
</style>
