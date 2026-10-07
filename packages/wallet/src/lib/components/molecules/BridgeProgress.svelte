<script lang="ts">
  import { onMount } from 'svelte';
  import { chainTip, watchChainTip } from '../../stores/chainTip';
  import { bridgeProgress } from '../../utils/bridgeProgress';

  export let blockHeight: number | null | undefined = undefined;
  export let status = 'confirmed';
  onMount(watchChainTip);
  $: progress = bridgeProgress(blockHeight, $chainTip, status);
</script>

<div class="bridge-progress">
  <span>{progress.label}</span>
  {#if progress.phase !== 'failed'}
    <progress
      max={progress.target}
      value={progress.blocks ?? 0}
      aria-label="Bridge block wait"
      aria-valuetext={progress.label}
    ></progress>
  {/if}
  {#if progress.phase === 'ready'}<small>Delivery on Base requires bridge processing.</small>{/if}
</div>

<style>
  .bridge-progress {
    display: grid;
    gap: 5px;
    margin-top: 8px;
    font-size: 12px;
    color: var(--color-text-secondary);
  }
  progress {
    display: block;
    width: 100%;
    min-width: 100px;
    height: 5px;
    border: 0;
    border-radius: 4px;
    overflow: hidden;
    appearance: none;
    background: var(--color-border);
  }
  progress::-webkit-progress-bar {
    background: var(--color-border);
  }
  progress::-webkit-progress-value {
    background: var(--color-text);
    border-radius: 4px;
  }
  progress::-moz-progress-bar {
    background: var(--color-text);
    border-radius: 4px;
  }
  small {
    font-size: 11px;
  }
</style>
