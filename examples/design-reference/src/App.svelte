<script lang="ts">
  import { router } from './lib/stores/router';
  import type { Route } from './lib/stores/router';
  import { walletStore, activeWallet } from './lib/stores/wallet';
  import { transactionStore } from './lib/stores/transactions';
  
  import WelcomeScreen from './lib/components/organisms/WelcomeScreen.svelte';
  import SeedPhraseDisplay from './lib/components/organisms/SeedPhraseDisplay.svelte';
  import ConfirmSeedPhrase from './lib/components/organisms/ConfirmSeedPhrase.svelte';
  import PasswordCreation from './lib/components/organisms/PasswordCreation.svelte';
  import WalletCreatedAnimation from './lib/components/organisms/WalletCreatedAnimation.svelte';
  import WalletImportedAnimation from './lib/components/organisms/WalletImportedAnimation.svelte';
  import WalletDashboard from './lib/components/organisms/WalletDashboard.svelte';
  import WalletManagement from './lib/components/organisms/WalletManagement.svelte';
  import SendTransaction from './lib/components/organisms/SendTransaction.svelte';
  import ReceiveTransaction from './lib/components/organisms/ReceiveTransaction.svelte';
  import TransactionHistory from './lib/components/organisms/TransactionHistory.svelte';
  import Settings from './lib/components/organisms/Settings.svelte';
  import ImportWallet from './lib/components/organisms/ImportWallet.svelte';
  import AddressBook from './lib/components/organisms/AddressBook.svelte';
  import LockScreen from './lib/components/organisms/LockScreen.svelte';
  import ConfirmTransaction from './lib/components/organisms/ConfirmTransaction.svelte';
  import TransactionFailed from './lib/components/organisms/TransactionFailed.svelte';
  import TransactionDetails from './lib/components/organisms/TransactionDetails.svelte';
  
  let currentRoute: Route = 'welcome';
  let routeData: any = null;

  router.subscribe(state => {
    currentRoute = state.currentRoute;
    routeData = state.routeData;
  });
  
  function handleCreateWallet() {
    const seedPhrase = generateMockSeedPhrase();
    walletStore.createWallet({
      name: 'My Wallet',
      address: `bc1q${Math.random().toString(36).substr(2, 15)}`,
      balance: 0.12345678,
      seedPhrase,
      network: 'mainnet'
    });
    router.navigate('seed-phrase');
  }
  
  function handleImportWallet() {
    console.log('Import wallet flow');
    router.navigate('dashboard');
  }
  
  function handleSeedPhraseConfirm() {
    router.navigate('dashboard');
  }
  
  function handleSendTransaction(data: any) {
    console.log('Sending transaction:', data);
    transactionStore.addTransaction({
      walletId: $activeWallet?.id || '',
      type: 'send',
      amount: data.amount,
      address: data.recipient,
      date: new Date(),
      status: 'pending',
      confirmations: 0,
      txHash: `${Math.random().toString(36).substr(2, 64)}`,
      fee: data.fee
    });
    router.navigate('dashboard');
  }
  
  function handleLogout() {
    walletStore.logout();
    transactionStore.clearTransactions();
    router.reset();
  }
  
  function handleExportSeed() {
    if ($activeWallet) {
      console.log('Exporting seed phrase:', $activeWallet.seedPhrase);
      alert('Seed phrase: ' + $activeWallet.seedPhrase.join(' '));
    }
  }
  
  function handleTabChange(tab: any) {
    if (tab.id === 'wallet') {
      router.navigate('dashboard');
    } else if (tab.id === 'send') {
      router.navigate('send');
    } else if (tab.id === 'history') {
      router.navigate('history');
    } else if (tab.id === 'settings') {
      router.navigate('settings');
    }
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

<main class="app">
  {#if currentRoute === 'welcome'}
    <WelcomeScreen
      onCreateWallet={handleCreateWallet}
      onImportWallet={() => router.navigate('import-wallet')}
    />
  {:else if currentRoute === 'seed-phrase'}
    <SeedPhraseDisplay
      seedPhrase={$activeWallet?.seedPhrase || []}
      onContinue={() => router.navigate('confirm-seed')}
      onBack={() => router.back()}
    />
  {:else if currentRoute === 'confirm-seed'}
    <ConfirmSeedPhrase
      seedPhrase={$activeWallet?.seedPhrase || []}
      onConfirm={() => router.navigate('password-creation')}
      onBack={() => router.back()}
    />
  {:else if currentRoute === 'password-creation'}
    <PasswordCreation
      onCreateWallet={() => router.navigate('wallet-created')}
      onBack={() => router.back()}
    />
  {:else if currentRoute === 'wallet-created'}
    <WalletCreatedAnimation
      onComplete={() => router.navigate('dashboard')}
    />
  {:else if currentRoute === 'import-wallet'}
    <ImportWallet
      onImport={() => router.navigate('wallet-imported')}
      onBack={() => router.back()}
    />
  {:else if currentRoute === 'wallet-imported'}
    <WalletImportedAnimation
      onComplete={() => router.navigate('dashboard')}
    />
  {:else if currentRoute === 'lock-screen'}
    <LockScreen
      onUnlock={() => router.navigate('dashboard')}
      onForgotPassword={() => router.navigate('import-wallet')}
    />
  {:else if currentRoute === 'dashboard'}
    <WalletDashboard
      balance={$activeWallet?.balance || 0}
      walletName={$activeWallet?.name || 'Wallet'}
      on:navigate={(e) => {
        if (e.detail.route === 'transaction-details') {
          router.navigate('transaction-details', e.detail.data);
        } else {
          router.navigate(e.detail);
        }
      }}
      onSettings={() => router.navigate('settings')}
      onManageWallets={() => router.navigate('wallet-management')}
    />
  {:else if currentRoute === 'wallet-management'}
    <WalletManagement
      onBack={() => router.back()}
    />
  {:else if currentRoute === 'wallet-created'}
    <WalletCreatedAnimation
      onComplete={() => router.navigate('dashboard')}
    />
  {:else if currentRoute === 'wallet-imported'}
    <WalletImportedAnimation
      onComplete={() => router.navigate('dashboard')}
    />
  {:else if currentRoute === 'send'}
    <SendTransaction
      balance={$activeWallet?.balance || 0}
      onSend={() => router.navigate('confirm-transaction')}
      onCancel={() => router.back()}
    />
  {:else if currentRoute === 'receive'}
    <ReceiveTransaction
      address={$activeWallet?.address || ''}
      onBack={() => router.back()}
    />
  {:else if currentRoute === 'confirm-transaction'}
    <ConfirmTransaction
      onConfirm={() => router.navigate('dashboard')}
      onCancel={() => router.back()}
      onBack={() => router.back()}
    />
  {:else if currentRoute === 'transaction-failed'}
    <TransactionFailed
      onTryAgain={() => router.navigate('send')}
      onGoBack={() => router.navigate('dashboard')}
      onBack={() => router.back()}
    />
  {:else if currentRoute === 'history'}
    <TransactionHistory
      transactions={$activeWallet ? transactionStore.getTransactionsByWallet($activeWallet.id) : []}
      onBack={() => router.back()}
    />
  {:else if currentRoute === 'settings'}
    <Settings
      onLogout={handleLogout}
      onExportSeed={handleExportSeed}
      onImportWallet={() => router.navigate('import-wallet')}
      onAddressBook={() => router.navigate('address-book')}
      onManageWallets={() => router.navigate('wallet-management')}
      onBack={() => router.back()}
    />
  {:else if currentRoute === 'address-book'}
    <AddressBook
      onSelectContact={(contact) => {
        console.log('Selected contact:', contact);
        router.navigate('send');
      }}
      onBack={() => router.back()}
    />
  {:else if currentRoute === 'transaction-details'}
    <TransactionDetails
      type={routeData?.type || 'sent'}
      amount={routeData?.amount || 0}
      fee={routeData?.fee || 0}
      from={routeData?.type === 'received' ? routeData?.address : $activeWallet?.address || ''}
      to={routeData?.type === 'sent' ? routeData?.address : $activeWallet?.address || ''}
      txHash={'7f83b1657ff1fc53b92dc18148a1d65dfc2d4b1fa3d677284addd200126d9069'}
      timestamp={new Date().toLocaleString()}
      confirmations={routeData?.confirmations || 0}
      blockHeight={823456}
      gasUsed={21000}
      gasLimit={30000}
      status={routeData?.status || 'confirmed'}
      onBack={() => router.back()}
    />
  {/if}
</main>

<style>
  .app {
    width: 100%;
    height: 100%;
    background: var(--color-background);
    border-radius: var(--radius-xl);
    overflow: hidden;
    box-shadow: 0 25px 50px -12px rgba(0, 0, 0, 0.25);
    display: flex;
    flex-direction: column;
  }
  
  @media (max-width: 400px) {
    .app {
      border-radius: 0;
      box-shadow: none;
    }
  }
</style>