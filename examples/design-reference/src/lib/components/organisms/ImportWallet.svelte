<script lang="ts">
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';
  import ProgressBar from '../atoms/ProgressBar.svelte';
  
  export let onImport: () => void = () => {};
  export let onBack: () => void = () => {};
  export let showHeader: boolean = true;
  
  let seedPhrase = '';
  let walletName = '';
  let error = '';
  let isImporting = false;
  
  function validateSeedPhrase(phrase: string): boolean {
    const words = phrase.trim().split(/\s+/);
    return words.length === 12 && words.every(word => word.length > 0);
  }
  
  async function handleImport() {
    error = '';
    
    if (!walletName.trim()) {
      error = 'Please enter a wallet name';
      return;
    }
    
    if (!seedPhrase.trim()) {
      error = 'Please enter your seed phrase';
      return;
    }
    
    if (!validateSeedPhrase(seedPhrase)) {
      error = 'Invalid seed phrase. Please enter exactly 12 words';
      return;
    }
    
    isImporting = true;
    
    // Simulate import process
    setTimeout(() => {
      isImporting = false;
      onImport();
    }, 1000);
  }
</script>

<div class="import-wallet fixed-screen">
  {#if showHeader}
    <Header title="" showBack={true} showLogo={false} on:click={onBack} />
    <div class="progress-wrapper">
      <ProgressBar currentStep={1} totalSteps={2} />
    </div>
  {/if}
  
  <div class="import-content">
    <div class="import-header">
      <h2 class="import-title">Import Wallet</h2>
      <p class="import-subtitle">Enter your 12-word seed phrase to access your existing wallet</p>
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
          disabled={isImporting}
        />
      </div>
      
      <div class="form-group">
        <label for="seed-phrase" class="form-label">Seed Phrase</label>
        <textarea
          id="seed-phrase"
          class="textarea-field"
          class:error
          placeholder="Enter your 12-word seed phrase..."
          bind:value={seedPhrase}
          disabled={isImporting}
          rows="4"
        />
        <p class="helper-text">Enter the words separated by spaces</p>
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
      disabled={isImporting}
    >
      {isImporting ? 'Importing...' : 'Import Wallet'}
    </Button>
  </div>
</div>

<style>
  .import-wallet {
    height: 100%;
    background: var(--color-background);
    display: flex;
    flex-direction: column;
    position: relative;
  }

  .progress-wrapper {
    position: absolute;
    top: 28px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 10;
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
  
  .input-field.error,
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
    border: 1px solid var(--color-error)1731;
    color: var(--color-error);
    padding: 8px 12px;
    border-radius: 6px;
    font-size: 14px;
    margin-bottom: 16px;
  }
  
  .button-group {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    margin-top: 32px;
  }
</style>