<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { getRPCClient } from '../../utils/rpc';

  let status: 'connected' | 'disconnected' | 'connecting' = 'connecting';
  let rpcUrl = '';
  let interval: ReturnType<typeof setInterval>;
  let showDetails = false;

  async function checkConnection() {
    const rpc = getRPCClient();
    const connectionInfo = rpc.getConnectionStatus();

    status = connectionInfo.status;
    rpcUrl = connectionInfo.url;

    // Try to ping if disconnected
    if (status === 'disconnected') {
      try {
        const isAlive = await rpc.ping();
        if (isAlive) {
          status = 'connected';
        }
      } catch {
        // Still disconnected
      }
    }
  }

  onMount(() => {
    checkConnection();
    // Check connection every 10 seconds
    interval = setInterval(checkConnection, 10000);
  });

  onDestroy(() => {
    if (interval) {
      clearInterval(interval);
    }
  });

  function formatUrl(url: string): string {
    return url.replace('http://', '').replace('https://', '').replace('/rpc', '').substring(0, 25);
  }
</script>

<div
  class="connection-status"
  role="status"
  aria-label="Network connection status"
  class:connected={status === 'connected'}
  class:connecting={status === 'connecting'}
  class:disconnected={status === 'disconnected'}
  onmouseenter={() => (showDetails = true)}
  onmouseleave={() => (showDetails = false)}
>
  <span class="status-dot"></span>

  {#if showDetails}
    <span class="status-text">
      {#if status === 'connected'}
        Connected
      {:else if status === 'connecting'}
        Connecting...
      {:else}
        Disconnected
      {/if}
    </span>
  {/if}
</div>

<style>
  .connection-status {
    position: relative;
    display: inline-flex;
    align-items: center;
    padding: 6px;
    cursor: default;
  }

  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--color-error, #dc3545);
    transition: all 0.3s ease;
  }

  .connection-status.connected .status-dot {
    background: var(--color-success, #4caf50);
  }

  .connection-status.connecting .status-dot {
    background: var(--color-warning, #ffc107);
    animation: pulse 2s infinite;
  }

  .connection-status.disconnected .status-dot {
    background: var(--color-error, #dc3545);
  }

  .status-text {
    position: absolute;
    left: 20px;
    top: 50%;
    transform: translateY(-50%);
    font-size: 12px;
    font-weight: 400;
    color: var(--color-text-secondary);
    opacity: 0.7;
    animation: fadeIn 0.2s ease;
    white-space: nowrap;
    pointer-events: none;
  }

  @keyframes pulse {
    0% {
      box-shadow: 0 0 0 0 rgba(255, 193, 7, 0.7);
    }
    70% {
      box-shadow: 0 0 0 6px rgba(255, 193, 7, 0);
    }
    100% {
      box-shadow: 0 0 0 0 rgba(255, 193, 7, 0);
    }
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
    }
    to {
      opacity: 0.7;
    }
  }
</style>
