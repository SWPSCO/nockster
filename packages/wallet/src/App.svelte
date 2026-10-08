<script lang="ts">
  export let allowHardwareWhileLocked = false;
  export let readWalletFile:
    ((file: File) => Promise<{ phrase: string; address: string }[]>) | undefined = undefined;
  import { nextWalletName } from './lib/utils/walletName';
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { router } from './lib/stores/router';
  import type { Route } from './lib/stores/router';
  import { walletStore, activeWallet } from './lib/stores/wallet';
  import type { Wallet } from './lib/types/wallet';
  import { currentTheme } from './lib/stores/theme';
  import { settingsStore } from './lib/stores/settings';
  import { transactionStore } from './lib/stores/transactions';
  // Vault bridge for wallet operations
  import {
    getVaultStatus,
    createVault,
    unlockVaultWithPassword,
    lockVaultSession,
    setPendingWallet,
    getPendingWallet,
    clearPendingWallet,
    importWalletToVault,
    getWalletsFromVault,
    verifyPassword
  } from './lib/utils/vaultBridge';
  import { nicksToNocks } from './lib/utils/nicks';

  // Import all the components from the design concept
  import { validateWalletKey } from './vaultController';
  import type { WalletCandidate } from './lib/services/vanity';
  import { vanitySession } from './lib/stores/vanitySession';
  import CreateWallet from './lib/components/organisms/CreateWallet.svelte';
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
  import ImportWatchOnlyAddress from './lib/components/organisms/ImportWatchOnlyAddress.svelte';
  import HardwareWallet from './lib/components/organisms/HardwareWallet.svelte';
  import AddressBook from './lib/components/organisms/AddressBook.svelte';
  import JamTransaction from './lib/components/organisms/JamTransaction.svelte';
  import LockScreen from './lib/components/organisms/LockScreen.svelte';
  import ConfirmTransaction from './lib/components/organisms/ConfirmTransaction.svelte';
  import TransactionFailed from './lib/components/organisms/TransactionFailed.svelte';
  import TransactionDetails from './lib/components/organisms/TransactionDetails.svelte';
  import DevPanel from './components/dev/DevPanel.svelte';
  import ProgressBar from './lib/components/atoms/ProgressBar.svelte';
  import Button from './lib/components/atoms/Button.svelte';

  let currentRoute: Route = 'welcome';
  let routeData: any = null;
  let isInitialized = false;
  let initializationFailed = false;
  let retryInitialization: () => Promise<void> = async () => {};
  let activeWalletValue: Wallet | null = null;
  let transactions: any[] = [];
  let isLoadingTransactions = false;

  type HandoffRequest =
    | {
        action: 'send';
        recipients: Array<{ address: string; amount: string }>;
        origin?: string;
        createdAt: number;
      }
    | {
        action: 'jam';
        jam: string;
        toSign: boolean;
        origin?: string;
        createdAt: number;
      };

  const HANDOFF_STORAGE_KEY = 'handoffRequest';

  function parseHandoffFromSearch(): HandoffRequest | null {
    const params = new URLSearchParams(window.location.search);
    if (params.get('handoff') !== '1') {
      return null;
    }

    const action = params.get('action');
    if (action === 'send' || action === 'pay') {
      const tos = params.getAll('to');
      const amounts = params.getAll('amount');
      if (tos.length === 0 || amounts.length === 0 || tos.length !== amounts.length) {
        return null;
      }

      const recipients = tos.map((address, idx) => ({ address, amount: amounts[idx] }));
      return {
        action: 'send',
        recipients,
        origin: params.get('origin') || undefined,
        createdAt: Date.now()
      };
    }

    if (action === 'jam') {
      const jam = params.get('jam') || params.get('b64') || params.get('tx') || params.get('');
      if (!jam) {
        return null;
      }
      const toSignRaw = (params.get('toSign') || params.get('tosign') || '').toLowerCase();
      const toSign = toSignRaw === '1' || toSignRaw === 'true' || toSignRaw === 'yes';
      return {
        action: 'jam',
        jam,
        toSign,
        origin: params.get('origin') || undefined,
        createdAt: Date.now()
      };
    }

    return null;
  }

  async function persistHandoffRequest(request: HandoffRequest): Promise<void> {
    if (typeof chrome !== 'undefined' && chrome.storage) {
      return new Promise(resolve => {
        chrome.storage.local.set({ [HANDOFF_STORAGE_KEY]: request }, () => resolve());
      });
    }
    localStorage.setItem(HANDOFF_STORAGE_KEY, JSON.stringify(request));
  }

  async function fetchHandoffRequest(): Promise<HandoffRequest | null> {
    if (typeof chrome !== 'undefined' && chrome.storage) {
      return new Promise(resolve => {
        chrome.storage.local.get([HANDOFF_STORAGE_KEY], result => {
          resolve((result[HANDOFF_STORAGE_KEY] as HandoffRequest) || null);
        });
      });
    }

    const stored = localStorage.getItem(HANDOFF_STORAGE_KEY);
    if (!stored) return null;
    try {
      return JSON.parse(stored) as HandoffRequest;
    } catch {
      return null;
    }
  }

  async function clearHandoffRequest(): Promise<void> {
    if (typeof chrome !== 'undefined' && chrome.storage) {
      return new Promise(resolve => {
        chrome.storage.local.remove([HANDOFF_STORAGE_KEY], () => resolve());
      });
    }
    localStorage.removeItem(HANDOFF_STORAGE_KEY);
  }

  async function captureHandoffFromUrl(): Promise<void> {
    const request = parseHandoffFromSearch();
    if (!request) return;

    await persistHandoffRequest(request);

    const newUrl = window.location.pathname + window.location.hash;
    window.history.replaceState({}, document.title, newUrl);
  }

  async function maybeConsumeHandoffRequest(
    vaultState: { exists: boolean; unlocked: boolean },
    routerRoute: Route
  ): Promise<boolean> {
    const creationFlowRoutes: Route[] = [
      'create-wallet',
      'seed-phrase',
      'confirm-seed',
      'password-creation',
      'password-creation-import',
      'import-wallet'
    ];
    if (creationFlowRoutes.includes(routerRoute)) {
      return false;
    }

    const request = await fetchHandoffRequest();
    if (!request) {
      return false;
    }

    if (request.action === 'send') {
      if (!vaultState.exists || !vaultState.unlocked) {
        return false;
      }
      await clearHandoffRequest();
      router.navigate('send', { recipients: request.recipients, origin: request.origin });
    } else if (request.action === 'jam') {
      await clearHandoffRequest();
      router.navigate('jam', { jam: request.jam, toSign: request.toSign, origin: request.origin });
    }
    return true;
  }

  // Subscribe to stores with reactive assignment
  $: currentRoute = $router.currentRoute;
  $: routeData = $router.routeData;
  $: pendingForDisplay = currentRoute ? getPendingWallet() : null;

  // Debug logging for route changes
  $: {
    console.log('📍 Route changed to:', currentRoute);
    if (currentRoute === 'lock-screen') clearPendingWallet();
    if (currentRoute === 'lock-screen' || $walletStore.isLocked) vanitySession.clear();
  }

  // Fetch transactions when navigating to history
  $: if (currentRoute === 'history' && activeWalletValue) {
    isLoadingTransactions = true;
    walletStore
      .fetchTransactions(activeWalletValue.id)
      .then(txs => {
        transactions = txs;
        isLoadingTransactions = false;
      })
      .catch(error => {
        console.error('Failed to fetch transactions:', error);
        transactions = [];
        isLoadingTransactions = false;
      });
  }

  $: activeWalletValue = $activeWallet;

  // Progress bar logic
  const createFlowRoutes: Route[] = ['seed-phrase', 'confirm-seed', 'password-creation'];
  const importFlowRoutes: Route[] = ['import-wallet', 'password-creation-import'];

  $: shouldShowProgressBar = [...createFlowRoutes, ...importFlowRoutes].includes(currentRoute);
  $: isImportFlow = importFlowRoutes.includes(currentRoute);
  $: totalSteps = isImportFlow ? 2 : 3;
  $: currentStep = (() => {
    if (currentRoute === 'seed-phrase') return 1;
    if (currentRoute === 'confirm-seed') return 2;
    if (currentRoute === 'password-creation') return 3;
    if (currentRoute === 'import-wallet') return 1;
    if (currentRoute === 'password-creation-import') return 2;
    return 1;
  })();

  onMount(() => {
    let disposed = false;
    let initializing = false;
    const cleanups: Array<() => void> = [];
    const cleanup = () => cleanups.splice(0).forEach(stop => stop());
    retryInitialization = async () => {
      if (disposed || initializing) return;
      initializing = true;
      initializationFailed = false;
      try {
        await initialize(stop => {
          if (disposed) stop();
          else cleanups.push(stop);
        });
      } catch (error) {
        cleanup();
        console.error('Failed to initialize wallet:', error);
        if (!disposed) initializationFailed = true;
      } finally {
        initializing = false;
      }
    };
    void retryInitialization();
    return () => {
      disposed = true;
      vanitySession.clear();
      clearPendingWallet();
      cleanup();
    };
  });

  async function initialize(onCleanup: (stop: () => void) => void) {
    await captureHandoffFromUrl();

    // Load theme, settings, and router state
    await currentTheme.loadSavedTheme();
    await settingsStore.loadSettings();
    await router.loadFromStorage();
    await transactionStore.loadFromStorage();
    await walletStore.loadFromStorage();

    // Check vault status - this is the authoritative source
    const vaultState = await getVaultStatus();
    const routerState = get(router);
    const currentRouteFromStorage = routerState.currentRoute;
    const walletState = get(walletStore);
    const hasStoreOnlyWallets = (walletState.wallets || []).some(
      wallet => wallet.watchOnly || wallet.hardware
    );

    // Don't interrupt wallet creation flow - these routes should be preserved
    const creationFlowRoutes = [
      'create-wallet',
      'seed-phrase',
      'confirm-seed',
      'password-creation',
      'password-creation-import',
      'import-wallet'
    ];
    const isInCreationFlow = creationFlowRoutes.includes(currentRouteFromStorage);
    if (
      ['seed-phrase', 'confirm-seed', 'password-creation', 'password-creation-import'].includes(
        currentRouteFromStorage
      ) &&
      !getPendingWallet()
    ) {
      router.navigate(
        currentRouteFromStorage === 'password-creation-import' ? 'import-wallet' : 'create-wallet'
      );
    }

    if (!vaultState.exists) {
      walletStore.unlock();
      const allowWithoutVault =
        hasStoreOnlyWallets ||
        currentRouteFromStorage === 'hardware-wallet' ||
        currentRouteFromStorage === 'import-watch-only';

      // No vault exists - show welcome screen unless in creation flow or store-only flow
      if (!isInCreationFlow && currentRouteFromStorage !== 'welcome' && !allowWithoutVault) {
        router.navigate('welcome');
      }

      if (
        allowWithoutVault &&
        !isInCreationFlow &&
        (currentRouteFromStorage === 'welcome' || currentRouteFromStorage === 'lock-screen')
      ) {
        router.navigate('dashboard');
      }
    } else if (vaultState.exists && !vaultState.unlocked) {
      // Vault exists but is locked - show lock screen
      if (
        !isInCreationFlow &&
        currentRouteFromStorage !== 'lock-screen' &&
        !(allowHardwareWhileLocked && currentRouteFromStorage === 'hardware-wallet')
      ) {
        router.navigate('lock-screen');
      }
    } else if (vaultState.exists && vaultState.unlocked) {
      // Vault exists and is unlocked (either fresh unlock or restored from session)
      // Sync wallets to store
      const walletsResult = await getWalletsFromVault();
      if (!walletsResult.success) {
        throw new Error(walletsResult.error || 'Unable to load vault wallets');
      }
      if (walletsResult.wallets.length > 0) {
        // Sync vault wallets to the store
        await syncVaultWalletsToStore(walletsResult.wallets);

        // Restore to previous route (or dashboard) if coming from lock/welcome
        if (currentRouteFromStorage === 'welcome' || currentRouteFromStorage === 'lock-screen') {
          router.navigate('dashboard');
        }
        // Otherwise keep current route
      } else if (
        !isInCreationFlow &&
        !(allowHardwareWhileLocked && currentRouteFromStorage === 'hardware-wallet')
      ) {
        // Vault unlocked but no wallets - go to welcome
        router.navigate('welcome');
      }
    }

    await maybeConsumeHandoffRequest(vaultState, currentRouteFromStorage);

    onCleanup(walletStore.startStorageSync());
    isInitialized = true;

    // Track user activity for auto-lock (throttled to save every 10 seconds max)
    const activityEvents = ['mousedown', 'keydown', 'touchstart', 'scroll'];
    let lastSaveTime = 0;
    const SAVE_THROTTLE_MS = 10000; // Save at most every 10 seconds

    const handleActivity = () => {
      walletStore.updateLastActivity();
      // Throttle saves to storage
      const now = Date.now();
      if (now - lastSaveTime > SAVE_THROTTLE_MS) {
        lastSaveTime = now;
        walletStore.saveToStorage();
      }
    };

    activityEvents.forEach(event => {
      window.addEventListener(event, handleActivity, { passive: true });
    });

    // Periodic auto-lock check (every 30 seconds)
    const autoLockInterval = window.setInterval(async () => {
      try {
        if (!(await getVaultStatus()).exists) return;
        const currentSettings = get(settingsStore);
        const didLock = walletStore.checkAutoLock(currentSettings.autoLockTimeout);
        if (didLock) {
          await lockVaultSession();
          if (!(allowHardwareWhileLocked && get(router).currentRoute === 'hardware-wallet'))
            router.navigate('lock-screen');
        }
      } catch (error) {
        console.warn('Unable to check wallet auto-lock:', error);
      }
    }, 30000);

    onCleanup(() => {
      activityEvents.forEach(event => {
        window.removeEventListener(event, handleActivity);
      });
      clearInterval(autoLockInterval);
    });
  }

  /**
   * Sync wallets from vault to the Svelte store
   * The vault is the authoritative source - this ensures the store matches vault state
   */
  async function syncVaultWalletsToStore(vaultWallets: Wallet[]) {
    const state = get(walletStore);

    // Build a new wallets array from vault data, preserving store-only fields
    const syncedWallets: Wallet[] = vaultWallets.map(vaultWallet => {
      // Find existing wallet by vault ID or name
      const existing = state.wallets.find(
        w =>
          w.id === vaultWallet.id ||
          w.name === vaultWallet.name ||
          (w.addresses?.[0] &&
            vaultWallet.addresses?.[0] &&
            w.addresses[0] === vaultWallet.addresses[0])
      );

      if (existing) {
        // Merge: vault data takes precedence, but preserve store-only fields
        return {
          ...existing,
          id: vaultWallet.id, // Use vault ID for consistency
          name: vaultWallet.name,
          addresses: vaultWallet.addresses,
          masterPublicKey: vaultWallet.masterPublicKey,
          // Preserve store-only fields
          balance: existing.balance || 0,
          notes: existing.notes,
          transactions: existing.transactions,
          cachedData: existing.cachedData
        };
      } else {
        // New wallet from vault
        return vaultWallet;
      }
    });

    const syncedWalletIds = new Set(syncedWallets.map(wallet => wallet.id));
    const syncedAddresses = new Set(syncedWallets.flatMap(wallet => wallet.addresses ?? []));
    const storeOnlyWallets = state.wallets.filter(wallet => {
      if (!(wallet.watchOnly || wallet.hardware)) return false;
      if (syncedWalletIds.has(wallet.id)) return false;
      return !(wallet.addresses ?? []).some(address => syncedAddresses.has(address));
    });

    const mergedWallets = [...syncedWallets, ...storeOnlyWallets];

    // Update the store with synced wallets plus store-only wallets (hardware/watch-only)
    walletStore.setWallets(mergedWallets);

    // Set active wallet if none selected or current active wallet no longer exists
    const updatedState = get(walletStore);
    const activeStillExists =
      updatedState.activeWallet && mergedWallets.some(w => w.id === updatedState.activeWallet?.id);

    if (!activeStillExists && mergedWallets.length > 0) {
      walletStore.selectWallet(mergedWallets[0].id);
    }

    // Mark as unlocked
    walletStore.unlock(''); // Empty password since vault handles auth
    await walletStore.saveToStorage();

    // Fetch balances for all wallets in the background
    // This ensures wallet list shows correct balances
    for (const wallet of mergedWallets) {
      if (wallet.addresses && wallet.addresses.length > 0) {
        walletStore.fetchBalance(wallet.id).catch(err => {
          console.warn(`Failed to fetch balance for wallet ${wallet.name}:`, err);
        });
      }
    }
  }

  function handleCreateWallet() {
    clearPendingWallet();
    walletStore.setCreatingAdditionalWallet(false);
    router.navigate('create-wallet');
  }

  async function handleGeneratedWallet(candidate: WalletCandidate, name: string) {
    setPendingWallet(candidate, name);
    router.navigate('seed-phrase');
  }

  function cancelWalletCreation() {
    clearPendingWallet();
    const additional = get(walletStore).wallets.length > 0;
    walletStore.setCreatingAdditionalWallet(false);
    router.navigate(additional ? 'wallet-management' : 'welcome');
  }

  function backToCreation() {
    clearPendingWallet();
    router.navigate('create-wallet');
  }

  function handleImportWallet() {
    clearPendingWallet();
    walletStore.setCreatingAdditionalWallet(false);
    router.navigate('import-wallet');
  }

  function handleSeedPhraseContinue() {
    // User has seen the seed phrase, proceed to confirmation
    router.navigate('confirm-seed');
  }

  async function handleSeedPhraseConfirmed() {
    const vaultState = await getVaultStatus();

    // If vault already exists and is unlocked (adding additional wallet)
    if (vaultState.exists && vaultState.unlocked) {
      if (!(await finalizeWalletCreation())) return;
      walletStore.setCreatingAdditionalWallet(false);
      router.navigate('dashboard');
    } else {
      // Need to create vault first - go to password creation
      router.navigate('password-creation');
    }
  }

  async function handlePasswordCreated(password: string) {
    const vaultState = await getVaultStatus();

    // Create vault if it doesn't exist
    if (!vaultState.exists) {
      const created = await createVault(password);
      if (!created) {
        alert('Failed to create vault');
        return;
      }
      // Unlock the newly created vault
      const unlockResult = await unlockVaultWithPassword(password);
      if (!unlockResult.success) {
        alert('Failed to unlock vault: ' + unlockResult.error);
        return;
      }
    }

    // Now finalize the wallet creation (import to vault)
    if (!(await finalizeWalletCreation())) return;

    const state = get(walletStore);
    if (state.isCreatingAdditionalWallet) {
      walletStore.setCreatingAdditionalWallet(false);
      router.navigate('dashboard');
    } else {
      router.navigate('wallet-created');
    }
  }

  async function handlePasswordCreatedImport(password: string) {
    const vaultState = await getVaultStatus();

    // Create vault if it doesn't exist
    if (!vaultState.exists) {
      const created = await createVault(password);
      if (!created) {
        alert('Failed to create vault');
        return;
      }
      // Unlock the newly created vault
      const unlockResult = await unlockVaultWithPassword(password);
      if (!unlockResult.success) {
        alert('Failed to unlock vault: ' + unlockResult.error);
        return;
      }
    }

    // Import the pending recovery material into the vault
    if (!(await finalizeWalletCreation())) return;

    const state = get(walletStore);
    if (state.isCreatingAdditionalWallet) {
      walletStore.setCreatingAdditionalWallet(false);
      router.navigate('dashboard');
    } else {
      router.navigate('wallet-imported');
    }
  }

  /**
   * Save pending recovery material in the encrypted vault.
   */
  async function finalizeWalletCreation() {
    const pending = getPendingWallet();
    if (!pending) return false;
    const name = pending.walletName;
    const address = await validateWalletKey(pending.key);
    if (pending.address && address !== pending.address)
      throw new Error('Wallet address verification failed.');
    const result = await importWalletToVault(name, pending.key);

    if (!result.success || !result.wallet) {
      alert('Failed to save wallet to vault: ' + result.error);
      return false;
    }

    // Replace the pending wallet and persist the completed list before navigating.
    const state = get(walletStore);
    const wallet = result.wallet;
    walletStore.setWallets([
      ...state.wallets.filter(
        existing =>
          !(existing.name === name && existing.addresses.length === 0) &&
          existing.id !== wallet.id &&
          existing.addresses[0] !== wallet.addresses[0]
      ),
      wallet
    ]);
    walletStore.selectWallet(wallet.id);
    await walletStore.saveToStorage();

    clearPendingWallet();
    return true;
  }

  function handleWalletCreated() {
    router.navigate('dashboard');
  }

  async function handleWalletImported(key: string, walletName: string) {
    const address = await validateWalletKey(key);
    const state = get(walletStore);
    const vaultState = await getVaultStatus();

    // Generate a unique wallet name if not provided
    let finalWalletName = walletName.trim();
    if (!finalWalletName) {
      const vaultResult = await getWalletsFromVault();
      finalWalletName = nextWalletName([
        ...new Set([
          ...state.wallets.map(wallet => wallet.name),
          ...(vaultResult.wallets?.map(wallet => wallet.name) ?? [])
        ])
      ]);
    }

    const kind = key.trim().startsWith('zprv') ? 'extended' : 'mnemonic';
    setPendingWallet({ kind, key: key.trim(), address }, finalWalletName);
    if (vaultState.exists && vaultState.unlocked) {
      if (!(await finalizeWalletCreation())) throw new Error('Unable to import wallet.');
      walletStore.setCreatingAdditionalWallet(false);
      router.navigate('dashboard');
    } else {
      router.navigate('password-creation-import');
    }
  }

  async function handleWatchOnlyImport(address: string, walletName: string) {
    console.log('Importing watch-only address:', address);
    await walletStore.importWatchOnlyAddress(address, walletName);

    // Save to storage so watch-only wallet persists
    walletStore.saveToStorage();

    // Navigate to dashboard
    router.navigate('dashboard');
  }

  async function handleLockWallet() {
    await lockVaultSession();
    walletStore.logout();
    router.navigate('lock-screen');
  }

  async function handleUnlock(password: string): Promise<boolean> {
    // Unlock the vault first
    const result = await unlockVaultWithPassword(password);
    if (!result.success) {
      console.error('Failed to unlock vault:', result.error);
      return false;
    }

    // Sync wallets from vault to store
    if (result.wallets.length > 0) {
      await syncVaultWalletsToStore(result.wallets);
    }

    // Vault handles authentication - no need to store password

    const didNavigate = await maybeConsumeHandoffRequest(
      { exists: true, unlocked: true },
      get(router).currentRoute
    );
    if (!didNavigate) {
      router.navigate('dashboard');
    }
    return true;
  }

  function handleNavigate(event: CustomEvent<{ route: Route; data?: any }>) {
    router.navigate(event.detail.route, event.detail.data);
  }
