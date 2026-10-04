<script lang="ts">
  export let type: 'text' | 'password' | 'email' | 'number' = 'text';
  export let placeholder: string = '';
  export let value: string = '';
  export let label: string = '';
  export let error: string = '';
  export let disabled: boolean = false;
  export let readonly: boolean = false;
  export let required: boolean = false;
  export let id: string = `input-${Math.random().toString(36).substr(2, 9)}`;

  let focused = false;

  function handleFocus() {
    focused = true;
  }

  function handleBlur() {
    focused = false;
  }
</script>

<div class="input-wrapper">
  {#if label}
    <label for={id} class="label">
      {label}
      {#if required}
        <span class="required">*</span>
      {/if}
    </label>
  {/if}

  <div class="input-container" class:focused class:error={!!error} class:disabled>
    <input
      {id}
      {type}
      {placeholder}
      {disabled}
      {readonly}
      {required}
      bind:value
      on:focus={handleFocus}
      on:blur={handleBlur}
      on:input
      on:change
      on:keydown
      on:keyup
      on:keypress
      class="input"
    />
    <slot name="icon" />
  </div>

  {#if error}
    <span class="error-message">{error}</span>
  {/if}
</div>

<style>
  .input-wrapper {
    width: 100%;
  }

  .label {
    display: block;
    margin-bottom: 8px;
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  .required {
    color: var(--color-error);
    margin-left: 2px;
  }

  .input-container {
    position: relative;
    width: 100%;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md, 8px);
    background: var(--color-surface);
    transition: all 150ms ease;
  }

  .input-container.focused {
    border-color: var(--color-text);
    box-shadow: none;
  }

  .input-container.error {
    border-color: var(--color-error);
  }

  .input-container.error.focused {
    box-shadow: 0 0 0 3px rgba(239, 68, 68, 0.1);
  }

  .input-container.disabled {
    background: var(--color-surface);
    opacity: 0.6;
  }

  .input {
    width: 100%;
    padding: 12px;
    border: none;
    background: transparent;
    font-size: 14px;
    color: var(--color-text);
    outline: none;
  }

  .input::placeholder {
    color: var(--color-text-tertiary);
  }

  .input:disabled {
    cursor: not-allowed;
  }

  .error-message {
    display: block;
    margin-top: var(--spacing-xs);
    font-size: var(--font-sm);
    color: var(--color-error);
  }

  :global(.input-container .icon) {
    position: absolute;
    right: var(--spacing-md);
    top: 50%;
    transform: translateY(-50%);
    color: var(--color-text-secondary);
  }
</style>
