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
  let importKind: 'mnemonic' | 'raw' | 'extended' = 'mnemonic';
  $: inputLabel =
    importKind === 'mnemonic'
      ? 'Seed phrase'
      : importKind === 'raw'
        ? 'Secret key · hex'
        : 'Extended private key';
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
      error = `Enter your ${inputLabel.toLowerCase()}.`;
      return;
    }
    if (importKind === 'mnemonic' && !validateSeedPhrase(seedPhrase)) {
      error = 'Enter exactly 24 recovery words.';
      return;
    }
    if (importKind === 'raw' && !/^(?:0x)?[a-fA-F0-9]{64}$/i.test(seedPhrase.trim())) {
      error = 'Enter a 64-character hexadecimal secret key.';
      return;
    }
    if (importKind === 'extended' && !seedPhrase.trim().startsWith('zprv')) {
      error = 'Enter an extended private key starting with zprv.';
      return;
    }

    isImporting = true;

    try {
      // Call the import function and wait for it to complete
      await onImport(seedPhrase.trim(), walletName.trim());
      seedPhrase = '';
      // If successful, the parent component will handle navigation
    } catch (err) {
      error =
        err instanceof Error
          ? err.message
          : 'Unable to import wallet. Check your recovery material.';
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
      <p class="import-subtitle">Restore a wallet with your recovery phrase or private key.</p>
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
        <label for="import-kind" class="form-label">Import with</label>
        <select
          id="import-kind"
          class="input-field"
          bind:value={importKind}
          on:change={() => {
            seedPhrase = '';
            error = '';
          }}
          disabled={isImporting}
        >
          <option value="mnemonic">24-word seed phrase</option>
          <option value="raw">Secret key · hex</option>
          <option value="extended">Extended private key</option>
        </select>
      </div>
      <div class="form-group">
        <label for="seed-phrase" class="form-label">{inputLabel}</label>
        <textarea
          id="seed-phrase"
          class="textarea-field"
          class:error
          placeholder={importKind === 'mnemonic'
            ? 'Enter your 24 words…'
            : importKind === 'raw'
              ? '64 hexadecimal characters'
              : 'zprv…'}
          spellcheck="false"
          autocomplete="off"
          autocapitalize="off"
          bind:value={seedPhrase}
          disabled={isImporting}
          rows="4"
        ></textarea>
        <p class="helper-text">
          {importKind === 'mnemonic'
            ? 'Enter the words separated by spaces.'
            : importKind === 'raw'
              ? 'A 32-byte private signing key. An optional 0x prefix is accepted. This wallet has no seed phrase.'
              : 'Paste your zprv extended private key.'}
        </p>
      </div>

      {#if error}
        <div class="error-message">{error}</div>
      {/if}
    </div>
  </div>

  <div class="button-footer">
    <Button
      variant="primary"
      fullWidth={true}
      on:click={handleImport}
      disabled={nameExists || isImporting}
    >
      {isImporting ? 'Importing...' : 'Import Wallet'}
    </Button>
  </div>
</div>

<style>
  .button-footer {
    position: static;
    flex-shrink: 0;
  }
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
    min-height: 0;
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
    color: var(--color-text);
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
