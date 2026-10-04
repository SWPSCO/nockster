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

<button {type} {disabled} class={classes} on:click>
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
    transition: all 0.2s ease;
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

  /* Dark mode primary button */
  :global([data-theme='dark']) .button--primary {
    background: rgba(255, 255, 255, 0.9);
    color: #000;
  }

  :global([data-theme='dark']) .button--primary:hover:not(.button--disabled) {
    background: rgba(255, 255, 255, 0.95);
    transform: translateY(-1px);
    box-shadow: 0 10px 30px rgba(255, 255, 255, 0.1);
  }

  :global([data-theme='dark']) .button--primary:active:not(.button--disabled) {
    background: rgba(255, 255, 255, 0.9);
    transform: translateY(0);
    box-shadow: none;
  }

  /* Light mode primary button */
  :global([data-theme='light']) .button--primary:hover:not(.button--disabled) {
    background: #222;
    transform: translateY(-1px);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.1);
  }

  :global([data-theme='light']) .button--primary:active:not(.button--disabled) {
    background: var(--color-text);
    transform: translateY(0);
    box-shadow: none;
  }

  .button--secondary {
    background: var(--color-background);
    color: var(--color-text);
    border: 2px solid var(--color-border);
  }

  /* Dark mode secondary button */
  :global([data-theme='dark']) .button--secondary {
    background: rgba(255, 255, 255, 0.04);
    color: rgba(255, 255, 255, 0.9);
    border: 1px solid rgba(255, 255, 255, 0.12);
  }

  :global([data-theme='dark']) .button--secondary:hover:not(.button--disabled) {
    background: rgba(255, 255, 255, 0.07);
    border-color: rgba(255, 255, 255, 0.12);
  }

  :global([data-theme='dark']) .button--secondary:active:not(.button--disabled) {
    background: rgba(255, 255, 255, 0.04);
  }

  /* Light mode secondary button */
  :global([data-theme='light']) .button--secondary:hover:not(.button--disabled) {
    background: rgba(0, 0, 0, 0.05);
    border-color: rgba(0, 0, 0, 0.15);
  }

  :global([data-theme='light']) .button--secondary:active:not(.button--disabled) {
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
    cursor: not-allowed;
  }

  /* Dark mode disabled state */
  :global([data-theme='dark']) .button--disabled {
    background: rgba(255, 255, 255, 0.05) !important;
    color: rgba(255, 255, 255, 0.3) !important;
    border-color: rgba(255, 255, 255, 0.08) !important;
  }

  /* Light mode disabled state */
  :global([data-theme='light']) .button--disabled {
    opacity: 0.5;
  }

  .button:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
  }
</style>
