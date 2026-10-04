<script lang="ts">
  import { onMount } from 'svelte';
  import WelcomeScreen from './lib/components/organisms/WelcomeScreen.svelte';
  import SeedPhraseDisplay from './lib/components/organisms/SeedPhraseDisplay.svelte';
  import ConfirmSeedPhrase from './lib/components/organisms/ConfirmSeedPhrase.svelte';
  import PasswordCreation from './lib/components/organisms/PasswordCreation.svelte';
  import WalletDashboard from './lib/components/organisms/WalletDashboard.svelte';
  import WalletManagement from './lib/components/organisms/WalletManagement.svelte';
  import SendTransaction from './lib/components/organisms/SendTransaction.svelte';
  import TransactionHistory from './lib/components/organisms/TransactionHistory.svelte';
  import Settings from './lib/components/organisms/Settings.svelte';
  import LockScreen from './lib/components/organisms/LockScreen.svelte';
  import ImportWallet from './lib/components/organisms/ImportWallet.svelte';
  import AddressBook from './lib/components/organisms/AddressBook.svelte';
  import ConfirmTransaction from './lib/components/organisms/ConfirmTransaction.svelte';
  import TransactionFailed from './lib/components/organisms/TransactionFailed.svelte';
  import ReceiveTransaction from './lib/components/organisms/ReceiveTransaction.svelte';
  import WalletCreatedAnimation from './lib/components/organisms/WalletCreatedAnimation.svelte';
  import WalletImportedAnimation from './lib/components/organisms/WalletImportedAnimation.svelte';
  import TransactionDetails from './lib/components/organisms/TransactionDetails.svelte';
  
  type ViewMode = 'all' | 'setup' | 'main' | 'management' | 'settings';
  type ThemeMode = 'light' | 'dark';
  type ScreenMode = 'welcome' | 'wallet-dashboard' | 'wallet-management' | 'settings' | 'send' | 'receive' | 'transaction-details';

  let selectedView: ViewMode = 'all';
  let selectedTheme: ThemeMode = 'light';
  let currentScreen: ScreenMode = 'wallet-dashboard';

  function playDemo() {
    const url = `/demo?theme=${selectedTheme}`;
    window.open(url, '_blank');
  }

  function handleNavigation(detail: any) {
    if (typeof detail === 'string') {
      currentScreen = detail as ScreenMode;
    } else if (detail && detail.route) {
      currentScreen = detail.route as ScreenMode;
    }
    console.log('Navigate to:', currentScreen);
  }
  
  let mockSeedPhrase = [
    'apple', 'bridge', 'cancel', 'danger',
    'energy', 'forest', 'garden', 'harvest',
    'island', 'jungle', 'kitchen', 'laptop'
  ];
  
  let mockTransactions = [
    {
      id: 'tx-1',
      type: 'send' as const,
      amount: 0.05,
      address: 'bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq',
      date: new Date(Date.now() - 3600000),
      status: 'confirmed' as const,
      confirmations: 12,
      txHash: 'abc123...',
      fee: 0.0001
    },
    {
      id: 'tx-2',
      type: 'receive' as const,
      amount: 0.1,
      address: 'bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4',
      date: new Date(Date.now() - 7200000),
      status: 'confirmed' as const,
      confirmations: 24,
      txHash: 'def456...',
      fee: 0.0001
    },
    {
      id: 'tx-3',
      type: 'send' as const,
      amount: 0.025,
      address: 'bc1q5xvftzgd8a4x8f6g3k4jw5a2j4k5j6k5j6k5j6',
      date: new Date(Date.now() - 86400000),
      status: 'pending' as const,
      confirmations: 2,
      txHash: 'ghi789...',
      fee: 0.0002
    }
  ];
</script>

