<script lang="ts">
  import { walletStore } from '../../stores/wallet';
  import { createEventDispatcher } from 'svelte';
  import { fade } from 'svelte/transition';

  export let currentName: string;

  const dispatch = createEventDispatcher();

  let newName = currentName;
  $: nameExists = newName.trim() !== currentName && $walletStore.wallets.some(wallet => wallet.name === newName.trim());
  let inputElement: HTMLInputElement;

  function handleSubmit() {
    if (newName.trim() && !nameExists) {
      if (newName !== currentName) {
        dispatch('rename', newName.trim());
      } else {
        // Close modal even if name hasn't changed
        dispatch('cancel');
      }
    }
  }

  function handleCancel() {
    dispatch('cancel');
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      handleSubmit();
    } else if (e.key === 'Escape') {
      handleCancel();
    }
  }

  // Focus input on mount
  function focusInput(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
</script>

<div
  class="modal-overlay"
  role="presentation"
  transition:fade={{ duration: 200 }}
  on:click={handleCancel}
  on:keydown={e => {
    if (e.key === 'Escape') handleCancel();
  }}
>
  <div
    class="modal-content"
    role="dialog"
    aria-modal="true"
    aria-labelledby="modal-title"
    tabindex="-1"
    on:click|stopPropagation
    on:keydown|stopPropagation
  >
    <h2 class="modal-title" id="modal-title">Rename Wallet</h2>

    <div class="input-group">
      <label for="wallet-name" class="input-label">Wallet Name</label>
      <input
        id="wallet-name"
        type="text"
        class="input-field"
        bind:value={newName}
        class:duplicate-name={nameExists}
        aria-invalid={nameExists}
        bind:this={inputElement}
        use:focusInput
        on:keydown={handleKeydown}
        placeholder="Enter wallet name"
        maxlength="20"
      />
      {#if nameExists}
        <p class="duplicate-name" role="status">A wallet with this name already exists</p>
      {/if}
    </div>

    <div class="modal-actions">
      <button class="button-secondary" on:click={handleCancel}> Cancel </button>
      <button
        class="button-primary"
        on:click={handleSubmit}
        disabled={!newName.trim() || newName === currentName || nameExists}
      >
        Save
      </button>
    </div>
  </div>
</div>

<style>
  .modal-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    padding: 20px;
  }

  .modal-content {
    background: var(--color-background);
    border-radius: 16px;
    padding: 24px;
    width: 100%;
    max-width: 320px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.15);
    border: 1px solid var(--color-border);
  }

  .modal-title {
    font-size: 20px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 20px;
    text-align: center;
  }

  .input-group {
    margin-bottom: 24px;
  }

  .input-label {
    display: block;
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text-secondary);
    margin-bottom: 8px;
  }

  .input-field {
    width: 100%;
    padding: 12px 14px;
    border: 1.5px solid var(--color-border);
    border-radius: 10px;
    font-size: 15px;
    color: var(--color-text);
    background: var(--color-surface);
    outline: none;
    transition: all 0.15s ease;
  }

  .input-field:focus {
    border-color: var(--color-text);
  }

  .input-field::placeholder {
    color: var(--color-text-tertiary);
  }

  .modal-actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .button-primary,
  .button-secondary {
    padding: 12px 20px;
    border-radius: 10px;
    font-size: 15px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
    border: none;
  }

  .button-primary {
    background: var(--color-text);
    color: var(--color-background);
  }

  .button-primary:hover:not(:disabled) {
    background: #171717;
  }

  .button-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .button-secondary {
    background: var(--color-surface);
    color: var(--color-text);
  }

  .button-secondary:hover {
    background: var(--color-border);
  }
  .input-field.duplicate-name,
  .duplicate-name {
    color: var(--color-error);
    border-color: var(--color-error);
  }
</style>
