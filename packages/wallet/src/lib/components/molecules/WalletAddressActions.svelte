<script lang="ts">
  import Icon from '../atoms/Icon.svelte';
  import { onDestroy } from 'svelte';
  export let address: string;
  let copiedAddress = '';
  let copyError = false;
  let copyTimer: ReturnType<typeof setTimeout>;
  onDestroy(() => clearTimeout(copyTimer));

  async function copyAddress() {
    try {
      await navigator.clipboard.writeText(address);
      copiedAddress = address;
      copyError = false;
      clearTimeout(copyTimer);
      copyTimer = setTimeout(() => (copiedAddress = ''), 2000);
    } catch {
      copyError = true;
    }
  }
</script>

<div class="address-actions">
  <button
    type="button"
    aria-label={copiedAddress === address && address ? 'Copied' : 'Copy address'}
    title={copiedAddress === address && address ? 'Copied' : 'Copy address'}
    disabled={!address}
    on:click|stopPropagation={copyAddress}
    on:keydown|stopPropagation
  >
    <Icon name={copiedAddress === address && address ? 'check' : 'copy'} size="small" />
  </button>
  {#if address}
    <a
      href={`https://nockblocks.com/address/${encodeURIComponent(address)}`}
      target="_blank"
      rel="noopener noreferrer"
      on:click|stopPropagation
      on:keydown|stopPropagation
      aria-label="View address on Nockblocks"
      title="View address on Nockblocks"
    >
      <Icon name="externalLink" size="small" />
    </a>
  {/if}
  {#if copyError}<span role="status">Unable to copy address</span>{/if}
</div>

<style>
  .address-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    position: relative;
    flex-shrink: 0;
  }
  button,
  a {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    padding: 8px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--color-text-secondary);
    cursor: pointer;
    text-decoration: none;
  }
  button:hover,
  a:hover {
    color: var(--color-text);
    background: var(--color-surface);
  }
  button:focus-visible,
  a:focus-visible {
    outline: 2px solid var(--color-text);
    outline-offset: 2px;
  }
  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  span {
    position: absolute;
    top: 100%;
    right: 0;
    width: max-content;
    max-width: 180px;
    color: var(--color-error);
    background: var(--color-background);
    font-size: 12px;
    z-index: 1;
  }
</style>
