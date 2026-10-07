<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { firmwareUpdate } from '../hardware/firmware';
  import {
    lockVaultSession,
    clearPendingWallet
  } from '../../../../packages/wallet/src/lib/utils/vaultBridge';
  import { walletStore } from '../../../../packages/wallet/src/lib/stores/wallet';

  interface Status {
    phase: string;
    version?: string;
    downloaded: number;
    total?: number;
    error?: string;
  }
  let status: Status = { phase: '', downloaded: 0 };
  let installing = false;
  let installError = '';
  let checkPending = false;
  $: busy = installing || ['checking', 'downloading', 'installing'].includes(status.phase);
  $: progress = status.total
    ? Math.min(100, Math.round((status.downloaded / status.total) * 100))
    : null;

  async function downloadPage() {
    try {
      await openUrl('https://nockster.com/');
    } catch {
      installError = 'Open nockster.com in your browser to download the installer.';
    }
  }
  async function check() {
    if (checkPending || installing) return;
    checkPending = true;
    try {
      status = await invoke<Status>('desktop_update_check');
    } catch {
      status = {
        phase: 'error',
        downloaded: 0,
        error: 'Unable to check for updates. Try again later.'
      };
    } finally {
      checkPending = false;
    }
  }
  async function install() {
    if (installing || $firmwareUpdate.installing) return;
    installing = true;
    installError = '';
    try {
      if (!(await lockVaultSession())) throw new Error('Unable to lock your wallet. Try again.');
      clearPendingWallet();
      walletStore.lock();
      await invoke('desktop_update_install');
    } catch (error) {
      installError = error instanceof Error ? error.message : String(error);
      installing = false;
      try {
        status = await invoke<Status>('desktop_update_status');
      } catch {
        /* Keep the failure visible. */
      }
    }
  }
  onMount(() => {
    let alive = true;
    let polling = false;
    void check();
    const poll = setInterval(async () => {
      if (polling || !checkPending) return;
      polling = true;
      try {
        const latest = await invoke<Status>('desktop_update_status');
        if (alive) status = latest;
      } catch {
        /* A closed native window has no status to display. */
      } finally {
        polling = false;
      }
    }, 1000);
    const periodic = setInterval(() => void check(), 6 * 60 * 60 * 1000);
    const online = () => void check();
    window.addEventListener('online', online);
    return () => {
      alive = false;
      clearInterval(poll);
      clearInterval(periodic);
      window.removeEventListener('online', online);
    };
  });
</script>

<div class="app-updates">
  <span class="version">v{import.meta.env.VITE_APP_VERSION}</span>
  {#if status.phase === 'manual'}
    <p role="status">Version {status.version} needs a new installer.</p>
    <button onclick={downloadPage}>Download from nockster.com</button>
  {:else if status.phase === 'ready'}
    <p role="status">Version {status.version} is ready.</p>
    <button
      class="update-action"
      disabled={installing || $firmwareUpdate.installing}
      onclick={install}
    >
      Restart to update
    </button>
    {#if $firmwareUpdate.installing}<small>Available after the firmware update finishes.</small
      >{/if}
  {:else if status.phase === 'downloading'}
    <p role="status">Downloading update{progress === null ? '…' : ` · ${progress}%`}</p>
    <progress max="100" value={progress ?? undefined} aria-label="App update download"></progress>
  {:else if status.phase === 'checking'}
    <p role="status">Checking for updates…</p>
  {:else if status.phase !== 'disabled'}
    <button disabled={busy || checkPending} onclick={check}>Check for updates</button>
    {#if status.phase === 'current'}<small role="status">You’re up to date.</small>{/if}
    {#if status.phase === 'error'}<small role="status" title={status.error}
        >Update check failed. Try again later.</small
      ><button onclick={downloadPage}>Download from nockster.com</button>{/if}
  {/if}
  {#if installError}<small role="alert">{installError}</small>
    {#if status.phase !== 'error' && status.phase !== 'manual'}<button onclick={downloadPage}
        >Download from nockster.com</button
      >{/if}
  {/if}
</div>
{#if installing}
  <div class="update-overlay" role="status" aria-live="polite">
    <div>
      <strong>Installing update…</strong>
      <p>Your wallet is locked. Nockster will restart when the update is ready.</p>
    </div>
  </div>
{/if}

<style>
  .app-updates {
    border-top: 1px solid var(--border-color, #303030);
    padding: 14px 0 2px;
    font-size: 12px;
  }
  .version {
    color: #888;
    font-size: 11px;
  }
  p {
    margin: 8px 0;
    line-height: 1.5;
  }
  small {
    display: block;
    color: #999;
    line-height: 1.5;
    margin: 6px 0;
  }
  button {
    margin-top: 5px;
  }
  .update-action {
    color: #fff;
  }
  progress {
    width: 100%;
    height: 4px;
    accent-color: #eee;
  }
  .update-overlay {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: grid;
    place-items: center;
    background: #141414f5;
    padding: 32px;
  }
  .update-overlay > div {
    max-width: 360px;
  }
  .update-overlay strong {
    font-size: 20px;
  }
</style>
