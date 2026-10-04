<script lang="ts">
  import Header from '../molecules/Header.svelte';

  export let balance: number = 1234.56;
  export let onSend: (data: any) => void = () => {};
  export let onCancel: () => void = () => {};
  
  let recipient = '';
  let amount = '';
  let selectedFee = 'standard';
  let sendAll = false;

  // Mock exchange rate - in production this would come from an API
  const NOCK_TO_USD = 8.45;

  const feeOptions = {
    economy: { label: 'Economy', amount: 0.01 },
    standard: { label: 'Standard', amount: 0.02 },
    fast: { label: 'Fast', amount: 0.05 }
  };

  function formatUSD(nockAmount: number): string {
    const usdAmount = nockAmount * NOCK_TO_USD;
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: 'USD',
      minimumFractionDigits: 2,
      maximumFractionDigits: 2
    }).format(usdAmount);
  }
  
  function setSendAll() {
    sendAll = true;
    amount = balance.toFixed(8);
  }

  function formatNumber(value: string): string {
    const num = parseFloat(value) || 0;
    return new Intl.NumberFormat('en-US', {
      minimumFractionDigits: 2,
      maximumFractionDigits: 8
    }).format(num);
  }
  
  function getTotalAmount() {
    const amountValue = parseFloat(amount) || 0;
    const fee = feeOptions[selectedFee].amount;
    return {
      amount: amountValue,
      fee: fee,
      total: amountValue + fee
    };
  }

  function handleAmountInput(e: Event) {
    const input = e.target as HTMLInputElement;
    const value = input.value.replace(/,/g, '');
    if (!isNaN(parseFloat(value))) {
      amount = value;
    }
  }
</script>