</script>

<main class="nockster-wallet" data-theme={$currentTheme}>
  {#if isInitialized}
    <div class="wallet-content">
      {#if shouldShowProgressBar}
        <div class="progress-wrapper">
          <ProgressBar {currentStep} {totalSteps} />
        </div>
      {/if}

      {#if $vanitySession.options && currentRoute !== 'create-wallet' && currentRoute !== 'lock-screen'}
        <div class="vanity-banner" role="status">
          <span
            >{$vanitySession.progress.status === 'found'
              ? 'Vanity address ready'
              : $vanitySession.progress.status === 'mining'
                ? 'Vanity search running'
                : 'Vanity search stopped'}</span
          >
          <button
            on:click={() => {
              walletStore.setCreatingAdditionalWallet($walletStore.wallets.length > 0);
              router.navigate('create-wallet');
            }}>Return to Search</button
          >
          <button on:click={() => vanitySession.clear()}>Cancel Search</button>
        </div>
      {/if}
      <div class="route-content">
        {#if currentRoute === 'welcome'}
          <WelcomeScreen
            onCreateWallet={handleCreateWallet}
            onImportWallet={handleImportWallet}
            onConnectHardware={() => router.navigate('hardware-wallet')}
          />
        {:else if currentRoute === 'create-wallet'}
          <CreateWallet onReady={handleGeneratedWallet} onBack={cancelWalletCreation} />
        {:else if currentRoute === 'seed-phrase'}
          <SeedPhraseDisplay
            seedPhrase={pendingForDisplay?.key.split(' ') || []}
            onContinue={handleSeedPhraseContinue}
            onBack={backToCreation}
          />
        {:else if currentRoute === 'confirm-seed'}
          <ConfirmSeedPhrase
            seedPhrase={pendingForDisplay?.key.split(' ') || []}
            onConfirm={handleSeedPhraseConfirmed}
            onBack={() => router.navigate('seed-phrase')}
          />
        {:else if currentRoute === 'password-creation'}
          <PasswordCreation
            onCreateWallet={handlePasswordCreated}
            onBack={() => router.navigate('confirm-seed')}
          />
        {:else if currentRoute === 'password-creation-import'}
          <PasswordCreation
            onCreateWallet={handlePasswordCreatedImport}
            onBack={() => {
              clearPendingWallet();
              router.navigate('import-wallet');
            }}
          />
        {:else if currentRoute === 'wallet-created'}
          <WalletCreatedAnimation onContinue={handleWalletCreated} />
        {:else if currentRoute === 'wallet-imported'}
          <WalletImportedAnimation onComplete={handleWalletCreated} />
        {:else if currentRoute === 'dashboard'}
          <WalletDashboard
            onNavigate={(route, data) => {
              console.log('📱 App.svelte onNavigate called:', route, data);
              router.navigate(route, data);
            }}
            onManageWallets={() => {
              console.log('📱 App.svelte onManageWallets callback triggered');
              router.navigate('wallet-management');
            }}
            onSettings={() => {
              console.log('📱 App.svelte onSettings callback triggered');
              router.navigate('settings');
            }}
          />
        {:else if currentRoute === 'wallet-management'}
          <WalletManagement onBack={() => router.navigate('dashboard')} />
        {:else if currentRoute === 'send'}
          <slot name="send">
            <SendTransaction
              balance={nicksToNocks(BigInt(Math.trunc(activeWalletValue?.balance || 0)))}
              onBack={() => router.navigate('dashboard')}
              onSend={txData => router.navigate('confirm-transaction', txData)}
            />
          </slot>
        {:else if currentRoute === 'jam'}
          <slot
            name="transaction"
            jam={routeData?.jam || ''}
            toSign={Boolean(routeData?.toSign)}
            origin={routeData?.origin || undefined}
          >
            <JamTransaction
              jam={routeData?.jam || ''}
              toSign={Boolean(routeData?.toSign)}
              origin={routeData?.origin || undefined}
              onBack={() => router.navigate('dashboard')}
            />
          </slot>
        {:else if currentRoute === 'receive'}
          <ReceiveTransaction
            address={activeWalletValue?.addresses?.[0] || ''}
            onBack={() => router.navigate('dashboard')}
          />
        {:else if currentRoute === 'history'}
          <TransactionHistory
            {transactions}
            isLoading={isLoadingTransactions}
            onBack={() => router.navigate('dashboard')}
          />
        {:else if currentRoute === 'settings'}
          <Settings
            showPopOut={import.meta.env.MODE !== 'desktop'}
            onBack={() => router.navigate('dashboard')}
            onLogout={async () => {
              if (confirm('This will delete ALL wallet data. Are you sure?')) {
                try {
                  await walletStore.clearAllData();
                } catch {
                  alert('Unable to clear wallet data. Please try again.');
                }
              }
            }}
            onWalletHistory={() => router.navigate('history')}
            onManageWallets={() => router.navigate('wallet-management')}
            onAddressBook={() => router.navigate('address-book')}
            onExportSeed={() => {
              // TODO: Implement export seed phrase
              alert('Export seed phrase functionality coming soon');
            }}
            onImportWallet={() => {
              walletStore.setCreatingAdditionalWallet(true);
              router.navigate('import-wallet');
            }}
            onLock={async () => {
              await lockVaultSession();
              router.navigate('lock-screen');
            }}
            onPopOut={() => {
              const popupUrl = chrome.runtime.getURL('dist/index.html');
              chrome.windows.create({
                url: popupUrl,
                type: 'popup',
                width: 400,
                height: 600,
                focused: true
              });
            }}
          >
            <svelte:fragment slot="updates"><slot name="desktop-updates" /></svelte:fragment>
          </Settings>
        {:else if currentRoute === 'import-wallet'}
          <ImportWallet
            {readWalletFile}
            onImport={(phrase, name) => handleWalletImported(phrase, name)}
            onBack={() => {
              clearPendingWallet();
              const state = get(walletStore);
              if (state.isCreatingAdditionalWallet) {
                walletStore.setCreatingAdditionalWallet(false);
                router.navigate('wallet-management');
              } else {
                router.navigate('welcome');
              }
            }}
          />
        {:else if currentRoute === 'import-watch-only'}
          <ImportWatchOnlyAddress
            onImport={(address, name) => handleWatchOnlyImport(address, name)}
            onBack={() => router.navigate('wallet-management')}
          />
        {:else if currentRoute === 'hardware-wallet'}
          <slot name="hardware"
            ><HardwareWallet onBack={() => router.navigate('wallet-management')} /></slot
          >
        {:else if currentRoute === 'address-book'}
          <AddressBook
            desktopLayout={import.meta.env.MODE === 'desktop'}
            onBack={() => router.navigate('settings')}
          />
        {:else if currentRoute === 'lock-screen'}
          <LockScreen onUnlock={handleUnlock} />
        {:else if currentRoute === 'confirm-transaction'}
          <ConfirmTransaction
            recipientName={routeData?.recipientName ?? ''}
            recipientAddress={routeData?.recipientAddress ?? ''}
            amount={routeData?.amount ?? 0}
            fee={routeData?.fee ?? 0}
            onBack={() => router.navigate('send')}
            onConfirm={() => router.navigate('dashboard')}
            onCancel={() => router.navigate('send')}
          />
        {:else if currentRoute === 'transaction-failed'}
          <TransactionFailed onBack={() => router.navigate('send')} />
        {:else if currentRoute === 'transaction-details'}
          <TransactionDetails transaction={routeData} onBack={() => router.navigate('dashboard')} />
        {/if}
      </div>
    </div>
  {:else}
    <div class="loading" aria-busy={!initializationFailed}>
      {#if initializationFailed}
        <div class="initialization-message" role="alert">
          <h2>Unable to load your wallet</h2>
          <p>Nockster couldn’t read your saved wallet. Try loading it again.</p>
        </div>
        <Button on:click={retryInitialization}>Try again</Button>
      {:else}
        <div class="spinner" aria-hidden="true"></div>
        <p role="status">Loading your wallet...</p>
      {/if}
    </div>
  {/if}
  {#if $settingsStore.developerMode && currentRoute !== 'settings'}<DevPanel />{/if}
</main>
{#if $settingsStore.developerMode}
  <button
    class="debug-exit-button"
    type="button"
    on:click={() => settingsStore.updateSetting('developerMode', false)}
  >
    Hide Debug
  </button>
{/if}

<style>
  .vanity-banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--color-border);
    font-size: 12px;
  }
  .vanity-banner span {
    flex: 1;
    min-width: 120px;
  }
  .vanity-banner button {
    color: var(--color-text);
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 6px;
    padding: 6px 8px;
    cursor: pointer;
  }

  .nockster-wallet {
    width: 357px;
    height: 600px;
    min-width: 357px;
    min-height: 600px;
    display: flex;
    flex-direction: column;
    background: var(--color-background, #ffffff);
    color: var(--color-text, #1a1a1a);
    overflow: hidden;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  }

  .wallet-content {
    flex: 1;
    overflow-y: auto;
    position: relative;
    width: 100%;
    height: 100%;
  }

  .progress-wrapper {
    position: absolute;
    top: 28px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 10;
  }

  .route-content {
    width: 100%;
    height: 100%;
    position: relative;
  }

  .loading {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1rem;
    padding: 24px;
    text-align: center;
  }

  .initialization-message h2 {
    margin: 0 0 12px;
    font-size: 22px;
    font-weight: 600;
  }

  .initialization-message p {
    margin: 0;
    font-size: 16px;
    line-height: 1.5;
    color: var(--color-text-secondary);
  }

  .spinner {
    width: 40px;
    height: 40px;
    border: 3px solid var(--color-border);
    border-top-color: var(--color-text);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation: none;
    }
  }

  .debug-exit-button {
    position: fixed;
    bottom: 12px;
    left: 12px;
    padding: 8px 12px;
    border-radius: 6px;
    background: #fff;
    color: #111;
    font-size: 12px;
    border: 1px solid rgba(0, 0, 0, 0.2);
    cursor: pointer;
    z-index: 9999;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.1);
  }
</style>
