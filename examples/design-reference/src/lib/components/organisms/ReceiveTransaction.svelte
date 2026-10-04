<script lang="ts">
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';
  import { onMount } from 'svelte';

  export let address: string = 'nc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq';
  export let onBack: () => void = () => {};
  export let showHeader: boolean = true;
  export let requestAmount: number = 0;

  // Mock exchange rate - in production this would come from an API
  const NOCK_TO_USD = 8.45;

  let copied = false;
  let qrCodeUrl = '';
  let amountInput = '';

  onMount(() => {
    updateQRCode();
  });

  function updateQRCode() {
    const amount = parseFloat(amountInput) || 0;
    const qrData = amount > 0 ? `${address}?amount=${amount}` : address;
    qrCodeUrl = `https://api.qrserver.com/v1/create-qr-code/?size=200x200&data=${encodeURIComponent(qrData)}`;
  }

  function formatUSD(nockAmount: number): string {
    const usdAmount = nockAmount * NOCK_TO_USD;
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: 'USD',
      minimumFractionDigits: 2,
      maximumFractionDigits: 2
    }).format(usdAmount);
  }

  function handleAmountInput(e: Event) {
    const input = e.target as HTMLInputElement;
    const value = input.value.replace(/,/g, '');
    if (!isNaN(parseFloat(value)) || value === '') {
      amountInput = value;
      updateQRCode();
    }
  }

  function copyAddress() {
    const amount = parseFloat(amountInput) || 0;
    const textToCopy = amount > 0 ? `${address}?amount=${amount}` : address;
    navigator.clipboard.writeText(textToCopy).then(() => {
      copied = true;
      setTimeout(() => {
        copied = false;
      }, 2000);
    });
  }

  function shareAddress() {
    const amount = parseFloat(amountInput) || 0;
    const textToShare = amount > 0 ? `${address}?amount=${amount}` : address;
    if (navigator.share) {
      navigator.share({
        title: 'My Nockchain Address',
        text: textToShare
      });
    } else {
      copyAddress();
    }
  }
</script>

<div class="receive-transaction fixed-screen">
  {#if showHeader}
    <Header title="Receive" showBack={true} on:click={onBack} />
  {/if}

  <div class="receive-content">
    <div class="receive-header">
      <h2 class="receive-title">Receive Nockchain</h2>
      <p class="receive-subtitle">Share your address to receive NOCK</p>
    </div>

    <div class="amount-section">
      <label class="amount-label">Request Amount (Optional)</label>
      <div class="amount-input-wrapper">
        <input
          type="text"
          class="amount-input"
          placeholder="0.00"
          bind:value={amountInput}
          on:input={handleAmountInput}
        />
        <span class="amount-suffix">NOCK</span>
      </div>
      {#if parseFloat(amountInput) > 0}
        <div class="amount-usd">{formatUSD(parseFloat(amountInput))}</div>
      {/if}
    </div>

    <div class="qr-container">
      <div class="qr-code">
        {#if qrCodeUrl}
          <img src={qrCodeUrl} alt="QR Code" />
        {:else}
          <div class="qr-placeholder">Loading QR Code...</div>
        {/if}
      </div>
      {#if parseFloat(amountInput) > 0}
        <div class="qr-amount-label">
          Requesting {parseFloat(amountInput).toFixed(8)} NOCK
        </div>
      {/if}
    </div>

    <div class="address-section">
      <label class="address-label">Your Address</label>
      <div class="address-container">
        <div class="address-text">{address}</div>
        <button class="copy-btn" on:click={copyAddress} title="Copy address">
          {#if copied}
            ✓
          {:else}
            📋
          {/if}
        </button>
      </div>
      {#if copied}
        <div class="copy-feedback">Address copied to clipboard!</div>
      {/if}
    </div>

    <div class="info-section">
      <p class="info-text">
        <strong>Note:</strong> Only send Nockchain to this address. Sending other cryptocurrencies may result in permanent loss.
      </p>
    </div>
  </div>

  <div class="button-footer">
    <div class="action-buttons">
      <Button
        variant="secondary"
        fullWidth={true}
        on:click={shareAddress}
      >
        Share Address
      </Button>

      <Button
        variant="primary"
        fullWidth={true}
        on:click={copyAddress}
      >
        {copied ? 'Copied!' : 'Copy Address'}
      </Button>
    </div>
  </div>
</div>

<style>
  .receive-transaction {
    height: 100%;
    background: var(--color-background);
    display: flex;
    flex-direction: column;
  }

  .receive-content {
    flex: 1;
    padding: 20px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .receive-header {
    text-align: center;
    margin-bottom: 24px;
  }

  .receive-title {
    font-size: 18px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 4px;
  }

  .receive-subtitle {
    font-size: 14px;
    color: var(--color-text-secondary);
  }

  .amount-section {
    margin-bottom: 20px;
  }

  .amount-label {
    display: block;
    font-size: 12px;
    font-weight: 500;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 8px;
  }

  .amount-input-wrapper {
    position: relative;
    display: flex;
    align-items: center;
  }

  .amount-input {
    width: 100%;
    padding: 14px 16px;
    padding-right: 60px;
    border: 1.5px solid var(--color-border)957;
    border-radius: 10px;
    background: var(--color-background);
    font-size: 16px;
    color: var(--color-text);
    outline: none;
    transition: all 0.15s ease;
  }

  .amount-input:focus {
    border-color: var(--color-text);
  }

  .amount-input::placeholder {
    color: var(--color-text-tertiary);
  }

  .amount-suffix {
    position: absolute;
    right: 16px;
    font-size: 14px;
    font-weight: 500;
    color: var(--color-text-secondary);
    pointer-events: none;
  }

  .amount-usd {
    font-size: 13px;
    color: var(--color-text-secondary);
    margin-top: 6px;
    padding-left: 2px;
  }

  .qr-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    margin-bottom: 20px;
    gap: 8px;
  }

  .qr-code {
    width: 180px;
    height: 180px;
    background: white;
    border: 1.5px solid var(--color-border)1785;
    border-radius: 12px;
    padding: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .qr-amount-label {
    font-size: 12px;
    color: var(--color-text-secondary);
    text-align: center;
  }

  .qr-code img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .qr-placeholder {
    color: var(--color-text-secondary);
    font-size: 14px;
  }

  .address-section {
    margin-bottom: 24px;
  }

  .address-label {
    display: block;
    font-size: 14px;
    font-weight: 500;
    color: var(--color-text);
    margin-bottom: 8px;
  }

  .address-container {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--color-surface);
    border: 1px solid var(--color-border)2494;
    border-radius: var(--radius-md, 8px);
    padding: 16px;
  }

  .address-text {
    flex: 1;
    font-family: monospace;
    font-size: 14px;
    color: var(--color-text);
    word-break: break-all;
    line-height: 1.5;
  }

  .copy-btn {
    background: none;
    border: none;
    font-size: 18px;
    cursor: pointer;
    padding: 4px;
    opacity: 0.7;
    transition: opacity 0.2s;
  }

  .copy-btn:hover {
    opacity: 1;
  }

  .copy-feedback {
    margin-top: 8px;
    font-size: 14px;
    color: var(--color-success);
    text-align: center;
  }

  .action-buttons {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .info-section {
    margin-top: auto;
    margin-bottom: 16px;
    padding-top: 16px;
    border-top: 1px solid var(--color-border)3290;
  }

  .info-text {
    font-size: 14px;
    color: var(--color-text-secondary);
    line-height: 1.5;
    text-align: center;
  }

  .info-text strong {
    color: var(--color-text);
  }
</style>