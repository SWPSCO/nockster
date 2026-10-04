<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { Wallet } from '../../stores/wallet';

  export let wallet: Wallet;
  export let isActive: boolean = false;

  const dispatch = createEventDispatcher();

  let showMenu = false;

  function handleClick() {
    if (!isActive) {
      dispatch('click');
    }
  }

  function handleMenuClick(e: Event) {
    e.stopPropagation();
    showMenu = !showMenu;
  }

  function handleAction(action: string) {
    dispatch('action', { action, wallet });
    showMenu = false;
  }

  function formatAddress(address: string): string {
    if (address.length > 10) {
      return `${address.slice(0, 6)}...${address.slice(-4)}`;
    }
    return address;
  }

  // Close menu when clicking outside
  function handleOutsideClick(e: Event) {
    if (showMenu && !(e.target as HTMLElement).closest('.menu-button')) {
      showMenu = false;
    }
  }
</script>

<svelte:window on:click={handleOutsideClick} />

<div class="wallet-item" class:active={isActive} on:click={handleClick}>
  <div class="wallet-indicator">
    {#if isActive}
      <span class="active-dot"></span>
    {/if}
  </div>

  <div class="wallet-info">
    <div class="wallet-name">{wallet.name}</div>
    <div class="wallet-balance">{wallet.balance.toFixed(2)} NOCK</div>
    <div class="wallet-address">{formatAddress(wallet.address)}</div>
  </div>

  <div class="wallet-actions">
    <button class="menu-button" on:click={handleMenuClick}>
      <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
        <circle cx="10" cy="5" r="1.5" fill="currentColor"/>
        <circle cx="10" cy="10" r="1.5" fill="currentColor"/>
        <circle cx="10" cy="15" r="1.5" fill="currentColor"/>
      </svg>
    </button>

    {#if showMenu}
      <div class="menu-dropdown">
        {#if !isActive}
          <button class="menu-option" on:click={() => dispatch('click')}>
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
              <circle cx="8" cy="8" r="6" stroke="currentColor" stroke-width="1.5"/>
              <path d="M5 8L7 10L11 6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
            Set as Active
          </button>
        {/if}
        <button class="menu-option" on:click={() => handleAction('rename')}>
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
            <path d="M11.5 2.5L13.5 4.5L11.5 2.5ZM12.5 1.5L8 6L6 10L10 8L14.5 3.5C14.7761 3.22386 14.7761 2.77614 14.5 2.5L13.5 1.5C13.2239 1.22386 12.7761 1.22386 12.5 1.5Z" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
          Rename
        </button>
        <button class="menu-option" on:click={() => handleAction('duplicate')}>
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
            <rect x="5" y="5" width="9" height="9" stroke="currentColor" stroke-width="1.5" rx="1"/>
            <path d="M11 5V3C11 2.44772 10.5523 2 10 2H3C2.44772 2 2 2.44772 2 3V10C2 10.5523 2.44772 11 3 11H5" stroke="currentColor" stroke-width="1.5"/>
          </svg>
          Duplicate
        </button>
        <button class="menu-option danger" on:click={() => handleAction('delete')}>
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
            <path d="M6 2L6 1.5C6 1.22386 6.22386 1 6.5 1H9.5C9.77614 1 10 1.22386 10 1.5V2M2 4H14M12.5 4V13C12.5 13.5523 12.0523 14 11.5 14H4.5C3.94772 14 3.5 13.5523 3.5 13V4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>
          </svg>
          Delete
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .wallet-item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px;
    background: var(--color-surface);
    border: 1.5px solid var(--color-border);
    border-radius: 12px;
    cursor: pointer;
    transition: all 0.15s ease;
    position: relative;
  }

  .wallet-item:hover {
    border-color: rgba(255, 255, 255, 0.15);
    background: rgba(255, 255, 255, 0.05);
  }

  .wallet-item.active {
    border-color: var(--color-success);
    background: rgba(52, 199, 89, 0.1);
  }

  .wallet-indicator {
    width: 8px;
    height: 100%;
    display: flex;
    align-items: center;
  }

  .active-dot {
    width: 8px;
    height: 8px;
    background: var(--color-success);
    border-radius: 50%;
  }

  .wallet-info {
    flex: 1;
    min-width: 0;
  }

  .wallet-name {
    font-size: 16px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 4px;
  }

  .wallet-balance {
    font-size: 18px;
    font-weight: 500;
    color: var(--color-text);
    margin-bottom: 4px;
  }

  .wallet-address {
    font-size: 13px;
    color: var(--color-text-tertiary);
    font-family: monospace;
  }

  .wallet-actions {
    position: relative;
  }

  .menu-button {
    padding: 8px;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-text-secondary);
    transition: color 0.15s ease;
    border-radius: 8px;
  }

  .menu-button:hover {
    color: var(--color-text);
    background: var(--color-surface);
  }

  .menu-dropdown {
    position: absolute;
    top: 100%;
    right: 0;
    margin-top: 4px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.1);
    overflow: hidden;
    z-index: 10;
    min-width: 180px;
  }

  .menu-option {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 10px 14px;
    background: none;
    border: none;
    font-size: 14px;
    font-weight: 500;
    color: var(--color-text-secondary);
    cursor: pointer;
    text-align: left;
    transition: background 0.15s ease;
  }

  .menu-option:hover {
    background: var(--color-surface);
  }

  .menu-option.danger {
    color: var(--color-error);
  }

  .menu-option.danger:hover {
    background: rgba(255, 59, 48, 0.1);
  }

  .menu-option:not(:last-child) {
    border-bottom: 1px solid var(--color-border);
  }

  .menu-option svg {
    flex-shrink: 0;
  }
</style>