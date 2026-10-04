<script lang="ts">
  import WalletAddressActions from './WalletAddressActions.svelte';
  import { createEventDispatcher } from 'svelte';
  import { formatNocksWithSeparator } from '../../utils/nicks';

  import type { Wallet } from '../../types/wallet';
  export let wallet: Wallet;
  export let isActive = false;

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

  function handleAction(action: string, e: Event) {
    e.stopPropagation(); // Prevent click from bubbling to wallet card
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

<div
  class="wallet-item"
  class:active={isActive}
  role="button"
  tabindex="0"
  on:click={handleClick}
  on:keydown={e => {
    if (e.target === e.currentTarget && (e.key === 'Enter' || e.key === ' ')) {
      e.preventDefault();
      handleClick();
    }
  }}
>
  <div class="wallet-info">
    <div class="wallet-content">
      <div class="wallet-heading">
        <div class="wallet-name">{wallet.name}</div>
      </div>
      <div class="wallet-balance">
        {formatNocksWithSeparator(BigInt(Math.trunc(wallet.balance || 0)), 2)} NOCK
      </div>
      <div class="wallet-address-row">
        <span class="wallet-address"
          >{formatAddress(wallet.addresses?.[wallet.currentAddressIndex || 0] || '')}</span
        >
        <WalletAddressActions address={wallet.addresses?.[wallet.currentAddressIndex || 0] || ''} />
      </div>
    </div>
  </div>

  <div class="wallet-actions">
    <button class="menu-button" aria-label="Wallet options" on:click={handleMenuClick}>
      <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
        <circle cx="10" cy="5" r="1.5" fill="currentColor" />
        <circle cx="10" cy="10" r="1.5" fill="currentColor" />
        <circle cx="10" cy="15" r="1.5" fill="currentColor" />
      </svg>
    </button>

    <div class="menu-dropdown" class:active={showMenu}>
      <button class="menu-option" on:click={e => handleAction('rename', e)}>
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
          <path
            d="M11.5 2.5L13.5 4.5L11.5 2.5ZM12.5 1.5L8 6L6 10L10 8L14.5 3.5C14.7761 3.22386 14.7761 2.77614 14.5 2.5L13.5 1.5C13.2239 1.22386 12.7761 1.22386 12.5 1.5Z"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
        Rename
      </button>
      <button class="menu-option danger" on:click={e => handleAction('delete', e)}>
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
          <path
            d="M6 2L6 1.5C6 1.22386 6.22386 1 6.5 1H9.5C9.77614 1 10 1.22386 10 1.5V2M2 4H14M12.5 4V13C12.5 13.5523 12.0523 14 11.5 14H4.5C3.94772 14 3.5 13.5523 3.5 13V4"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
          />
        </svg>
        Delete
      </button>
    </div>
  </div>
</div>

<style>
  .wallet-item {
    display: flex;
    align-items: stretch;
    gap: 12px;
    padding: 15px 14px 15px 16px;
    min-height: 112px;
    background: var(--color-surface);
    border-radius: 10px;
    cursor: pointer;
    transition: all 0.2s ease;
    position: relative;
  }

  /* No border by default for both themes */
  .wallet-item {
    border: none;
  }

  /* Dark mode - non-active cards lighter */
  :global([data-theme='dark']) .wallet-item:not(.active) {
    background: rgba(255, 255, 255, 0.04);
  }

  /* Dark mode hover - as bright as active card */
  :global([data-theme='dark']) .wallet-item:hover {
    background: rgba(255, 255, 255, 0.08);
  }

  /* Light mode hover */
  :global([data-theme='light']) .wallet-item:hover {
    background: rgba(0, 0, 0, 0.03);
  }

  /* Dark mode active - border to indicate selection */
  :global([data-theme='dark']) .wallet-item.active {
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.15);
  }

  /* Light mode active - border to indicate selection */
  :global([data-theme='light']) .wallet-item.active {
    background: rgba(0, 0, 0, 0.04);
    border: 1px solid rgba(0, 0, 0, 0.15);
  }

  .wallet-info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    height: 100%;
  }

  .wallet-content {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .wallet-heading {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }

  .wallet-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text);
    line-height: 1;
  }

  .wallet-balance {
    font-size: 14px;
    font-weight: 500;
    color: var(--color-text-secondary);
    line-height: 1;
  }

  .wallet-address-row {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
  }

  .wallet-address {
    font-size: 12px;
    color: var(--color-text-tertiary);
    font-family: monospace;
    line-height: 1;
    white-space: nowrap;
  }

  .wallet-actions {
    position: relative;
    display: flex;
    align-items: center;
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
    overflow: visible;
    z-index: 10;
    min-width: 180px;
    opacity: 0;
    transform: translateY(-10px) scale(0.95);
    transform-origin: top right;
    pointer-events: none;
    transition: all 0.2s ease;
  }

  .menu-dropdown::before {
    content: '';
    position: absolute;
    top: -7px;
    right: 12px;
    width: 12px;
    height: 12px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-right: none;
    border-bottom: none;
    transform: rotate(45deg);
  }

  .menu-dropdown.active {
    opacity: 1;
    transform: translateY(0) scale(1);
    pointer-events: all;
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