<div class="showcase-container">
  <header class="showcase-header">
    <h1>Fletch Wallet - Chrome Extension Design</h1>
    <p class="subtitle">Complete User Flow Mockup - Svelte Component Architecture</p>
    
    <div class="controls-container">
      <!-- Theme Selector -->
      <div class="selector-group">
        <label for="theme-select" class="selector-label">Theme:</label>
        <select id="theme-select" class="dropdown-select" bind:value={selectedTheme}>
          <option value="light">☀️ Light</option>
          <option value="dark">🌙 Dark</option>
        </select>
      </div>

      <!-- View Selector -->
      <div class="selector-group">
        <label for="view-select" class="selector-label">View:</label>
        <select id="view-select" class="dropdown-select" bind:value={selectedView}>
          <option value="all">All Screens</option>
          <option value="setup">Initial Setup</option>
          <option value="main">Main Interface</option>
          <option value="management">Wallet Management</option>
          <option value="settings">Settings & Security</option>
        </select>
      </div>

      <!-- Play Demo Button -->
      <div class="selector-group">
        <button class="play-demo-btn" on:click={playDemo}>
          ▶️ Play Demo
        </button>
      </div>
    </div>
  </header>
  
  {#if selectedView === 'all' || selectedView === 'setup'}
  <section class="showcase-section">
    <h2 class="section-title">Initial Setup Flow</h2>
    <div class="screens-row">
      <div class="screen-wrapper">
        <h3 class="screen-title">Welcome Screen</h3>
        <p class="screen-description">Entry point for new and existing users</p>
        <div class="screen" data-theme={selectedTheme}>
          <WelcomeScreen 
            onCreateWallet={() => console.log('Create wallet')}
            onImportWallet={() => console.log('Import wallet')}
          />
        </div>
      </div>
      
      <div class="screen-wrapper">
        <h3 class="screen-title">Create New Wallet</h3>
        <p class="screen-description">Generated 12-word seed phrase display</p>
        <div class="screen" data-theme={selectedTheme}>
          <SeedPhraseDisplay
            seedPhrase={mockSeedPhrase}
            revealed={true}
            onContinue={() => console.log('Continue')}
            onBack={() => console.log('Back clicked')}
          />
        </div>
      </div>
      
      <div class="screen-wrapper">
        <h3 class="screen-title">Confirm Seed Phrase</h3>
        <p class="screen-description">Verification of seed phrase backup</p>
        <div class="screen" data-theme={selectedTheme}>
          <ConfirmSeedPhrase 
            seedPhrase={mockSeedPhrase}
            onConfirm={() => console.log('Confirmed')}
            onBack={() => console.log('Back')}
          />
        </div>
      </div>
      
      <div class="screen-wrapper">
        <h3 class="screen-title">Set Password</h3>
        <p class="screen-description">Secure the wallet with password</p>
        <div class="screen" data-theme={selectedTheme}>
          <PasswordCreation 
            onCreateWallet={() => console.log('Create wallet')}
            onBack={() => console.log('Back')}
          />
        </div>
      </div>
      
      <div class="screen-wrapper">
        <h3 class="screen-title">Import Wallet</h3>
        <p class="screen-description">Restore existing wallet from seed phrase</p>
        <div class="screen" data-theme={selectedTheme}>
          <ImportWallet
            onImport={() => console.log('Import wallet')}
            onBack={() => console.log('Back')}
            showHeader={true}
          />
        </div>
      </div>

      <div class="screen-wrapper">
        <h3 class="screen-title">Wallet Created Animation</h3>
        <p class="screen-description">Auto-dismissing success animation (2.5s)</p>
        <div class="screen" data-theme={selectedTheme} style="position: relative; overflow: hidden;">
          <WalletCreatedAnimation
            onComplete={() => console.log('Animation complete')}
          />
        </div>
      </div>

      <div class="screen-wrapper">
        <h3 class="screen-title">Wallet Imported Animation</h3>
        <p class="screen-description">Auto-dismissing import animation (2.5s)</p>
        <div class="screen" data-theme={selectedTheme} style="position: relative; overflow: hidden;">
          <WalletImportedAnimation
            onComplete={() => console.log('Animation complete')}
          />
        </div>
      </div>
    </div>
  </section>
  {/if}
  
  {#if selectedView === 'all' || selectedView === 'main'}
  <section class="showcase-section">
    <h2 class="section-title">Main Wallet Interface</h2>
    <div class="screens-row">
      <div class="screen-wrapper">
        <h3 class="screen-title">Dashboard</h3>
        <p class="screen-description">Main wallet view with balance and quick actions</p>
        <div class="screen" data-theme={selectedTheme}>
          <WalletDashboard
            balance={1234.56}
            noteCount={12}
            walletName="Main Wallet"
            recentTransactions={mockTransactions.slice(0, 3)}
            onSettings={() => currentScreen = 'settings'}
            onManageWallets={() => currentScreen = 'wallet-management'}
            on:navigate={(e) => handleNavigation(e.detail)}
          />
        </div>
      </div>
      
      <div class="screen-wrapper">
        <h3 class="screen-title">Send Transaction</h3>
        <p class="screen-description">Send Nockchain with fee selection</p>
        <div class="screen" data-theme={selectedTheme}>
          <SendTransaction
            balance={0.12345678}
            onSend={(data) => console.log('Send', data)}
            onCancel={() => console.log('Cancel')}
          />
        </div>
      </div>

      <div class="screen-wrapper">
        <h3 class="screen-title">Receive Transaction</h3>
        <p class="screen-description">Receive Nockchain with QR code and address</p>
        <div class="screen" data-theme={selectedTheme}>
          <ReceiveTransaction
            address="nc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq"
            onBack={() => console.log('Back')}
            showHeader={true}
          />
        </div>
      </div>

      <div class="screen-wrapper">
        <h3 class="screen-title">Transaction History</h3>
        <p class="screen-description">Complete transaction records with filtering</p>
        <div class="screen" data-theme={selectedTheme}>
          <TransactionHistory 
            transactions={mockTransactions}
          />
        </div>
      </div>
      
      <div class="screen-wrapper">
        <h3 class="screen-title">Confirm Transaction</h3>
        <p class="screen-description">Transaction review before sending</p>
        <div class="screen" data-theme={selectedTheme}>
          <ConfirmTransaction 
            recipientName="Alice"
            recipientAddress="nock1qvk8x9y7...ht4k3n2p5m"
            amount={50.00}
            fee={0.02}
            onConfirm={() => console.log('Confirmed')}
            onCancel={() => console.log('Cancelled')}
            onBack={() => console.log('Back')}
          />
        </div>
      </div>
      
      <div class="screen-wrapper">
        <h3 class="screen-title">Transaction Failed</h3>
        <p class="screen-description">Error handling for failed transactions</p>
        <div class="screen" data-theme={selectedTheme}>
          <TransactionFailed
            errorMessage="Insufficient balance. You need 50.02 but only have 45.00 available."
            onTryAgain={() => console.log('Try again')}
            onGoBack={() => console.log('Go back')}
            onBack={() => console.log('Back')}
          />
        </div>
      </div>

      <div class="screen-wrapper">
        <h3 class="screen-title">Transaction Details</h3>
        <p class="screen-description">Detailed view of a single transaction</p>
        <div class="screen" data-theme={selectedTheme}>
          <TransactionDetails
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
        </div>
      </div>
    </div>
  </section>
  {/if}
  
  {#if selectedView === 'all' || selectedView === 'management'}
  <section class="showcase-section">
    <h2 class="section-title">Wallet Management</h2>
    <div class="screens-row">
      <div class="screen-wrapper">
        <h3 class="screen-title">Manage Wallets</h3>
        <p class="screen-description">Create, import, rename and delete wallets</p>
        <div class="screen" data-theme={selectedTheme}>
          <WalletManagement
            onBack={() => console.log('Back')}
          />
        </div>
      </div>

      <div class="screen-wrapper">
        <h3 class="screen-title">Address Book</h3>
        <p class="screen-description">Manage saved wallet addresses</p>
        <div class="screen" data-theme={selectedTheme}>
          <AddressBook
            onAddContact={() => console.log('Add contact')}
            onSelectContact={(contact) => console.log('Selected:', contact)}
          />
        </div>
      </div>
    </div>
  </section>
  {/if}
  
  {#if selectedView === 'all' || selectedView === 'settings'}
  <section class="showcase-section">
    <h2 class="section-title">Settings & Security</h2>
    <div class="screens-row">
      <div class="screen-wrapper">
        <h3 class="screen-title">Settings</h3>
        <p class="screen-description">Wallet configuration and security</p>
        <div class="screen" data-theme={selectedTheme}>
          <Settings
            onLogout={() => console.log('Logout')}
            onExportSeed={() => console.log('Export seed')}
            onImportWallet={() => console.log('Import wallet')}
            onAddressBook={() => console.log('Open address book')}
            onManageWallets={() => console.log('Manage wallets')}
          />
        </div>
      </div>
      
      <div class="screen-wrapper">
        <h3 class="screen-title">Locked Wallet</h3>
        <p class="screen-description">Security lock screen</p>
        <div class="screen" data-theme={selectedTheme}>
          <LockScreen 
            onUnlock={(password) => console.log('Unlock with:', password)}
            onForgotPassword={() => console.log('Forgot password')}
          />
        </div>
      </div>
    </div>
  </section>
  {/if}
  
  {#if selectedView === 'all'}
  <section class="showcase-section">
    <h2 class="section-title">Component Architecture</h2>
    <div class="architecture-grid">
      <div class="architecture-card">
        <h3>Atomic Components</h3>
        <ul>
          <li>Button - Multiple variants</li>
          <li>Input - With validation</li>
          <li>Card - Container component</li>
          <li>Icon - SVG icon library</li>
          <li>Logo - Brand component</li>
        </ul>
      </div>
      
      <div class="architecture-card">
        <h3>Molecular Components</h3>
        <ul>
          <li>Header - Navigation header</li>
          <li>TabBar - Bottom navigation</li>
          <li>TransactionItem - Transaction display</li>
          <li>WalletCard - Wallet selector</li>
          <li>FeeSelector - Fee options</li>
          <li>SeedWord - Seed phrase word</li>
        </ul>
      </div>
      
      <div class="architecture-card">
        <h3>Organism Components</h3>
        <ul>
          <li>WelcomeScreen - Onboarding</li>
          <li>SeedPhraseDisplay - Seed management</li>
          <li>WalletDashboard - Main interface</li>
          <li>SendTransaction - Send flow</li>
          <li>TransactionHistory - History view</li>
          <li>Settings - Preferences</li>
        </ul>
      </div>
    </div>
  </section>
  {/if}
</div>

<style>
  :global(html), :global(body) {
    margin: 0;
    padding: 0;
    height: auto !important;
    overflow: auto !important;
  }
  
  :global(body) {
    background: linear-gradient(135deg, #f5f5f5 0%, #e0e0e0 100%);
    min-height: 100vh;
  }
  
  :global(#app) {
    height: auto !important;
  }
  
  .showcase-container {
    width: 100%;
    max-width: 1400px;
    margin: 0 auto;
    padding: 40px 20px;
  }
  
  .showcase-header {
    text-align: center;
    margin-bottom: 60px;
  }
  
  .showcase-header h1 {
    font-size: 36px;
    font-weight: 700;
    color: #000;
    margin-bottom: 20px;
  }
  
  .subtitle {
    font-size: 16px;
    color: #666;
    margin-bottom: 30px;
  }
  
  .controls-container {
    display: flex;
    flex-direction: column;
    gap: 20px;
    margin: 30px 0;
    padding: 20px;
    background: rgba(255, 255, 255, 0.9);
    border-radius: 12px;
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.1);
  }
  
  .selector-group {
    display: flex;
    align-items: center;
    gap: 15px;
    flex-wrap: wrap;
    justify-content: center;
  }
  
  .selector-label {
    font-weight: 600;
    font-size: 14px;
    color: #333;
    min-width: 60px;
  }
  
  .dropdown-select {
    padding: 10px 16px;
    background: white;
    border: 2px solid #e0e0e0;
    border-radius: 8px;
    font-size: 14px;
    font-weight: 500;
    color: #333;
    cursor: pointer;
    transition: all 0.2s ease;
    min-width: 200px;
  }

  .dropdown-select:hover {
    border-color: #999;
  }

  .dropdown-select:focus {
    outline: none;
    border-color: #000;
    box-shadow: 0 0 0 3px rgba(0, 0, 0, 0.1);
  }

  .play-demo-btn {
    padding: 12px 24px;
    background: #000;
    color: white;
    border: none;
    border-radius: 8px;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .play-demo-btn:hover {
    background: #333;
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
  }
  
  .showcase-section {
    margin-bottom: 80px;
  }
  
  .section-title {
    font-size: 24px;
    font-weight: 600;
    color: #000;
    margin: 60px 0 30px;
    padding-left: 20px;
    border-left: 4px solid #000;
  }
  
  .screens-row {
    display: flex;
    gap: 30px;
    margin-bottom: 40px;
    flex-wrap: wrap;
    justify-content: center;
  }
  
  .screen-wrapper {
    text-align: center;
  }
  
  .screen-title {
    font-size: 14px;
    font-weight: 600;
    color: #333;
    margin-bottom: 10px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  
  .screen-description {
    font-size: 12px;
    color: #666;
    margin-bottom: 15px;
    max-width: 360px;
  }
  
  .screen {
    width: 360px;
    height: 600px;
    background: var(--color-background, #ffffff);
    border-radius: 16px;
    box-shadow: 0 10px 40px rgba(0, 0, 0, 0.1);
    overflow: hidden;
    position: relative;
  }
  
  .architecture-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 30px;
    margin-top: 40px;
  }
  
  .architecture-card {
    background: white;
    border-radius: 12px;
    padding: 30px;
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.1);
  }
  
  .architecture-card h3 {
    font-size: 18px;
    font-weight: 600;
    color: #000;
    margin-bottom: 20px;
  }
  
  .architecture-card ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  
  .architecture-card li {
    padding: 8px 0;
    color: #666;
    font-size: 14px;
    border-bottom: 1px solid #f0f0f0;
  }
  
  .architecture-card li:last-child {
    border-bottom: none;
  }
  
  @media (max-width: 768px) {
    .screens-row {
      flex-direction: column;
      align-items: center;
    }
    
    .architecture-grid {
      grid-template-columns: 1fr;
    }
  }
</style>