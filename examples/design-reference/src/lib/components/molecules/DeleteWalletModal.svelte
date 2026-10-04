<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { fade } from 'svelte/transition';

  export let walletName: string;
  export let balance: number;
  export let canDelete: boolean = true;

  const dispatch = createEventDispatcher();

  let confirmationText = '';
  $: isConfirmed = confirmationText.toLowerCase() === walletName.toLowerCase();

  function handleDelete() {
    if (isConfirmed && canDelete) {
      dispatch('delete');
    }
  }

  function handleCancel() {
    dispatch('cancel');
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && isConfirmed) {
      handleDelete();
    } else if (e.key === 'Escape') {
      handleCancel();
    }
  }
</script>

<div class="modal-overlay" transition:fade={{ duration: 200 }} on:click={handleCancel}>
  <div class="modal-content" on:click|stopPropagation>
    <div class="warning-icon">
      <svg width="48" height="48" viewBox="0 0 48 48" fill="none">
        <circle cx="24" cy="24" r="20" stroke="#dc2626" stroke-width="2"/>
        <path d="M24 16V26" stroke="#dc2626" stroke-width="2" stroke-linecap="round"/>
        <circle cx="24" cy="32" r="1" fill="#dc2626"/>
      </svg>
    </div>

    <h2 class="modal-title">Delete Wallet</h2>

    {#if !canDelete}
      <p class="warning-text">
        You cannot delete your only wallet. Create another wallet first.
      </p>
    {:else}
      <p class="warning-text">
        Are you sure you want to delete <strong>{walletName}</strong>?
      </p>

      {#if balance > 0}
        <div class="balance-warning">
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
            <path d="M8 1L1 14H15L8 1Z" stroke="#f59e0b" stroke-width="1.5" stroke-linejoin="round"/>
            <path d="M8 6V9" stroke="#f59e0b" stroke-width="1.5" stroke-linecap="round"/>
            <circle cx="8" cy="11" r="0.5" fill="#f59e0b"/>
          </svg>
          This wallet contains {balance.toFixed(2)} NOCK
        </div>
      {/if}

      <p class="info-text">
        This action cannot be undone. Make sure you have backed up your seed phrase before deleting this wallet.
      </p>

      <div class="confirmation-group">
        <label for="confirm-delete" class="input-label">
          Type <strong>{walletName}</strong> to confirm deletion
        </label>
        <input
          id="confirm-delete"
          type="text"
          class="input-field"
          bind:value={confirmationText}
          on:keydown={handleKeydown}
          placeholder="Enter wallet name"
        />
      </div>
    {/if}

    <div class="modal-actions">
      <button class="button-secondary" on:click={handleCancel}>
        Cancel
      </button>
      {#if canDelete}
        <button
          class="button-danger"
          on:click={handleDelete}
          disabled={!isConfirmed}
        >
          Delete Wallet
        </button>
      {/if}
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
    max-width: 360px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.15);
  }

  .warning-icon {
    display: flex;
    justify-content: center;
    margin-bottom: 16px;
  }

  .modal-title {
    font-size: 20px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 16px;
    text-align: center;
  }

  .warning-text {
    font-size: 14px;
    color: var(--color-text-secondary);
    margin-bottom: 16px;
    text-align: center;
    line-height: 1.5;
  }

  .warning-text strong {
    font-weight: 600;
    color: var(--color-text);
  }

  .balance-warning {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px;
    background: #fef3c7;
    border: 1px solid #fde68a;
    border-radius: 8px;
    margin-bottom: 16px;
    font-size: 14px;
    font-weight: 500;
    color: #92400e;
  }

  .info-text {
    font-size: 13px;
    color: var(--color-text-secondary);
    margin-bottom: 20px;
    text-align: center;
    line-height: 1.5;
  }

  .confirmation-group {
    margin-bottom: 24px;
  }

  .input-label {
    display: block;
    font-size: 13px;
    color: var(--color-text-secondary);
    margin-bottom: 8px;
    text-align: center;
  }

  .input-label strong {
    font-weight: 600;
    color: var(--color-text);
  }

  .input-field {
    width: 100%;
    padding: 12px 14px;
    border: 1.5px solid var(--color-border)1665;
    border-radius: 10px;
    font-size: 15px;
    color: var(--color-text);
    background: var(--color-background);
    outline: none;
    transition: all 0.15s ease;
  }

  .input-field:focus {
    border-color: var(--color-error);
  }

  .input-field::placeholder {
    color: var(--color-text-tertiary);
  }

  .modal-actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  .button-danger,
  .button-secondary {
    padding: 12px 20px;
    border-radius: 10px;
    font-size: 15px;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.15s ease;
    border: none;
  }

  .button-danger {
    background: var(--color-error);
    color: var(--color-background);
  }

  .button-danger:hover:not(:disabled) {
    background: #b91c1c;
  }

  .button-danger:disabled {
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
</style>