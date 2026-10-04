<script lang="ts">
  import { onMount } from 'svelte';
  import App from './App.svelte';

  // Import all screens
  import WelcomeScreen from './lib/components/organisms/WelcomeScreen.svelte';
  import SeedPhraseDisplay from './lib/components/organisms/SeedPhraseDisplay.svelte';
  import ConfirmSeedPhrase from './lib/components/organisms/ConfirmSeedPhrase.svelte';
  import PasswordCreation from './lib/components/organisms/PasswordCreation.svelte';
  import WalletCreatedAnimation from './lib/components/organisms/WalletCreatedAnimation.svelte';
  import ImportWallet from './lib/components/organisms/ImportWallet.svelte';
  import WalletImportedAnimation from './lib/components/organisms/WalletImportedAnimation.svelte';
  import WalletDashboard from './lib/components/organisms/WalletDashboard.svelte';
  import WalletManagement from './lib/components/organisms/WalletManagement.svelte';
  import SendTransaction from './lib/components/organisms/SendTransaction.svelte';
  import ReceiveTransaction from './lib/components/organisms/ReceiveTransaction.svelte';
  import TransactionHistory from './lib/components/organisms/TransactionHistory.svelte';
  import ConfirmTransaction from './lib/components/organisms/ConfirmTransaction.svelte';
  import TransactionFailed from './lib/components/organisms/TransactionFailed.svelte';
  import AddressBook from './lib/components/organisms/AddressBook.svelte';
  import Settings from './lib/components/organisms/Settings.svelte';
  import LockScreen from './lib/components/organisms/LockScreen.svelte';
  import TransactionDetails from './lib/components/organisms/TransactionDetails.svelte';

  let theme = 'light';
  let selectedScreen = 'app';
  let showSelector = true;

  const screens = [
    { value: 'app', label: 'Full App Flow', component: App },
    { value: 'welcome', label: 'Welcome Screen', component: WelcomeScreen },
    { value: 'seedPhrase', label: 'Seed Phrase Display', component: SeedPhraseDisplay },
    { value: 'confirmSeed', label: 'Confirm Seed Phrase', component: ConfirmSeedPhrase },
    { value: 'password', label: 'Set Password', component: PasswordCreation },
    { value: 'walletCreated', label: 'Wallet Created Animation', component: WalletCreatedAnimation },
    { value: 'import', label: 'Import Wallet', component: ImportWallet },
    { value: 'walletImported', label: 'Wallet Imported Animation', component: WalletImportedAnimation },
    { value: 'dashboard', label: 'Wallet Dashboard', component: WalletDashboard },
    { value: 'walletManagement', label: 'Wallet Management', component: WalletManagement },
    { value: 'send', label: 'Send Transaction', component: SendTransaction },
    { value: 'receive', label: 'Receive Transaction', component: ReceiveTransaction },
    { value: 'history', label: 'Transaction History', component: TransactionHistory },
    { value: 'confirm', label: 'Confirm Transaction', component: ConfirmTransaction },
    { value: 'failed', label: 'Transaction Failed', component: TransactionFailed },
    { value: 'addressBook', label: 'Address Book', component: AddressBook },
    { value: 'settings', label: 'Settings', component: Settings },
    { value: 'lock', label: 'Lock Screen', component: LockScreen },
    { value: 'transactionDetails', label: 'Transaction Details', component: TransactionDetails }
  ];

  // Mock data for screens that need it
  const mockSeedPhrase = [
    'apple', 'bridge', 'cancel', 'danger',
    'energy', 'forest', 'garden', 'harvest',
    'island', 'jungle', 'kitchen', 'laptop'
  ];

  const mockTransactions = [
    {
      id: 'tx-1',
      type: 'send' as const,
      amount: 50.00,
      date: '2 hours ago',
      address: 'nc1q...',
      status: 'confirmed' as const
    },
    {
      id: 'tx-2',
      type: 'receive' as const,
      amount: 125.50,
      date: '1 day ago',
      address: 'nc1q...',
      status: 'confirmed' as const
    }
  ];

  $: currentScreen = screens.find(s => s.value === selectedScreen);

  onMount(() => {
    const params = new URLSearchParams(window.location.search);
    theme = params.get('theme') || 'light';
    const screenParam = params.get('screen');
    if (screenParam && screens.find(s => s.value === screenParam)) {
      selectedScreen = screenParam;
    }
  });

  function handleKeyPress(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      showSelector = !showSelector;
    }
  }
</script>

<svelte:window on:keydown={handleKeyPress} />

