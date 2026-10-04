<script lang="ts">
  import { get } from 'svelte/store';
  import { walletStore } from '../../stores/wallet';
  import { nextWalletName } from '../../utils/walletName';
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';

  export let onImport: (seedPhrase: string, walletName: string) => Promise<void> | void = () => {};
  export let onBack: () => void = () => {};
  export let showHeader: boolean = true;

  let seedPhrase = '';
  let walletName = nextWalletName(get(walletStore).wallets.map(wallet => wallet.name));
  $: nameExists = $walletStore.wallets.some(wallet => wallet.name === walletName.trim());
  let error = '';
  let isImporting = false;

  function validateSeedPhrase(phrase: string): boolean {
    const words = phrase.trim().split(/\s+/);
    return words.length === 24 && words.every(word => word.length > 0);
  }

  async function handleImport() {
    error = '';
    if (nameExists) return;

    if (!seedPhrase.trim()) {
      error = 'Please enter your seed phrase';
      return;
    }

    if (!validateSeedPhrase(seedPhrase)) {
      error = 'Invalid seed phrase. Please enter exactly 24 words';
      return;
    }

    isImporting = true;

    try {
      // Call the import function and wait for it to complete
      await onImport(seedPhrase, walletName.trim());
      // If successful, the parent component will handle navigation
    } catch (err) {
      // Handle import errors
      console.error('Import error:', err);
      const message = (err instanceof Error ? err.message : String(err)).toLowerCase();
      const errorMessage =
        message.includes('invalid') || message.includes('seed') || message.includes('mnemonic')
          ? 'Invalid seed phrase. Please check your words and try again.'
          : 'Failed to import wallet. Please try again.';
      error = errorMessage;
    } finally {
      isImporting = false;
    }
  }
</script>

<div class="import-wallet fixed-screen">
  {#if showHeader}
    <Header title="" showBack={true} showLogo={false} on:click={onBack} />
  {/if}

  <div class="import-content">
    <div class="import-header">
      <h2 class="import-title">Import Wallet</h2>
      <p class="import-subtitle">Enter your 24 word seed phrase to access your existing wallet</p>
    </div>

    <div class="import-form">
      <div class="form-group">
        <label for="wallet-name" class="form-label">Wallet Name</label>
        <input
          id="wallet-name"
          type="text"
          class="input-field"
          placeholder="e.g., Main Wallet"
          bind:value={walletName}
          class:duplicate-name={nameExists}
          aria-invalid={nameExists}
          disabled={isImporting}
          maxlength="20"
        />
        {#if nameExists}
        <p class="duplicate-name" role="status">A wallet with this name already exists</p>
      {/if}
      </div>

      <div class="form-group">
        <label for="seed-phrase" class="form-label">Seed Phrase</label>
        <textarea
          id="seed-phrase"
          class="textarea-field"
          class:error
          placeholder="Enter your 24-word seed phrase..."
          bind:value={seedPhrase}
          disabled={isImporting}
          rows="4"
        ></textarea>
        <p class="helper-text">Enter the words separated by spaces</p>
      </div>

      {#if error}
        <div class="error-message">{error}</div>
      {/if}
    </div>
  </div>

  <div class="button-footer">
    <Button variant="primary" fullWidth={true} on:click={handleImport} disabled={nameExists || isImporting}>
      {isImporting ? 'Importing...' : 'Import Wallet'}
    </Button>
  </div>
</div>

<style>
  .input-field.duplicate-name,
  .duplicate-name {
    color: var(--color-error);
    border-color: var(--color-error);
  }
  .import-wallet {
    height: 100%;
    background: var(--color-background);
    display: flex;
    flex-direction: column;
    position: relative;
  }

  .import-content {
    flex: 1;
    padding: 20px 16px 16px;
    overflow-y: auto;
  }

  .import-header {
    text-align: center;
    margin-bottom: 36px;
  }

  .import-title {
    font-size: 20px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 8px;
  }

  .import-subtitle {
    font-size: 14px;
    color: var(--color-text-secondary);
    line-height: 1.4;
  }

  .import-form {
    max-width: 400px;
    margin: 0 auto;
  }

  .form-group {
    margin-bottom: 24px;
  }

  .form-label {
    display: block;
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 8px;
  }

  .input-field,
  .textarea-field {
    width: 100%;
    padding: 14px 16px;
    border: 1.5px solid transparent;
    border-radius: 10px;
    background: var(--color-surface);
    font-size: 15px;
    outline: none;
    transition: all 0.15s ease;
  }

  .textarea-field {
    resize: vertical;
    min-height: 100px;
    font-family: inherit;
    line-height: 1.5;
  }

  .input-field:focus,
  .textarea-field:focus {
    border-color: var(--color-border);
    background: var(--color-background);
  }

  .textarea-field.error {
    border-color: var(--color-error);
  }

  .input-field:disabled,
  .textarea-field:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .helper-text {
    font-size: 13px;
    color: var(--color-text-tertiary);
    margin-top: 6px;
  }

  .error-message {
    background: var(--color-error-light);
    border: 1px solid var(--color-error);
    color: var(--color-error);
    padding: 8px 12px;
    border-radius: 6px;
    font-size: 14px;
    margin-bottom: 16px;
  }
</style>
