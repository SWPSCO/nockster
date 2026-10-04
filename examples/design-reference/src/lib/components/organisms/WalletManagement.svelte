<script lang="ts">
  import { walletStore, activeWallet } from '../../stores/wallet';
  import type { Wallet } from '../../stores/wallet';
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';
  import WalletListItem from '../molecules/WalletListItem.svelte';
  import RenameWalletModal from '../molecules/RenameWalletModal.svelte';
  import DeleteWalletModal from '../molecules/DeleteWalletModal.svelte';

  export let onBack: () => void = () => {};

  let showAddMenu = false;
  let showRenameModal = false;
  let showDeleteModal = false;
  let selectedWallet: Wallet | null = null;

  $: wallets = $walletStore.wallets;
  $: activeWalletId = $walletStore.activeWalletId;
  $: otherWallets = wallets.filter(w => w.id !== activeWalletId);

  function handleWalletClick(wallet: Wallet) {
    walletStore.selectWallet(wallet.id);
  }

  function handleWalletAction(event: CustomEvent<{ action: string, wallet: Wallet }>) {
    const { action, wallet } = event.detail;
    selectedWallet = wallet;

    switch(action) {
      case 'rename':
        showRenameModal = true;
        break;
      case 'delete':
        showDeleteModal = true;
        break;
      case 'duplicate':
        walletStore.duplicateWallet(wallet.id);
        break;
    }
  }

  function handleRename(event: CustomEvent<string>) {
    if (selectedWallet) {
      walletStore.renameWallet(selectedWallet.id, event.detail);
    }
    showRenameModal = false;
    selectedWallet = null;
  }

  function handleDelete() {
    if (selectedWallet && wallets.length > 1) {
      walletStore.deleteWallet(selectedWallet.id);
    }
    showDeleteModal = false;
    selectedWallet = null;
  }

  function handleCreateWallet() {
    showAddMenu = false;
    walletStore.createWallet({
      name: `Wallet ${wallets.length + 1}`,
      address: `nc1q${Math.random().toString(36).substr(2, 15)}`,
      balance: 0,
      seedPhrase: generateMockSeedPhrase(),
      network: 'mainnet'
    });
  }

  function handleImportWallet() {
    showAddMenu = false;
    // Navigate to import screen
    onBack(); // For now, just go back
  }

  function generateMockSeedPhrase() {
    const words = [
      'abandon', 'ability', 'able', 'about', 'above', 'absent',
      'absorb', 'abstract', 'absurd', 'abuse', 'access', 'accident'
    ];
    return Array.from({ length: 12 }, () =>
      words[Math.floor(Math.random() * words.length)]
    );
  }
</script>

<div class="wallet-management fixed-screen">
  <Header title="Manage Wallets" showBack={true} on:click={onBack} />

  <div class="content">
    {#if $activeWallet}
      <div class="section">
        <h3 class="section-title">Active Wallet</h3>
        <WalletListItem
          wallet={$activeWallet}
          isActive={true}
          on:click={() => {}}
          on:action={handleWalletAction}
        />
      </div>
    {/if}

    {#if otherWallets.length > 0}
      <div class="section">
        <h3 class="section-title">Other Wallets</h3>
        <div class="wallet-list">
          {#each otherWallets as wallet}
            <WalletListItem
              {wallet}
              isActive={false}
              on:click={() => handleWalletClick(wallet)}
              on:action={handleWalletAction}
            />
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
      <Button
        variant="primary"
        fullWidth={true}
        on:click={() => showAddMenu = !showAddMenu}
      >
        + Add Wallet
      </Button>

      {#if showAddMenu}
        <div class="add-menu">
          <button class="menu-item" on:click={handleCreateWallet}>
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path d="M10 4V16M4 10H16" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>
            </svg>
            Create New Wallet
          </button>
          <button class="menu-item" on:click={handleImportWallet}>
            <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
              <path d="M10 4V12M10 12L7 9M10 12L13 9M4 16H16" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
            Import Wallet
          </button>
        </div>
      {/if}
    </div>
  </div>

  {#if showRenameModal && selectedWallet}
    <RenameWalletModal
      currentName={selectedWallet.name}
      on:rename={handleRename}
      on:cancel={() => showRenameModal = false}
    />
  {/if}

  {#if showDeleteModal && selectedWallet}
    <DeleteWalletModal
      walletName={selectedWallet.name}
      balance={selectedWallet.balance}
      canDelete={wallets.length > 1}
      on:delete={handleDelete}
      on:cancel={() => showDeleteModal = false}
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
    border-bottom: 1px solid var(--color-surface)1600;
  }

  .menu-item svg {
    color: var(--color-text-secondary);
  }
</style>