<script>
  import { wallets, activeWallet, currentView, lockVault } from '../lib/stores/wallet';
  import { deleteWallet } from '../lib/stores/wallet';

  async function selectWallet(wallet) {
    activeWallet.set(wallet);
    currentView.set('dashboard');
  }

  async function handleDeleteWallet(wallet) {
    if (
      confirm(
        `Are you sure you want to delete wallet "${wallet.name}"? This action cannot be undone.`
      )
    ) {
      await deleteWallet(wallet.id);
    }
  }

  function handleCreateWallet() {
    currentView.set('create');
  }

  function handleImportWallet() {
    currentView.set('import');
  }

  function handleLock() {
    lockVault();
  }
</script>

<div class="wallet-vault">
  <div class="vault-header">
    <h2>Your Wallets</h2>
    <button class="btn-icon" on:click={handleLock} title="Lock Vault">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
        <path d="M7 11V7a5 5 0 0110 0v4" />
      </svg>
    </button>
  </div>

  {#if $wallets.length === 0}
    <div class="empty-state">
      <svg
        class="empty-icon"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
      >
        <path d="M21 12V7a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2h7" />
        <path d="M3 10h18" />
        <path d="M16 19l2 2 4-4" />
      </svg>
      <h3>No wallets yet</h3>
      <p>Create or import your first wallet to get started</p>
    </div>
  {:else}
    <div class="wallet-list">
      {#each $wallets as wallet}
        <div class="wallet-card" on:click={() => selectWallet(wallet)}>
          <div class="wallet-info">
            <h3>{wallet.name}</h3>
            <p class="wallet-address">
              {wallet.addresses[0] ? wallet.addresses[0].substring(0, 10) + '...' : 'No address'}
            </p>
            <p class="wallet-date">
              Last used: {new Date(wallet.lastUsed).toLocaleDateString()}
            </p>
          </div>
          <button
            class="btn-delete"
            on:click|stopPropagation={() => handleDeleteWallet(wallet)}
            title="Delete wallet"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path
                d="M3 6h18M8 6V4a2 2 0 012-2h4a2 2 0 012 2v2m3 0v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6h14zM10 11v6M14 11v6"
              />
            </svg>
          </button>
        </div>
      {/each}
    </div>
  {/if}

  <div class="vault-actions">
    <button class="btn-primary" on:click={handleCreateWallet}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M12 5v14M5 12h14" />
      </svg>
      Create New Wallet
    </button>
    <button class="btn-secondary" on:click={handleImportWallet}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4M7 10l5 5 5-5M12 15V3" />
      </svg>
      Import Wallet
    </button>
  </div>
</div>

<style>
  .wallet-vault {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .vault-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1.5rem;
  }

  .vault-header h2 {
    margin: 0;
    font-size: 1.25rem;
    background: linear-gradient(135deg, #4fc3f7, #29b6f6);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
  }

  .btn-icon {
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 8px;
    padding: 0.5rem;
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    transition: all 0.2s;
  }

  .btn-icon:hover {
    background: rgba(255, 255, 255, 0.15);
    color: #fff;
  }

  .btn-icon svg {
    width: 20px;
    height: 20px;
  }

  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    padding: 2rem;
  }

  .empty-icon {
    width: 64px;
    height: 64px;
    color: rgba(255, 255, 255, 0.3);
    margin-bottom: 1rem;
  }

  .empty-state h3 {
    margin: 0 0 0.5rem 0;
    font-size: 1.125rem;
    color: rgba(255, 255, 255, 0.9);
  }

  .empty-state p {
    margin: 0;
    color: rgba(255, 255, 255, 0.5);
    font-size: 0.875rem;
  }

  .wallet-list {
    flex: 1;
    overflow-y: auto;
    margin-bottom: 1rem;
  }

  .wallet-card {
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    padding: 1rem;
    margin-bottom: 0.75rem;
    cursor: pointer;
    transition: all 0.2s;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .wallet-card:hover {
    background: rgba(255, 255, 255, 0.08);
    transform: translateX(4px);
  }

  .wallet-info h3 {
    margin: 0 0 0.25rem 0;
    font-size: 1rem;
    color: #fff;
  }

  .wallet-address {
    margin: 0 0 0.25rem 0;
    font-family: monospace;
    font-size: 0.75rem;
    color: rgba(255, 255, 255, 0.5);
  }

  .wallet-date {
    margin: 0;
    font-size: 0.75rem;
    color: rgba(255, 255, 255, 0.4);
  }

  .btn-delete {
    background: rgba(244, 67, 54, 0.1);
    border: 1px solid rgba(244, 67, 54, 0.2);
    border-radius: 6px;
    padding: 0.375rem;
    color: #ff6b6b;
    cursor: pointer;
    transition: all 0.2s;
  }

  .btn-delete:hover {
    background: rgba(244, 67, 54, 0.2);
  }

  .btn-delete svg {
    width: 16px;
    height: 16px;
  }

  .vault-actions {
    display: flex;
    gap: 0.75rem;
    padding-top: 1rem;
    border-top: 1px solid rgba(255, 255, 255, 0.1);
  }

  .btn-primary,
  .btn-secondary {
    flex: 1;
    padding: 0.75rem;
    border-radius: 8px;
    font-size: 0.875rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    border: none;
  }

  .btn-primary {
    background: linear-gradient(135deg, #4fc3f7, #29b6f6);
    color: white;
  }

  .btn-primary:hover {
    transform: translateY(-2px);
    box-shadow: 0 8px 16px rgba(79, 195, 247, 0.3);
  }

  .btn-secondary {
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    color: rgba(255, 255, 255, 0.9);
  }

  .btn-secondary:hover {
    background: rgba(255, 255, 255, 0.15);
  }

  .btn-primary svg,
  .btn-secondary svg {
    width: 18px;
    height: 18px;
  }
</style>
