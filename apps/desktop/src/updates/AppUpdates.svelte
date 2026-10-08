<script lang="ts">
  import { firmwareUpdate } from '../hardware/firmware';
  import { desktopUpdates } from './session';
  $: ({ status, installing, installError, checkPending } = $desktopUpdates);
  $: busy = installing || ['checking', 'downloading', 'installing'].includes(status.phase);
  $: progress = status.total
    ? Math.min(100, Math.round((status.downloaded / status.total) * 100))
    : null;
  const { check, install, downloadPage } = desktopUpdates;
</script>

<div class="app-updates">
  <h3>App updates</h3>
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
  {:else if status.phase === 'disabled'}
    <small>Updates are available in installed releases.</small>
  {:else}
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

<style>
  .app-updates {
    border-top: 1px solid var(--color-border);
    padding: 20px 0;
    font-size: 12px;
  }
  h3 {
    font-size: 16px;
    font-weight: 500;
    margin-bottom: 12px;
  }
  p {
    margin: 8px 0;
    line-height: 1.5;
  }
  small {
    display: block;
    color: var(--color-text-secondary);
    line-height: 1.5;
    margin: 6px 0;
  }
  button {
    margin-top: 5px;
    padding: 10px 14px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-surface);
    color: var(--color-text);
  }
  button:disabled {
    opacity: 0.5;
  }
  .update-action {
    color: var(--color-text);
  }
  progress {
    width: 100%;
    height: 4px;
    accent-color: var(--color-text);
  }
</style>
