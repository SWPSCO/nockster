<script lang="ts">
  import { fly } from 'svelte/transition';
  import { LINKS } from '../../constants';
  import Header from '../molecules/Header.svelte';
  import { currentTheme } from '../../stores/theme';
  import { settingsStore } from '../../stores/settings';
  import { walletStore } from '../../stores/wallet';
  import { onMount } from 'svelte';
  import swpsLogoLight from '../../assets/swps-logo-black.png';
  import swpsLogoDark from '../../assets/swps-logo.png';

  export let onLogout: () => void = () => {};
  export let onExportSeed: () => void = () => {};
  export let onImportWallet: () => void = () => {};
  export let onAddressBook: () => void = () => {};
  export let onManageWallets: () => void = () => {};
  export let onWalletHistory: () => void = () => {};
  export let onBack: () => void = () => {};
  export let onLock: () => void = () => {};
  export let showPopOut = true;
  export let onPopOut: () => void = () => {};

  // Reactive logo selection based on theme
  $: swpsLogo = $currentTheme === 'light' ? swpsLogoLight : swpsLogoDark;

  // Get wallet count
  $: walletCount = $walletStore.wallets.length;

  const selectedNetwork = 'mainnet';
  let darkMode = false;

  // Subscribe to theme store
  currentTheme.subscribe(theme => {
    darkMode = theme === 'dark';
  });

  // Apply theme change
  function toggleTheme() {
    const newTheme = darkMode ? 'light' : 'dark';
    currentTheme.setTheme(newTheme);
  }

  // Handle auto-lock time change
  function handleAutoLockChange(event: Event) {
    const target = event.target as HTMLSelectElement;
    const value = target.value === 'never' ? 0 : parseInt(target.value);
    settingsStore.updateSetting('autoLockTimeout', value);
  }

  // Toggle debug panel visibility
  function toggleDebugPanel() {
    settingsStore.updateSetting('developerMode', !$settingsStore.developerMode);
  }
</script>

<div
  class="settings"
  in:fly={{ x: 357, duration: 300, opacity: 0 }}
  out:fly={{ x: 357, duration: 300, opacity: 0 }}