<div class="send-transaction fixed-screen">
  <Header title="Send" showBack={true} on:click={onCancel} />
  
  <div class="content">
    <div class="input-group">
      <label for="recipient" class="input-label">Send to</label>
      <div class="input-wrapper">
        <input
          id="recipient"
          type="text"
          class="input-field"
          placeholder="Enter address or select contact"
          bind:value={recipient}
        />
        <button class="input-action" title="Address Book">
          <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
            <path d="M9 9C10.6569 9 12 7.65685 12 6C12 4.34315 10.6569 3 9 3C7.34315 3 6 4.34315 6 6C6 7.65685 7.34315 9 9 9Z" stroke="currentColor" stroke-width="1.5"/>
            <path d="M3 15C3 12.7909 5.23858 11 9 11C12.7614 11 15 12.7909 15 15" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
          </svg>
        </button>
      </div>
    </div>

    <div class="input-group">
      <div class="label-row">
        <label for="amount" class="input-label">Amount</label>
        <button class="send-all-btn" on:click={setSendAll}>
          Send All
        </button>
      </div>
      <div class="input-wrapper">
        <input
          id="amount"
          type="text"
          class="input-field"
          placeholder="0.00"
          bind:value={amount}
          on:input={handleAmountInput}
        />
        <span class="input-suffix">NOCK</span>
      </div>
      <div class="balance-hint">Available: {balance.toFixed(8)} NOCK</div>
    </div>
    
    <div class="fee-section">
      <label class="input-label">Network Fee</label>
      <div class="fee-selector">
        {#each Object.entries(feeOptions) as [key, option]}
          <button
            class="fee-option"
            class:selected={selectedFee === key}
            class:recommended={key === 'standard'}
            on:click={() => selectedFee = key}
          >
            <div class="fee-option-content">
              <div class="fee-option-label">
                {option.label}
                {#if key === 'standard'}
                  <span class="recommended-badge">Recommended</span>
                {/if}
              </div>
              <div class="fee-option-amount">{option.amount} NOCK</div>
            </div>
          </button>
        {/each}
      </div>
    </div>
    
    <div class="summary-card">
      <div class="summary-row">
        <span class="summary-label">Amount</span>
        <span class="summary-value">{formatNumber(getTotalAmount().amount.toString())}</span>
      </div>
      <div class="summary-row">
        <span class="summary-label">Network fee</span>
        <span class="summary-value">{getTotalAmount().fee.toFixed(4)}</span>
      </div>
      <div class="summary-row total">
        <span class="summary-label-total">Total</span>
        <span class="summary-value-total">{formatNumber(getTotalAmount().total.toString())}</span>
      </div>
    </div>
  </div>

  <div class="button-footer">
    <button
      class="btn-primary"
      on:click={() => onSend({ recipient, amount, fee: selectedFee, ...getTotalAmount() })}
      disabled={!recipient || !amount || parseFloat(amount) <= 0}
    >
      Review Transaction
    </button>
  </div>
</div>

<style>
  .send-transaction {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-background);
  }

  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 20px;
    overflow: hidden;
    gap: 20px;
  }

  .input-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .input-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .label-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .send-all-btn {
    font-size: 12px;
    font-weight: 500;
    color: #3b82f6;
    background: none;
    border: none;
    cursor: pointer;
    padding: 0;
    transition: color 0.15s ease;
  }

  .send-all-btn:hover {
    color: #2563eb;
    text-decoration: underline;
  }

  .input-wrapper {
    position: relative;
    display: flex;
    align-items: center;
  }

  .input-field {
    width: 100%;
    padding: 14px 16px;
    padding-right: 60px;
    border: 1.5px solid var(--color-border)1072;
    border-radius: 10px;
    background: var(--color-background);
    font-size: 16px;
    color: var(--color-text);
    outline: none;
    transition: all 0.15s ease;
  }

  .input-field:focus {
    border-color: var(--color-text);
  }

  .input-field::placeholder {
    color: var(--color-text-tertiary);
  }

  .input-action {
    position: absolute;
    right: 12px;
    padding: 6px;
    background: none;
    border: none;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
    border-radius: 6px;
  }

  .input-action:hover {
    background: var(--color-surface);
    color: var(--color-text);
  }

  .input-suffix {
    position: absolute;
    right: 16px;
    font-size: 14px;
    font-weight: 500;
    color: var(--color-text-secondary);
    pointer-events: none;
  }

  .balance-hint {
    font-size: 12px;
    color: var(--color-text-tertiary);
  }

  .fee-section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .fee-selector {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }

  .fee-option {
    padding: 10px;
    background: var(--color-background);
    border: 1.5px solid var(--color-border)2230;
    border-radius: 10px;
    cursor: pointer;
    transition: all 0.15s ease;
    position: relative;
  }

  .fee-option:hover {
    border-color: var(--color-text-tertiary);
  }

  .fee-option.recommended {
    border-color: var(--color-border);
  }

  .fee-option.selected {
    border-color: var(--color-text);
    background: var(--color-background);
  }

  .fee-option-content {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .fee-option-label {
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text-secondary);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .recommended-badge {
    font-size: 10px;
    color: #3b82f6;
    font-weight: 400;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .fee-option-amount {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text);
  }

  .summary-card {
    background: var(--color-surface);
    border: 1px solid var(--color-border)3133;
    border-radius: 10px;
    padding: 14px;
    margin-top: auto;
  }

  .summary-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 5px 0;
  }

  .summary-row.total {
    border-top: 1px solid var(--color-border)3428;
    margin-top: 6px;
    padding-top: 10px;
  }

  .summary-label {
    font-size: 13px;
    color: var(--color-text-secondary);
  }

  .summary-value {
    font-size: 14px;
    color: var(--color-text);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }

  .summary-label-total,
  .summary-value-total {
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text);
  }

  .btn-primary {
    background: var(--color-text);
    color: var(--color-background);
    border: none;
    border-radius: 10px;
    padding: 14px 20px;
    font-size: 15px;
    font-weight: 500;
    cursor: pointer;
    width: 100%;
    transition: all 0.15s ease;
  }

  .btn-primary:hover:not(:disabled) {
    background: #171717;
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>