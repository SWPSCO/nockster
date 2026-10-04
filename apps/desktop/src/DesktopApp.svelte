<script lang="ts">
  import { onMount } from 'svelte';
  import App from '../../../packages/wallet/src/App.svelte';
  import { router, type Route } from '../../../packages/wallet/src/lib/stores/router';
  import { walletStore, activeWallet } from '../../../packages/wallet/src/lib/stores/wallet';
  import { lockVaultSession, clearPendingMnemonic } from '../../../packages/wallet/src/lib/utils/vaultBridge';

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
  $: canNavigate = !locked && !inFlow && Boolean($activeWallet);
  $: activeRoute = $router.currentRoute;
  let error = '';

  async function lock() {
    if (!(await lockVaultSession())) {
      error = 'Unable to lock your wallet. Try again.';
      return;
    }
    clearPendingMnemonic();
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
    </nav>
    {#if canNavigate}
      <div class="desktop-wallets">
        <div class="wallets-heading">
          <span>Wallets</span><button
            aria-label="Manage wallets"
            onclick={() => router.navigate('wallet-management')}>+</button
          >
        </div>
        {#each $walletStore.wallets as wallet}
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
                >{wallet.watchOnly
                  ? 'Watch only'
                  : wallet.hardware
                    ? 'Hardware'
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
      <button disabled={!canNavigate} onclick={lock}
        >Lock wallet <kbd>⇧ {navigator.platform.includes('Mac') ? '⌘' : 'Ctrl'} L</kbd></button
      >
      <div class="desktop-network">Nockchain <span>Mainnet</span></div>
    </div>
  </aside>
  <div
    class="desktop-workspace"
    class:wide={['dashboard', 'history', 'wallet-management', 'address-book'].includes(activeRoute)}
  >
    <App />
  </div>
  {#if error}
    <div class="desktop-error" role="alert">
      <span>{error}</span><button onclick={() => (error = '')} aria-label="Dismiss error"
        >Dismiss</button
      >
    </div>
  {/if}
</div>
