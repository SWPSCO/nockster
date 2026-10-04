<script lang="ts">
  import { nextWalletName } from '../../utils/walletName';
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { walletStore, activeWallet } from '../../stores/wallet';
  import type { Wallet } from '../../types';
  import { router } from '../../stores/router';
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';
  import WalletListItem from '../molecules/WalletListItem.svelte';
  import RenameWalletModal from '../molecules/RenameWalletModal.svelte';
  import DeleteWalletModal from '../molecules/DeleteWalletModal.svelte';
  import {
    generateMnemonic,
    setPendingMnemonic,
    renameWalletInVault,
    deleteWalletFromVault,
    getVaultNickname,
    getWalletsFromVault
  } from '../../utils/vaultBridge';

  export let onBack: () => void = () => {};

  onMount(() => {
    console.log('WalletManagement component mounted');
  });

  let showAddMenu = false;
  let showRenameModal = false;
  let showDeleteModal = false;
  let selectedWallet: Wallet | null = null;

  $: wallets = $walletStore.wallets;
  $: currentActiveWallet = $activeWallet;
  // Ensure we properly filter out the active wallet from the others list
  $: otherWallets = currentActiveWallet
    ? wallets.filter(w => w.id !== currentActiveWallet.id)
    : wallets;
  $: canDeleteSelected =
    wallets.length > 1 ||
    !!(selectedWallet && (selectedWallet.watchOnly || selectedWallet.hardware));

  function isStoreOnlyWallet(wallet: Wallet | null): boolean {
    return !!wallet && !!(wallet.watchOnly || wallet.hardware);
  }

  function handleWalletClick(wallet: Wallet) {
    walletStore.selectWallet(wallet.id);
    router.navigate('dashboard');
  }

  function handleWalletAction(event: CustomEvent<{ action: string; wallet: Wallet }>) {
    const { action, wallet } = event.detail;
    selectedWallet = wallet;

    switch (action) {
      case 'rename':
        showRenameModal = true;
        break;
      case 'delete':
        showDeleteModal = true;
        break;
    }
  }

  async function handleRename(event: CustomEvent<string>) {
    if (selectedWallet) {
      const newNickname = event.detail;
      const storeOnly = isStoreOnlyWallet(selectedWallet);

      if (storeOnly) {
        walletStore.renameWallet(selectedWallet.id, newNickname);
        showRenameModal = false;
        selectedWallet = null;
        return;
      }

      const oldNickname = getVaultNickname(selectedWallet);

      // Rename in vault first
      const result = await renameWalletInVault(oldNickname, newNickname);
      if (!result.success) {
        alert('Failed to rename wallet: ' + result.error);
        showRenameModal = false;
        selectedWallet = null;
        return;
      }

      // Then update store
      walletStore.renameWallet(selectedWallet.id, newNickname);
    }
    showRenameModal = false;
    selectedWallet = null;
  }

  async function handleDelete() {
    if (selectedWallet) {
      const storeOnly = isStoreOnlyWallet(selectedWallet);
      if (!storeOnly && wallets.length <= 1) {
        showDeleteModal = false;
        selectedWallet = null;
        return;
      }

      if (!storeOnly) {
        const nickname = getVaultNickname(selectedWallet);

        // Delete from vault first
        const result = await deleteWalletFromVault(nickname);
        if (!result.success) {
          alert('Failed to delete wallet: ' + result.error);
          showDeleteModal = false;
          selectedWallet = null;
          return;
        }
      }

      // Then remove from store
      walletStore.deleteWallet(selectedWallet.id);
    }
    showDeleteModal = false;
    selectedWallet = null;
  }

  async function handleCreateWallet() {
    showAddMenu = false;

    // Generate a new mnemonic using the vault API
    const result = await generateMnemonic();
    if (!result.success || !result.mnemonic) {
      alert('Failed to generate wallet: ' + (result.error || 'Unknown error'));
      return;
    }

    // Generate a unique wallet name by checking existing vault wallets
    const vaultResult = await getWalletsFromVault();
    const walletName = nextWalletName([...new Set([...wallets.map(wallet => wallet.name), ...(vaultResult.wallets?.map(wallet => wallet.name) ?? [])])]);

    setPendingMnemonic(result.mnemonic, walletName);

    // Mark as creating additional wallet
    walletStore.setCreatingAdditionalWallet(true);

    // Create a temporary wallet in the store with the seed phrase for display
    await walletStore.createWallet(
      walletName,
      [], // addresses will be set after vault import
      result.mnemonic
    );

    // Navigate to seed phrase display
    router.navigate('seed-phrase');
  }

  function handleImportWallet() {
    showAddMenu = false;
    // Store that we're importing an additional wallet
    walletStore.setCreatingAdditionalWallet(true);
    // Navigate to import wallet screen
    router.navigate('import-wallet');
  }

  function handleWatchAddress() {
    showAddMenu = false;
    // Navigate to watch-only import screen
    router.navigate('import-watch-only');
  }

  function handleConnectHardware() {
    showAddMenu = false;
    router.navigate('hardware-wallet');
  }

  // Close add menu when clicking outside
  function handleOutsideClick(e: Event) {
    if (showAddMenu && !(e.target as HTMLElement).closest('.add-wallet-wrapper')) {
      showAddMenu = false;
    }
  }