<div class="demo-wrapper">
  {#if showSelector}
    <div class="demo-controls">
      <select
        class="screen-selector"
        bind:value={selectedScreen}
      >
        {#each screens as screen}
          <option value={screen.value}>{screen.label}</option>
        {/each}
      </select>
      <button
        class="hide-button"
        on:click={() => showSelector = false}
        title="Press ESC to toggle"
      >
        Hide
      </button>
    </div>
  {/if}

  <div class="demo-container" data-theme={theme}>
    {#if currentScreen}
      {#if currentScreen.value === 'app'}
        <svelte:component this={currentScreen.component} />
      {:else if currentScreen.value === 'seedPhrase'}
        <svelte:component
          this={currentScreen.component}
          seedPhrase={mockSeedPhrase}
          onContinue={() => console.log('Continue')}
          onBack={() => console.log('Back')}
        />
      {:else if currentScreen.value === 'confirmSeed'}
        <svelte:component
          this={currentScreen.component}
          seedPhrase={mockSeedPhrase}
          onConfirm={() => console.log('Confirm')}
          onBack={() => console.log('Back')}
        />
      {:else if currentScreen.value === 'dashboard'}
        <svelte:component
          this={currentScreen.component}
          balance={0.12}
          noteCount={12}
          walletName="Main Wallet"
          recentTransactions={mockTransactions}
          onSettings={() => console.log('Settings')}
          onManageWallets={() => console.log('Manage Wallets')}
        />
      {:else if currentScreen.value === 'walletManagement'}
        <svelte:component
          this={currentScreen.component}
          onBack={() => console.log('Back')}
        />
      {:else if currentScreen.value === 'history'}
        <svelte:component
          this={currentScreen.component}
          transactions={mockTransactions}
        />
      {:else if currentScreen.value === 'receive'}
        <svelte:component
          this={currentScreen.component}
          address="nc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq"
          onBack={() => console.log('Back')}
          showHeader={true}
        />
      {:else if currentScreen.value === 'confirm'}
        <svelte:component
          this={currentScreen.component}
          recipient="nc1q5xvftzgd8a4x8f6g3k4jw5a2j4k5j6k5j6k5j6"
          amount={0.025}
          fee={0.0002}
          total={0.0252}
          onConfirm={() => console.log('Confirm')}
          onCancel={() => console.log('Cancel')}
        />
      {:else if currentScreen.value === 'failed'}
        <svelte:component
          this={currentScreen.component}
          error="Insufficient funds"
          onRetry={() => console.log('Retry')}
          onCancel={() => console.log('Cancel')}
        />
      {:else if currentScreen.value === 'walletCreated' || currentScreen.value === 'walletImported'}
        <svelte:component
          this={currentScreen.component}
          onComplete={() => console.log('Animation complete')}
        />
      {:else if currentScreen.value === 'transactionDetails'}
        <svelte:component
          this={currentScreen.component}
          type="sent"
          amount={50.00}
          fee={0.02}
          from="nc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq"
          to="nc1q5xvftzgd8a4x8f6g3k4jw5a2j4k5j6k5j6k5j6"
          txHash="7f83b1657ff1fc53b92dc18148a1d65dfc2d4b1fa3d677284addd200126d9069"
          timestamp="2024-01-15 14:32:05"
          confirmations={12}
          blockHeight={823456}
          gasUsed={21000}
          gasLimit={30000}
          status="confirmed"
          onBack={() => console.log('Back')}
        />
      {:else}
        <svelte:component
          this={currentScreen.component}
          onBack={() => console.log('Back')}
          onContinue={() => console.log('Continue')}
          onCreateWallet={() => console.log('Create')}
          onImportWallet={() => console.log('Import')}
          onImport={() => console.log('Import')}
          onUnlock={() => console.log('Unlock')}
          onSend={() => console.log('Send')}
          onCancel={() => console.log('Cancel')}
          onSettings={() => console.log('Settings')}
        />
      {/if}
    {/if}
  </div>

  {#if !showSelector}
    <div class="show-hint">Press ESC to show controls</div>
  {/if}
</div>

<style>
  :global(html), :global(body) {
    margin: 0;
    padding: 0;
    height: 100vh;
    overflow: hidden;
    background: #f5f5f5;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  :global(#app) {
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .demo-wrapper {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 20px;
  }

  .demo-controls {
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 12px 20px;
    background: white;
    border-radius: 12px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
  }

  .screen-selector {
    padding: 8px 12px;
    border: 1.5px solid #e5e7eb;
    border-radius: 8px;
    background: #ffffff;
    font-size: 14px;
    font-weight: 500;
    color: #000;
    cursor: pointer;
    min-width: 200px;
    outline: none;
    transition: all 0.15s ease;
  }

  .screen-selector:hover {
    border-color: #d1d5db;
  }

  .screen-selector:focus {
    border-color: #000;
  }

  .hide-button {
    padding: 8px 16px;
    background: #f3f4f6;
    border: none;
    border-radius: 8px;
    font-size: 14px;
    font-weight: 500;
    color: #666;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .hide-button:hover {
    background: #e5e7eb;
    color: #000;
  }

  .demo-container {
    width: 360px;
    height: 600px;
    background: #ffffff;
    border-radius: 16px;
    overflow: hidden;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.15);
    position: relative;
  }

  .show-hint {
    position: absolute;
    bottom: -40px;
    font-size: 13px;
    color: #999;
    animation: fadeIn 0.3s ease;
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
      transform: translateY(10px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @media (max-width: 400px) {
    .demo-container {
      width: 100vw;
      height: 100vh;
      border-radius: 0;
    }

    .demo-controls {
      width: 90%;
      justify-content: space-between;
    }

    .screen-selector {
      min-width: 0;
      flex: 1;
    }
  }
</style>