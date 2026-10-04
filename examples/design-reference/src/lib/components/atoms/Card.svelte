<script lang="ts">
  export let padding: 'none' | 'small' | 'medium' | 'large' = 'medium';
  export let shadow: 'none' | 'small' | 'medium' | 'large' = 'medium';
  export let clickable: boolean = false;
  export let selected: boolean = false;
  
  $: classes = `
    card
    card--padding-${padding}
    card--shadow-${shadow}
    ${clickable ? 'card--clickable' : ''}
    ${selected ? 'card--selected' : ''}
  `.trim();
</script>

<div 
  class={classes}
  on:click
  on:keydown
  role={clickable ? 'button' : undefined}
  tabindex={clickable ? 0 : undefined}
>
  <slot />
</div>

<style>
  .card {
    background: var(--color-background);
    border: 1px solid var(--color-border)56;
    border-radius: var(--radius-md, 8px);
    transition: all 150ms ease;
  }
  
  .card--padding-none {
    padding: 0;
  }
  
  .card--padding-small {
    padding: var(--spacing-sm);
  }
  
  .card--padding-medium {
    padding: 14px;
  }
  
  .card--padding-large {
    padding: var(--spacing-lg);
  }
  
  .card--shadow-none {
    box-shadow: none;
  }
  
  .card--shadow-small {
    box-shadow: var(--shadow-sm);
  }
  
  .card--shadow-medium {
    box-shadow: var(--shadow-md);
  }
  
  .card--shadow-large {
    box-shadow: var(--shadow-lg);
  }
  
  .card--clickable {
    cursor: pointer;
  }
  
  .card--clickable:hover {
    border-color: var(--color-text);
    background: var(--color-surface);
  }
  
  .card--clickable:active {
    background: var(--color-background);
  }
  
  .card--selected {
    border-color: var(--color-text);
    background: var(--color-surface);
  }
  
  .card--clickable:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
  }
</style>