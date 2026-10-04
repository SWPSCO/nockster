<script lang="ts">
  import { formatUsdEstimate } from '../../utils/usd';
  import Header from '../molecules/Header.svelte';
  import Icon from '../atoms/Icon.svelte';
  import Button from '../atoms/Button.svelte';
  import { onMount } from 'svelte';
  import { formatNocksNumberWithSeparator } from '../../utils/nicks';
  import { nockPrice } from '../../stores/price';
  import QRCode from 'qrcode';

  export let address: string = '';
  export let onBack: () => void = () => {};
  export let showHeader: boolean = true;
  export const requestAmount: number = 0;

  let copied = false;
  let qrCodeUrl = '';
  let amountInput = '';

  onMount(() => {
    updateQRCode();
  });

  async function updateQRCode() {
    const amount = parseFloat(amountInput) || 0;
    const qrData = amount > 0 ? `${address}?amount=${amount}` : address;

    try {
      // Generate QR code locally
      qrCodeUrl = await QRCode.toDataURL(qrData, {
        width: 200,
        margin: 1,
        color: {
          dark: '#000000',
          light: '#FFFFFF'
        }
      });
    } catch (error) {
      console.error('Failed to generate QR code:', error);
    }
  }

  function formatUSD(nockAmount: number): string {
    return formatUsdEstimate(nockAmount, $nockPrice) ?? '';
  }

  function handleAmountInput(e: Event) {
    const input = e.target as HTMLInputElement;
    const value = input.value.replace(/,/g, '');
    if (!isNaN(parseFloat(value)) || value === '') {
      amountInput = value;
      updateQRCode(); // This is async but we don't need to await
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
          Requesting {formatNocksNumberWithSeparator(parseFloat(amountInput), 8)} NOCK
        </div>
        <div class="amount-usd">{formatUSD(parseFloat(amountInput))}</div>
      {/if}
    </div>

    <div class="address-section">
      <div class="address-heading">
        <div class="address-label">Your Address</div>
        {#if address}
          <a
            class="explorer-link"
            href={`https://nockblocks.com/address/${encodeURIComponent(address)}`}
            target="_blank"
            rel="noopener noreferrer"
            aria-label="View address on Nockblocks"
            title="View address on Nockblocks"><Icon name="externalLink" size="small" /></a
          >
        {/if}
      </div>
      <div class="address-text">{address}</div>
      <div class="copy-feedback" role="status">{copied ? 'Address copied' : ''}</div>
    </div>
  </div>

  <div class="button-footer">
    <div class="action-buttons">
      <Button variant="secondary" fullWidth={true} on:click={shareAddress}>Share Address</Button>

      <Button variant="primary" fullWidth={true} on:click={copyAddress}>
        {copied ? 'Copied!' : 'Copy Address'}
      </Button>
    </div>
  </div>
</div>

<style>
  .address-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  .explorer-link {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    color: var(--color-text-secondary);
    border-radius: 6px;
  }
  .explorer-link:hover {
    color: var(--color-text);
    background: var(--color-surface);
  }
  .explorer-link:focus-visible {
    outline: 2px solid var(--color-text);
    outline-offset: 2px;
  }
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
    border: 1.5px solid var(--color-border) 957;
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
    width: 200px;
    height: 200px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 4px;
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
    background: white;
    padding: 8px;
    border-radius: 8px;
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
  }

  .address-text {
    flex: 1;
    font-family: monospace;
    font-size: 14px;
    color: var(--color-text);
    word-break: break-all;
    line-height: 1.5;
  }

  .copy-feedback {
    min-height: 21px;
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
</style>