</script>

<svelte:window on:click={handleOutsideClick} />

<div class="wallet-management fixed-screen" in:fly={{ y: -10, duration: 300, opacity: 0 }}>
  <Header title="Manage Wallets" showBack={true} on:click={onBack} />

  <div class="content">
    {#if $activeWallet}
      <div class="section active-wallet-section">
        <WalletListItem
          wallet={$activeWallet}
          isActive={true}
          on:click={() => {}}
          on:action={handleWalletAction}
        />
      </div>
    {/if}

    {#if otherWallets.length > 0}
      <div class="section other-wallets-section">
        <div class="wallet-list">
          {#each otherWallets as wallet, i (wallet.id)}
            <div in:fly={{ y: -80, duration: 400, opacity: 0, delay: 200 + i * 40 }}>
              <WalletListItem
                {wallet}
                isActive={false}
                on:click={() => handleWalletClick(wallet)}
                on:action={handleWalletAction}
              />
            </div>
          {/each}
        </div>
      </div>
    {/if}

    {#if wallets.length === 0}
      <div class="empty-state">
        <p class="empty-message">No wallets found</p>
        <p class="empty-submessage">Create or import a wallet to get started</p>
      </div>
    {/if}
  </div>

  <div class="button-footer">
    <div class="add-wallet-wrapper">
      <Button variant="primary" fullWidth={true} on:click={() => (showAddMenu = !showAddMenu)}>
        + Add Wallet
      </Button>

      {#if showAddMenu}
        <div class="add-menu">
          <button class="menu-item" on:click={handleCreateWallet}>
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path
                d="M10 4V16M4 10H16"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
              />
            </svg>
            Create New Wallet
          </button>
          <button class="menu-item" on:click={handleImportWallet}>
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path
                d="M10 4V12M10 12L7 9M10 12L13 9M4 16H16"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
            Import Wallet
          </button>
          <button class="menu-item" on:click={handleWatchAddress}>
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path
                d="M10 12C11.6569 12 13 10.6569 13 9C13 7.34315 11.6569 6 10 6C8.34315 6 7 7.34315 7 9C7 10.6569 8.34315 12 10 12Z"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
              <path
                d="M10 3C5 3 2 9 2 9C2 9 5 15 10 15C15 15 18 9 18 9C18 9 15 3 10 3Z"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
            Watch Address
          </button>
          <button class="menu-item" on:click={handleConnectHardware}>
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path
                d="M6 3V6M14 3V6M6 14V17M14 14V17M4 8H16M4 12H16"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
              />
              <rect
                x="6"
                y="8"
                width="8"
                height="4"
                stroke="currentColor"
                stroke-width="2"
                rx="1"
              />
            </svg>
            Connect Nockster device
          </button>
        </div>
      {/if}
    </div>
  </div>

  {#if showRenameModal && selectedWallet}
    <RenameWalletModal
      currentName={selectedWallet.name}
      on:rename={handleRename}
      on:cancel={() => (showRenameModal = false)}
    />
  {/if}

  {#if showDeleteModal && selectedWallet}
    <DeleteWalletModal
      walletName={selectedWallet.name}
      balance={selectedWallet.balance ?? 0}
      canDelete={canDeleteSelected}
      on:delete={handleDelete}
      on:cancel={() => (showDeleteModal = false)}
    />
  {/if}
</div>

<style>
  .wallet-management {
    height: 100%;
    background: var(--color-background);
    display: flex;
    flex-direction: column;
  }

  .content {
    flex: 1;
    padding: 20px;
    overflow-y: auto;
  }

  .section {
    margin-bottom: 32px;
  }

  .active-wallet-section {
    position: sticky;
    top: -40px;
    background: var(--color-background);
    z-index: 10;
    padding-top: 40px;
    padding-bottom: 32px;
    margin-top: -20px;
    margin-bottom: 0;
    margin-left: -20px;
    margin-right: -20px;
    padding-left: 20px;
    padding-right: 20px;
  }

  .other-wallets-section {
    padding-top: 28px;
  }

  .section-title {
    font-size: 12px;
    font-weight: 500;
    color: var(--color-text-tertiary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 12px;
  }

  .wallet-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 300px;
    text-align: center;
  }

  .empty-message {
    font-size: 18px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 8px;
  }

  .empty-submessage {
    font-size: 14px;
    color: var(--color-text-secondary);
  }

  .add-wallet-wrapper {
    position: relative;
  }

  .add-menu {
    position: absolute;
    bottom: calc(100% + 8px);
    left: 0;
    right: 0;
    background: var(--color-background);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    overflow: hidden;
    box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.1);
  }

  .menu-item {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 14px 16px;
    background: none;
    border: none;
    font-size: 15px;
    font-weight: 500;
    color: var(--color-text);
    cursor: pointer;
    text-align: left;
    transition: background 0.15s ease;
  }

  .menu-item:hover {
    background: var(--color-surface);
  }

  .menu-item:not(:last-child) {
    border-bottom: 1px solid var(--color-surface) 1600;
  }

  .menu-item svg {
    color: var(--color-text-secondary);
  }
</style>