>
  <Header title="Settings" showBack={true} on:click={onBack} />

  <div class="content">
    <div class="settings-section">
      <h3 class="section-title">Security</h3>

      <div class="setting-item">
        <span class="setting-label">Lock Wallet</span>
        <button class="btn-text" on:click={onLock}>Lock →</button>
      </div>

      <div class="setting-item">
        <span class="setting-label">Auto-lock</span>
        <select
          class="select-control"
          value={$settingsStore.autoLockTimeout === 0
            ? 'never'
            : $settingsStore.autoLockTimeout.toString()}
          on:change={handleAutoLockChange}
        >
          <option value="1">1 minute</option>
          <option value="5">5 minutes</option>
          <option value="15">15 minutes</option>
          <option value="never">Never</option>
        </select>
      </div>

      <div class="setting-item">
        <span class="setting-label">Change Password</span>
        <button class="btn-text">Change →</button>
      </div>

      <div class="setting-item">
        <span class="setting-label">Show Seed Phrase</span>
        <button class="btn-text" on:click={onExportSeed}>View →</button>
      </div>
    </div>

    <div class="settings-section">
      <h3 class="section-title">Appearance</h3>

      <div class="setting-item">
        <span class="setting-label">Dark Mode</span>
        <button
          class="toggle"
          aria-label="Toggle dark mode"
          class:active={darkMode}
          on:click={toggleTheme}
        >
          <span class="toggle-slider"></span>
        </button>
      </div>
    </div>

    <div class="settings-section">
      <h3 class="section-title">Wallet Management</h3>

      <div class="setting-item">
        <span class="setting-label">Wallet History</span>
        <button class="btn-text" on:click={onWalletHistory}>View →</button>
      </div>

      <div class="setting-item">
        <span class="setting-label">Manage Wallets</span>
        <button class="btn-text" on:click={onManageWallets}>View →</button>
      </div>

      <div class="setting-item">
        <span class="setting-label">Import Wallet</span>
        <button class="btn-text" on:click={onImportWallet}>Import →</button>
      </div>

      <div class="setting-item">
        <span class="setting-label">Address Book</span>
        <button class="btn-text" on:click={onAddressBook}>View →</button>
      </div>

      <div class="setting-item">
        <span class="setting-label">Connected Wallets</span>
        <span class="setting-value">{walletCount} {walletCount === 1 ? 'wallet' : 'wallets'}</span>
      </div>
    </div>

    <div class="settings-section">
      <h3 class="section-title">Network</h3>

      <div class="setting-item">
        <span class="setting-label">Node Connection</span>
        <span class="status-indicator">
          <span class="status-dot"></span>
          Connected
        </span>
      </div>
    </div>

    <div class="settings-section">
      <h3 class="section-title">Advanced</h3>

      {#if showPopOut}
        <div class="setting-item">
          <span class="setting-label">Pop Out Window</span>
          <button class="btn-text" on:click={onPopOut}>Open →</button>
        </div>
      {/if}

      <div class="setting-item">
        <span class="setting-label">Debug Panel</span>
        <button
          class="toggle"
          aria-label="Toggle debug panel"
          class:active={$settingsStore.developerMode}
          on:click={toggleDebugPanel}
        >
          <span class="toggle-slider"></span>
        </button>
      </div>

      <div class="setting-item">
        <span class="setting-label">Export Transactions</span>
        <button class="btn-text">Export →</button>
      </div>
    </div>

    <div class="settings-section">
      <h3 class="section-title">About</h3>

      <div class="setting-item">
        <span class="setting-label">Version</span>
        <span class="setting-value">{import.meta.env.VITE_APP_VERSION}</span>
      </div>

      <slot name="updates" />

      <div class="setting-item">
        <span class="setting-label">Terms of Service</span>
        <button class="btn-text">View →</button>
      </div>

      <div class="setting-item">
        <span class="setting-label">Privacy Policy</span>
        <a class="btn-text" href={LINKS.PRIVACY} target="_blank" rel="noopener noreferrer">View →</a
        >
      </div>
    </div>

    <div class="developer-section">
      <div class="developer-label">Developed by</div>
      <a href="https://swps.io" target="_blank" rel="noopener noreferrer" class="developer-link">
        <img src={swpsLogo} alt="South Western Pool Supply" class="swps-logo" />
      </a>
    </div>

    <button class="btn-secondary danger" on:click={onLogout}> Clear All Data </button>
  </div>
</div>

<style>
  .settings {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-background);
  }

  .content {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
  }

  .settings-section {
    margin-bottom: 24px;
  }

  .settings-section:last-of-type {
    margin-bottom: 12px;
  }

  .section-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 12px;
  }

  .setting-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 0;
    border-bottom: 1px solid var(--color-surface);
  }

  .setting-item:last-child {
    border-bottom: none;
  }

  .setting-label {
    font-size: 15px;
    color: var(--color-text);
  }

  .setting-value {
    font-size: 15px;
    color: var(--color-text-secondary);
  }

  .select-control {
    padding: 8px 12px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    font-size: 14px;
    color: var(--color-text);
    cursor: pointer;
    outline: none;
  }

  .btn-text {
    background: none;
    border: none;
    color: var(--color-text-secondary);
    font-size: 14px;
    cursor: pointer;
    padding: 0;
  }

  .btn-text:hover {
    color: var(--color-text);
  }

  .swps-logo {
    width: 64%;
    height: auto;
    object-fit: contain;
    display: block;
    margin: 0 auto; /* Center the logo */
  }

  .status-indicator {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 14px;
    color: var(--color-text-secondary);
  }

  .status-dot {
    width: 8px;
    height: 8px;
    background: var(--color-success);
    border-radius: 50%;
  }

  .toggle {
    position: relative;
    width: 44px;
    height: 24px;
    background: var(--color-border);
    border: none;
    border-radius: 12px;
    cursor: pointer;
    transition: background 150ms ease;
  }

  .toggle.active {
    background: var(--color-success);
  }

  .toggle-slider {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 20px;
    height: 20px;
    background: var(--color-background);
    border-radius: 10px;
    transition: transform 150ms ease;
  }

  .toggle.active .toggle-slider {
    transform: translateX(20px);
  }

  .btn-secondary {
    background: var(--color-background);
    color: var(--color-text);
    border: 2px solid var(--color-border);
    border-radius: 8px;
    padding: 12px 20px;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
    width: 100%;
    margin-top: 12px;
  }

  .btn-secondary.danger {
    color: var(--color-error);
    border-color: var(--color-error);
  }

  .btn-secondary:hover {
    background: var(--color-surface);
  }

  .developer-section {
    text-align: center;
    padding: 12px 0;
    margin-top: auto;
    margin-bottom: 8px;
  }

  .developer-label {
    font-size: 14px;
    color: var(--color-text-secondary);
    margin-bottom: 8px;
  }

  .developer-link {
    display: block;
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text);
    text-decoration: none;
    transition: opacity 0.2s;
  }

  .developer-link:hover {
    opacity: 0.7;
    text-decoration: underline;
  }
</style>
