<script lang="ts">
  export let variant: 'primary' | 'secondary' | 'ghost' = 'primary';
  export let size: 'small' | 'medium' | 'large' = 'medium';
  export let disabled: boolean = false;
  export let fullWidth: boolean = false;
  export let type: 'button' | 'submit' | 'reset' = 'button';
  
  $: classes = `
    button
    button--${variant}
    button--${size}
    ${fullWidth ? 'button--full-width' : ''}
    ${disabled ? 'button--disabled' : ''}
  `.trim();
</script>

<button 
  {type}
  {disabled}
  class={classes}
  on:click
>
  <slot />
</button>

<style>
  .button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: var(--radius-md, 8px);
    font-weight: 600;
    transition: all 150ms ease;
    cursor: pointer;
    outline: none;
    position: relative;
    overflow: hidden;
  }
  
  .button--small {
    padding: var(--spacing-xs) var(--spacing-md);
    font-size: var(--font-sm);
    min-height: 32px;
  }
  
  .button--medium {
    padding: 12px 20px;
    font-size: 14px;
    min-height: 44px;
  }
  
  .button--large {
    padding: var(--spacing-md) var(--spacing-xl);
    font-size: var(--font-lg);
    min-height: 48px;
  }
  
  .button--primary {
    background: var(--color-text);
    color: var(--color-background);
    border: none;
  }
  
  .button--primary:hover:not(.button--disabled) {
    background: var(--color-text);
  }
  
  .button--primary:active:not(.button--disabled) {
    background: var(--color-text);
  }
  
  .button--secondary {
    background: var(--color-background);
    color: var(--color-text);
    border: 2px solid var(--color-border);
  }
  
  .button--secondary:hover:not(.button--disabled) {
    background: var(--color-surface);
    border-color: var(--color-text);
  }
  
  .button--secondary:active:not(.button--disabled) {
    background: var(--color-background);
  }
  
  .button--ghost {
    background: transparent;
    color: var(--color-text-secondary);
    border: none;
    text-decoration: underline;
    padding: 8px;
  }
  
  .button--ghost:hover:not(.button--disabled) {
    color: var(--color-text);
  }
  
  .button--full-width {
    width: 100%;
  }
  
  .button--disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  
  .button:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
  }
</style>