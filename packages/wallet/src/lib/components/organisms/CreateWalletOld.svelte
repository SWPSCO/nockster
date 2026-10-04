<script>
  import { createWallet } from '../lib/stores/wallet';

  let walletName = '';
  let mnemonic = '';
  let showMnemonic = false;
  let confirmSaved = false;
  let isCreating = false;
  let error = '';

  async function handleCreate() {
    error = '';

    if (!walletName.trim()) {
      error = 'Please enter a wallet name';
      return;
    }

    isCreating = true;

    try {
      const wallet = await createWallet(walletName);
      mnemonic = wallet.mnemonic;
      showMnemonic = true;
    } catch (err) {
      error = err.message;
    } finally {
      isCreating = false;
    }
  }

  function handleConfirm() {
    if (!confirmSaved) {
      error = 'Please confirm you have saved your recovery phrase';
      return;
    }
    // Navigation handled in createWallet function
  }
</script>

<div class="create-wallet">
  {#if !showMnemonic}
    <div class="create-form">
      <h2>Create New Wallet</h2>
      <p class="description">Create a new wallet with a unique recovery phrase</p>

      <form on:submit|preventDefault={handleCreate}>
        <div class="form-group">
          <label for="name">Wallet Name</label>
          <input
            id="name"
            type="text"
            bind:value={walletName}
            placeholder="e.g., Personal Wallet"
            required
          />
        </div>

        {#if error}
          <div class="error-message">
            {error}
          </div>
        {/if}

        <button type="submit" class="btn-primary" disabled={isCreating}>
          {isCreating ? 'Creating...' : 'Create Wallet'}
        </button>
      </form>
    </div>
  {:else}
    <div class="mnemonic-display">
      <h2>Save Your Recovery Phrase</h2>
      <div class="warning">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path
            d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"
          />
          <path d="M12 9v4M12 17h.01" />
        </svg>
        <p>Write down these words in order. You'll need them to recover your wallet.</p>
      </div>

      <div class="mnemonic-grid">
        {#each mnemonic.split(' ') as word, i}
          <div class="word-card">
            <span class="word-number">{i + 1}</span>
            <span class="word-text">{word}</span>
          </div>
        {/each}
      </div>

      <div class="confirm-section">
        <label class="checkbox-label">
          <input type="checkbox" bind:checked={confirmSaved} />
          <span>I have safely written down my recovery phrase</span>
        </label>

        {#if error}
          <div class="error-message">
            {error}
          </div>
        {/if}

        <button class="btn-primary" on:click={handleConfirm} disabled={!confirmSaved}>
          Continue to Wallet
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .create-wallet {
    display: flex;
    flex-direction: column;
  }

  .create-form,
  .mnemonic-display {
    background: rgba(255, 255, 255, 0.05);
    backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 12px;
    padding: 1.5rem;
  }

  h2 {
    margin: 0 0 0.5rem 0;
    font-size: 1.25rem;
    background: linear-gradient(135deg, #4fc3f7, #29b6f6);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
  }

  .description {
    color: rgba(255, 255, 255, 0.7);
    font-size: 0.875rem;
    margin-bottom: 1.5rem;
  }

  .form-group {
    margin-bottom: 1rem;
  }

  label {
    display: block;
    margin-bottom: 0.5rem;
    font-size: 0.875rem;
    color: rgba(255, 255, 255, 0.9);
  }

  input[type='text'] {
    width: 100%;
    padding: 0.75rem;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 8px;
    color: #fff;
    font-size: 0.875rem;
    transition: all 0.2s;
  }

  input[type='text']::placeholder {
    color: rgba(255, 255, 255, 0.4);
  }

  input[type='text']:focus {
    outline: none;
    border-color: #4fc3f7;
    background: rgba(255, 255, 255, 0.15);
  }

  .warning {
    display: flex;
    gap: 0.75rem;
    padding: 0.75rem;
    background: rgba(255, 152, 0, 0.1);
    border: 1px solid rgba(255, 152, 0, 0.3);
    border-radius: 8px;
    margin-bottom: 1.5rem;
  }

  .warning svg {
    width: 20px;
    height: 20px;
    color: #ff9800;
    flex-shrink: 0;
  }

  .warning p {
    margin: 0;
    font-size: 0.875rem;
    color: rgba(255, 255, 255, 0.8);
  }

  .mnemonic-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.5rem;
    padding: 1rem;
    background: rgba(0, 0, 0, 0.2);
    border-radius: 8px;
    margin-bottom: 1.5rem;
  }

  .word-card {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem;
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
  }

  .word-number {
    font-size: 0.75rem;
    color: rgba(255, 255, 255, 0.5);
    font-weight: 600;
  }

  .word-text {
    font-family: monospace;
    font-size: 0.875rem;
    color: #fff;
  }

  .confirm-section {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    cursor: pointer;
  }

  .checkbox-label input[type='checkbox'] {
    width: 18px;
    height: 18px;
    cursor: pointer;
  }

  .checkbox-label span {
    font-size: 0.875rem;
    color: rgba(255, 255, 255, 0.8);
  }

  .error-message {
    background: rgba(244, 67, 54, 0.1);
    border: 1px solid rgba(244, 67, 54, 0.3);
    color: #ff6b6b;
    padding: 0.75rem;
    border-radius: 8px;
    font-size: 0.875rem;
  }

  .btn-primary {
    width: 100%;
    padding: 0.875rem;
    background: linear-gradient(135deg, #4fc3f7, #29b6f6);
    color: white;
    border: none;
    border-radius: 8px;
    font-size: 1rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s;
  }

  .btn-primary:hover:not(:disabled) {
    transform: translateY(-2px);
    box-shadow: 0 8px 16px rgba(79, 195, 247, 0.3);
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
