<script lang="ts">
  import { onMount } from 'svelte';
  import App from '../../../packages/wallet/src/App.svelte';
  import { router, type Route } from '../../../packages/wallet/src/lib/stores/router';
  import { walletStore, activeWallet } from '../../../packages/wallet/src/lib/stores/wallet';
  import {
    lockVaultSession,
    clearPendingWallet
  } from '../../../packages/wallet/src/lib/utils/vaultBridge';
  import HardwareWorkspace from './hardware/HardwareWorkspace.svelte';
  import { hardwareSession } from './hardware/session';
  import { firmwareUpdate } from './hardware/firmware';
  import DesktopSend from './transactions/DesktopSend.svelte';
  import AppUpdates from './updates/AppUpdates.svelte';
  import TransactionReview from './transactions/TransactionReview.svelte';
  import JamTransaction from '../../../packages/wallet/src/lib/components/organisms/JamTransaction.svelte';

  const navigation: { label: string; route: Route; path: string }[] = [
    {
      label: 'Overview',
      route: 'dashboard',
      path: 'M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z'
    },
    { label: 'Send', route: 'send', path: 'M7 17 17 7M7 7h10v10' },
    { label: 'Receive', route: 'receive', path: 'm7 7 10 10M7 17h10V7' },
    { label: 'Activity', route: 'history', path: 'M4 5h16M4 12h16M4 19h16' },
    {
      label: 'Contacts',
      route: 'address-book',
      path: 'M16 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0ZM4 21v-2a6 6 0 0 1 6-6h4a6 6 0 0 1 6 6v2'
    },
    { label: 'Sign transaction', route: 'jam', path: 'M14 3H5v18h14V8ZM14 3v6h5M8 15l2 2 5-5' }
  ];
  const flowRoutes: Route[] = [
    'welcome',
    'create-wallet',
    'backup-secret-key',
    'lock-screen',
    'seed-phrase',
    'confirm-seed',
    'password-creation',
    'password-creation-import',
    'wallet-created',
    'wallet-imported',
    'import-wallet'
  ];
  $: locked = $walletStore.isLocked;
  $: inFlow = flowRoutes.includes($router.currentRoute);
  $: canNavigate =
    (!locked || Boolean($activeWallet?.hardware || $activeWallet?.watchOnly)) &&
    !inFlow &&
    Boolean($activeWallet);
  $: activeRoute = $router.currentRoute;
  let error = '';
  let workspace: HTMLDivElement;
  $: if (activeRoute) workspace?.scrollTo({ top: 0 });

  async function lock() {
    if (!(await lockVaultSession())) {
      error = 'Unable to lock your wallet. Try again.';
      return;
    }
    clearPendingWallet();
    walletStore.lock();
    router.navigate('lock-screen');
  }
  function keydown(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === 'l') {
      event.preventDefault();
      if (canNavigate) void lock();
    }
  }
  onMount(() => {
    const showError = (event: Event) => {
      error = (event as CustomEvent<string>).detail;
    };
    window.addEventListener('desktop-error', showError);
    return () => window.removeEventListener('desktop-error', showError);
  });
</script>

<svelte:window onkeydown={keydown} />
<div class="desktop-shell" class:focused-flow={inFlow}>
  <aside class="desktop-sidebar" aria-label="Wallet navigation">
    <div class="desktop-brand">
      <img src="/nockster-logo.svg" alt="" /><span class="nockster-brand">nockster</span>
    </div>
    <nav aria-label="Main">
      {#each navigation as item}
        <button
          disabled={!canNavigate}
          aria-current={activeRoute === item.route ? 'page' : undefined}
          onclick={() => router.navigate(item.route)}
        >
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            aria-hidden="true"><path d={item.path} /></svg
          >
          {item.label}
        </button>
      {/each}
      <button
        aria-label="Hardware"
        title={$hardwareSession.connection === 'connected' ? 'Device connected' : undefined}
        aria-current={activeRoute === 'hardware-wallet' ? 'page' : undefined}
        onclick={() => router.navigate('hardware-wallet')}
      >
        <svg
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          aria-hidden="true"
          ><rect x="6" y="2" width="12" height="20" rx="3" /><path d="M9 6h6v7H9zM10 18h4" /></svg
        >
        Hardware
        {#if $hardwareSession.connection === 'connected'}<span
            class="desktop-device-connected"
            aria-hidden="true"
          ></span>{/if}
      </button>
    </nav>
    {#if canNavigate}
      <div class="desktop-wallets">
        <div class="wallets-heading">
          <span>Wallets</span><button
            aria-label="Manage wallets"
            onclick={() => router.navigate('wallet-management')}>+</button
          >
        </div>
        {#each $walletStore.wallets.filter(wallet => !locked || wallet.hardware || wallet.watchOnly) as wallet}
          <button
            class:chosen={$activeWallet?.id === wallet.id}
            onclick={() => {
              walletStore.selectWallet(wallet.id);
              router.navigate('dashboard');
            }}
          >
            <span class="wallet-monogram" aria-hidden="true"
              >{wallet.name.slice(0, 1).toUpperCase()}</span
            >
            <span class="wallet-label"
              >{wallet.name}<small
                >{wallet.hardware
                  ? 'Hardware'
                  : wallet.watchOnly
                    ? 'Watch only'
                    : 'Nockchain'}</small
              ></span
            >
          </button>
        {/each}
      </div>
    {/if}
    <div class="desktop-sidebar-bottom">
      <button
        disabled={!canNavigate}
        aria-current={activeRoute === 'settings' ? 'page' : undefined}
        onclick={() => router.navigate('settings')}>Settings</button
      >
      {#if locked}<button onclick={() => router.navigate('lock-screen')}
          >Unlock software wallets</button
        >
      {:else}<button disabled={!canNavigate} onclick={lock}
          >Lock wallet <kbd>⇧ {navigator.platform.includes('Mac') ? '⌘' : 'Ctrl'} L</kbd></button
        >{/if}
      <div class="desktop-network">Nockchain <span>Mainnet</span></div>
      <AppUpdates />
    </div>
  </aside>
  <div
    bind:this={workspace}
    class="desktop-workspace"
    class:wide={[
      'dashboard',
      'history',
      'wallet-management',
      'address-book',
      'hardware-wallet'
    ].includes(activeRoute)}
  >
    <App allowHardwareWhileLocked>
      <HardwareWorkspace slot="hardware" />
      <DesktopSend slot="send" />
      <svelte:fragment slot="transaction" let:jam let:origin let:toSign>
        {#if jam}
          <JamTransaction {jam} {origin} {toSign} onBack={() => router.navigate('dashboard')} />
        {:else}
          <TransactionReview />
        {/if}
      </svelte:fragment>
    </App>
  </div>
  {#if error}
    <div class="desktop-error" role="alert">
      <span>{error}</span><button onclick={() => (error = '')} aria-label="Dismiss error"
        >Dismiss</button
      >
    </div>
  {/if}
  {#if $firmwareUpdate.installing && activeRoute !== 'hardware-wallet'}
    <div class="desktop-error" role="status">
      <span>Installing firmware. Keep your Nockster connected.</span><button
        onclick={() => router.navigate('hardware-wallet')}>Show device</button
      >
    </div>
  {/if}
</div>
