<script lang="ts">
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';

  export let onImport: (address: string, walletName: string) => Promise<void> | void = () => {};
  export let onBack: () => void = () => {};

  let address = '';
  let walletName = '';
  let error = '';
  let isImporting = false;

  function validateAddress(addr: string): boolean {
    // Basic validation - address should be a non-empty string
    // You can add more specific validation for Nockchain address format
    return addr.trim().length > 0;
  }

  async function handleImport() {
    error = '';

    if (!address.trim()) {
      error = 'Please enter an address to watch';
      return;
    }

    if (!validateAddress(address)) {
      error = 'Invalid address format';
      return;
    }

    isImporting = true;

    try {
      await onImport(address.trim(), walletName.trim() || 'Watch-Only Wallet');
    } catch (err) {
      console.error('Import error:', err);
      error = err instanceof Error ? err.message : 'Failed to import address. Please try again.';
    } finally {
      isImporting = false;
    }
  }
</script>

<div class="import-watch-only">
  <Header title="Watch Address" showBack={true} on:click={onBack} />

  <div class="content">
    <div class="intro">
      <h2>Watch a Public Address</h2>
      <p>
        Enter any Nockchain address to view its balance and transaction history. You won't be able
        to send funds from this address.
      </p>
    </div>

    {#if error}
      <div class="error-message">{error}</div>
    {/if}

    <div class="form">
      <div class="form-group">
        <label for="address">Address</label>
        <textarea
          id="address"
          bind:value={address}
          placeholder="Enter Nockchain address..."
          rows="3"
          disabled={isImporting}
        ></textarea>
      </div>

      <div class="form-group">
        <label for="walletName">Wallet Name (Optional)</label>
        <input
          id="walletName"
          type="text"
          bind:value={walletName}
          placeholder="e.g., Exchange Wallet"
          disabled={isImporting}
        />
      </div>

      <Button variant="primary" disabled={isImporting} fullWidth={true} on:click={handleImport}>
        {isImporting ? 'Importing...' : 'Watch Address'}
      </Button>
    </div>
  </div>
</div>

<style>
  .import-watch-only {
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
    gap: 24px;
  }

  .intro h2 {
    font-size: 20px;
    font-weight: 600;
    color: var(--color-text);
    margin: 0 0 8px 0;
  }

  .intro p {
    font-size: 14px;
    color: var(--color-text-secondary);
    line-height: 1.5;
    margin: 0;
  }

  .error-message {
    padding: 12px 16px;
    background: rgba(220, 38, 38, 0.1);
    border: 1px solid rgba(220, 38, 38, 0.3);
    border-radius: 8px;
    color: #dc2626;
    font-size: 14px;
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .form-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  label {
    font-size: 14px;
    font-weight: 500;
    color: var(--color-text);
  }

  textarea,
  input {
    padding: 12px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-surface);
    color: var(--color-text);
    font-size: 14px;
    font-family: 'SF Mono', 'Monaco', 'Inconsolata', 'Roboto Mono', monospace;
    outline: none;
    transition: border-color 0.15s ease;
  }

  textarea {
    resize: vertical;
    min-height: 80px;
  }

  textarea:focus,
  input:focus {
    border-color: var(--color-text);
  }

  textarea:disabled,
  input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  textarea::placeholder,
  input::placeholder {
    color: var(--color-text-tertiary);
  }
</style>
