<script lang="ts">
  import Header from '../molecules/Header.svelte';

  export let type: 'sent' | 'received' | 'consolidation' = 'sent';
  export let amount: number = 50.00;
  export let fee: number = 0.02;
  export let from: string = 'nc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq';
  export let to: string = 'nc1q5xvftzgd8a4x8f6g3k4jw5a2j4k5j6k5j6k5j6';
  export let txHash: string = '7f83b1657ff1fc53b92dc18148a1d65dfc2d4b1fa3d677284addd200126d9069';
  export let timestamp: string = '2024-01-15 14:32:05';
  export let confirmations: number = 12;
  export let blockHeight: number = 823456;
  export let gasUsed: number = 21000;
  export let gasLimit: number = 30000;
  export let status: 'pending' | 'confirmed' | 'failed' = 'confirmed';
  export let onBack: () => void = () => {};

  // Mock exchange rate - in production this would come from an API
  const NOCK_TO_USD = 8.45;

  let copiedField: string | null = null;

  function formatUSD(nockAmount: number): string {
    const usdAmount = nockAmount * NOCK_TO_USD;
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: 'USD',
      minimumFractionDigits: 2,
      maximumFractionDigits: 2
    }).format(usdAmount);
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

  function openExplorer() {
    // In production, this would open the actual block explorer
    console.log(`Opening explorer for tx: ${txHash}`);
    // window.open(`https://explorer.nockchain.com/tx/${txHash}`, '_blank');
  }

  $: statusColor = status === 'confirmed' ? '#059669' :
                   status === 'pending' ? '#f59e0b' : '#dc2626';

  $: statusText = status === 'confirmed' ? 'Confirmed' :
                  status === 'pending' ? 'Pending' : 'Failed';
</script>

<div class="transaction-details fixed-screen">
  <Header title="Transaction Details" showBack={true} on:click={onBack} />

  <div class="content">
    <!-- Status Section -->
    <div class="status-section">
      <div class="status-badge" style="background-color: {statusColor}10; color: {statusColor}">
        <span class="status-dot" style="background-color: {statusColor}"></span>
        {statusText}
      </div>
      <div class="transaction-type">
        {type === 'sent' ? 'Sent' : type === 'received' ? 'Received' : 'Consolidation'}
      </div>
    </div>

    <!-- Amount Section -->
    <div class="amount-section">
      <div class="amount-value">
        {type === 'sent' ? '-' : type === 'received' ? '+' : ''}{amount.toFixed(8)} NOCK
      </div>
      <div class="amount-usd">{formatUSD(amount)}</div>
    </div>

    <!-- Details Cards -->
    <div class="details-cards">
      <!-- From/To Card -->
      <div class="detail-card">
        <div class="detail-row">
          <span class="detail-label">From</span>
          <div class="detail-value-group">
            <span class="detail-value mono">{truncateAddress(from)}</span>
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
            <span class="detail-value mono">{truncateAddress(to)}</span>
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
            <span class="detail-value mono small">{truncateAddress(txHash)}</span>
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

        <div class="detail-row">
          <span class="detail-label">Confirmations</span>
          <span class="detail-value">
            {#if status === 'confirmed'}
              <span class="confirmations-badge">
                ✓ {confirmations.toLocaleString()} confirmations
              </span>
            {:else if status === 'pending'}
              <span class="pending-badge">Waiting for confirmation...</span>
            {:else}
              <span class="failed-badge">Transaction failed</span>
            {/if}
          </span>
        </div>
      </div>

      <!-- Fee & Gas Card -->
      <div class="detail-card">
        <div class="detail-row">
          <span class="detail-label">Network Fee</span>
          <div class="fee-values">
            <span class="detail-value">{fee.toFixed(4)} NOCK</span>
            <span class="detail-value-usd">{formatUSD(fee)}</span>
          </div>
        </div>

        <div class="detail-row">
          <span class="detail-label">Gas Used</span>
          <span class="detail-value">{gasUsed.toLocaleString()} / {gasLimit.toLocaleString()}</span>
        </div>

        <div class="detail-row">
          <span class="detail-label">Total</span>
          <div class="fee-values">
            <span class="detail-value bold">{(amount + fee).toFixed(8)} NOCK</span>
            <span class="detail-value-usd">{formatUSD(amount + fee)}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Explorer Link -->
    <button class="explorer-link" on:click={openExplorer}>
      <span>View in Block Explorer</span>
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
        <path d="M6 3L11 8L6 13" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
    </button>
  </div>
</div>

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
    justify-content: space-between;
    align-items: center;
  }

  .status-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 20px;
    font-size: 13px;
    font-weight: 500;
  }

  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }

  .transaction-type {
    font-size: 14px;
    color: var(--color-text-secondary);
  }

  .amount-section {
    text-align: center;
    padding: 24px 0;
    border-bottom: 1px solid var(--color-surface)790;
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
    border: 1px solid var(--color-border)1269;
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