<script lang="ts">
  import { formatUsdEstimate } from '../../utils/usd';
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';
  import { nockPrice } from '../../stores/price';
  import { STANDARD_NETWORK_FEE } from '../../types/nicks';
  import {
    formatFeeInNicks,
    formatNocksNumberWithSeparator,
    formatNocksWithSeparator
  } from '../../utils/nicks';

  export let recipientName: string = 'Alice';
  export let recipientAddress: string = 'nock1qvk8x9y7...ht4k3n2p5m';
  export let amount: number = 50.0;
  export let fee: number = 0.02;
  export let onConfirm: () => void = () => {};
  export let onCancel: () => void = () => {};
  export let onBack: () => void = () => {};

  let isConfirming = false;

  $: total = amount + fee;

  function formatUSD(nockAmount: number): string {
    return formatUsdEstimate(nockAmount, $nockPrice) ?? '';
  }

  async function handleConfirm() {
    isConfirming = true;
    // Simulate transaction processing
    setTimeout(() => {
      isConfirming = false;
      onConfirm();
    }, 1500);
  }
</script>

<div class="confirm-transaction fixed-screen">
  <Header title="Confirm Transaction" showBack={true} on:click={onBack} />

  <div class="content no-scroll">
    <h2 class="review-title">Review Transaction</h2>

    <div class="card recipient-card">
      <div class="label">SENDING TO</div>
      <div class="recipient-name">{recipientName}</div>
      <div class="recipient-address">{recipientAddress}</div>
    </div>

    <div class="card amount-card">
      <div class="amount-display">
        <div class="amount-value">{formatNocksNumberWithSeparator(amount, 2)} NOCK</div>
        <div class="amount-usd">{formatUSD(amount)}</div>
        <div class="amount-label">Amount</div>
      </div>
    </div>

    <div class="fee-details">
      <div class="fee-row">
        <span class="fee-label">Net sent:</span>
        <div class="fee-value-group">
          <span class="fee-value">{formatNocksNumberWithSeparator(amount, 8)} NOCK</span>
          <span class="fee-value-usd">{formatUSD(amount)}</span>
        </div>
      </div>
      <div class="fee-row">
        <span class="fee-label">Total fees:</span>
        <div class="fee-value-group">
          <span class="fee-value">{formatFeeInNicks(STANDARD_NETWORK_FEE)}</span>
          <span class="fee-value-nock">
            ({formatNocksWithSeparator(STANDARD_NETWORK_FEE, 8)} NOCK)
          </span>
        </div>
      </div>
      <div class="fee-row total">
        <span class="fee-label">Total:</span>
        <div class="fee-value-group">
          <span class="fee-value">{formatNocksNumberWithSeparator(total, 8)} NOCK</span>
          <span class="fee-value-usd">{formatUSD(total)}</span>
        </div>
      </div>
    </div>

    <div class="warning-box">
      <span class="warning-icon">⚠️</span>
      <p class="warning-text">This transaction cannot be reversed once sent.</p>
    </div>
  </div>

  <div class="button-footer">
    <Button variant="primary" fullWidth={true} on:click={handleConfirm} disabled={isConfirming}>
      {isConfirming ? 'Processing...' : 'Confirm & Send'}
    </Button>

    <Button variant="secondary" fullWidth={true} on:click={onCancel} disabled={isConfirming}>
      Cancel
    </Button>
  </div>
</div>

<style>
  .confirm-transaction {
    height: 100%;
    background: var(--color-background);
    display: flex;
    flex-direction: column;
  }

  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 16px;
    overflow: hidden;
    gap: 8px;
  }

  .content.no-scroll {
    overflow: hidden;
  }

  .review-title {
    font-size: 14px;
    font-weight: 600;
    text-align: center;
    margin-bottom: 4px;
  }

  .card {
    background: var(--color-surface);
    border: 1px solid var(--color-border) 494;
    border-radius: 8px;
    padding: 10px;
    flex-shrink: 0;
  }

  .recipient-card {
    background: var(--color-surface);
  }

  .label {
    font-size: 14px;
    color: var(--color-text-secondary);
    margin-bottom: 8px;
  }

  .recipient-name {
    font-size: 14px;
    font-weight: 600;
    margin-bottom: 4px;
  }

  .recipient-address {
    font-size: 14px;
    color: var(--color-text-secondary);
  }

  .amount-card {
    background: var(--color-background);
  }

  .amount-display {
    text-align: center;
    padding: 8px 0;
  }

  .amount-value {
    font-size: 24px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 2px;
  }

  .amount-usd {
    font-size: 16px;
    color: var(--color-text-secondary);
    font-weight: 400;
    margin-bottom: 4px;
  }

  .amount-label {
    font-size: 12px;
    color: var(--color-text-tertiary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .fee-details {
    background: var(--color-surface);
    border: 1px solid var(--color-border) 1472;
    border-radius: 8px;
    padding: 10px;
    flex-shrink: 0;
  }

  .fee-row {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    font-size: 14px;
    padding: 8px 0;
    border-bottom: 1px solid var(--color-border) 1742;
  }

  .fee-row:last-child {
    border-bottom: none;
  }

  .fee-row.total {
    font-weight: 600;
    margin-top: 8px;
    padding-top: 12px;
    border-top: 2px solid var(--color-border) 1924;
    border-bottom: none;
  }

  .fee-label {
    color: var(--color-text-secondary);
    font-size: 13px;
  }

  .fee-value-group {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
  }

  .fee-value {
    color: var(--color-text);
    font-size: 14px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }

  .fee-value-nock {
    color: var(--color-text-tertiary);
    font-size: 11px;
    font-weight: 400;
  }

  .fee-value-usd {
    color: var(--color-text-secondary);
    font-size: 12px;
    font-weight: 400;
  }

  .fee-row.total .fee-label,
  .fee-row.total .fee-value {
    color: var(--color-text);
    font-weight: 600;
  }

  .warning-box {
    display: flex;
    gap: 6px;
    padding: 8px;
    background: #fef3c7;
    border: 1px solid #fde68a;
    border-radius: 6px;
    margin-top: auto;
    margin-bottom: 8px;
    flex-shrink: 0;
  }

  .warning-icon {
    font-size: 16px;
    flex-shrink: 0;
  }

  .warning-text {
    font-size: 14px;
    color: #92400e;
    margin: 0;
  }
</style>
