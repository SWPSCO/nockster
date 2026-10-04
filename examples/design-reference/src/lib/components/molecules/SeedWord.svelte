<script lang="ts">
  export let word: string;
  export let index: number;
  export let revealed: boolean = true;
  export let selectable: boolean = false;
  export let selected: boolean = false;
  
  function handleClick() {
    if (selectable) {
      selected = !selected;
    }
  }
</script>

<button
  class="seed-word"
  class:hidden={!revealed}
  class:selectable
  class:selected
  on:click={handleClick}
  disabled={!selectable}
  type="button"
>
  <span class="seed-index">{index}</span>
  <span class="seed-text">
    {#if revealed}
      {word}
    {:else}
      ••••••
    {/if}
  </span>
</button>

<style>
  .seed-word {
    display: flex;
    align-items: center;
    gap: var(--spacing-sm);
    padding: var(--spacing-sm) var(--spacing-md);
    background: var(--color-background);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    font-family: 'SF Mono', Monaco, monospace;
    transition: all var(--transition-fast);
    cursor: default;
    text-align: left;
  }
  
  .seed-word.selectable {
    cursor: pointer;
  }
  
  .seed-word.selectable:hover {
    background: var(--color-surface);
    border-color: var(--color-text-secondary);
  }
  
  .seed-word.selected {
    background: var(--color-primary);
    border-color: var(--color-primary);
    color: white;
  }
  
  .seed-word.selected .seed-index {
    background: rgba(255, 255, 255, 0.2);
    color: white;
  }
  
  .seed-word.hidden {
    background: var(--color-surface);
  }
  
  .seed-index {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 24px;
    height: 24px;
    padding: 0 var(--spacing-xs);
    background: var(--color-surface);
    border-radius: var(--radius-sm);
    font-size: var(--font-xs);
    font-weight: var(--font-semibold);
    color: var(--color-text-secondary);
  }
  
  .seed-text {
    font-size: var(--font-sm);
    font-weight: var(--font-medium);
    flex: 1;
  }
  
  .seed-word:disabled {
    cursor: default;
  }
  
  .seed-word:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
  }
</style>