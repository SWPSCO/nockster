<script lang="ts">
  import { onMount } from 'svelte';
  import Button from '../atoms/Button.svelte';
  import Logo from '../atoms/Logo.svelte';

  export let onUnlock: (password: string) => Promise<boolean> | boolean = () => false;

  // Development mode detection
  const isDevelopment = import.meta.env.MODE === 'development';
  const DEV_PASSWORD = 'devpass123';

  let password = isDevelopment ? DEV_PASSWORD : '';
  let error = '';
  let isUnlocking = false;

  // Auto-unlock in development mode after a short delay
  onMount(() => {
    if (isDevelopment) {
      console.log('🚀 Development mode: Auto-unlocking with dev password...');
      setTimeout(() => {
        handleUnlock();
      }, 500);
    }
  });

  async function handleUnlock() {
    if (!password) {
      error = 'Password is required';
      return;
    }

    error = '';
    isUnlocking = true;

    try {
      // Call onUnlock and await if it returns a promise
      const result = onUnlock(password);
      const success = result instanceof Promise ? await result : result;

      if (!success) {
        error = 'Incorrect password';
      }
    } catch (err) {
      console.error('Unlock error:', err);
      error = 'Failed to unlock wallet';
    } finally {
      isUnlocking = false;
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      handleUnlock();
    }
  }
</script>

<div class="lock-screen fixed-screen">
  <div class="lock-content">
    <div class="logo-row">
      <Logo size="large" showText={false} />
      <img src="./nockster-logotype.svg" alt="Nockster" class="logotype" />
    </div>
    <h2 class="lock-title">Wallet Locked</h2>
    <!-- <p class="lock-subtitle">Enter your password to unlock</p> -->

    {#if isDevelopment}
      <div class="dev-mode-notice">🚀 Development Mode: Auto-unlocking...</div>
    {/if}

    <div class="unlock-form">
      <input
        type="password"
        class="input-field"
        class:error
        placeholder="Enter password"
        bind:value={password}
        on:keydown={handleKeydown}
        disabled={isUnlocking}
      />

      {#if error}
        <span class="error-message">{error}</span>
      {/if}

      <Button variant="primary" fullWidth={true} disabled={isUnlocking} on:click={handleUnlock}>
        {isUnlocking ? 'Unlocking...' : 'Unlock'}
      </Button>
    </div>
  </div>
</div>

<style>
  .lock-screen {
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--color-background);
  }

  .lock-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    width: 100%;
    max-width: 300px;
    padding: 0 30px;
  }

  .logo-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 24px;
  }

  .logo-row :global(.logo-container) {
    color: var(--color-text);
  }

  .logotype {
    width: 160px;
    height: auto;
  }

  /* Invert logotype in dark mode */
  :global([data-theme='dark']) .logotype,
  :global([data-theme='dark-blue']) .logotype,
  :global([data-theme='purple']) .logotype {
    filter: invert(1);
  }

  .lock-title {
    font-size: 24px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 8px;
  }

  .lock-subtitle {
    font-size: 14px;
    color: var(--color-text-secondary);
    margin-bottom: 32px;
  }

  .dev-mode-notice {
    background: #fef3c7;
    color: #92400e;
    padding: 12px 16px;
    border-radius: 8px;
    margin-bottom: 20px;
    text-align: center;
    font-size: 13px;
    font-weight: 500;
    border: 1px solid #fde68a;
    width: 100%;
  }

  :global([data-theme='dark']) .dev-mode-notice,
  :global([data-theme='dark-blue']) .dev-mode-notice,
  :global([data-theme='purple']) .dev-mode-notice {
    background: rgba(251, 191, 36, 0.1);
    color: #fbbf24;
    border-color: rgba(251, 191, 36, 0.2);
  }

  .unlock-form {
    width: 80%;
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .input-field {
    width: 100%;
    padding: 16px;
    border: 1px solid var(--color-border) 782;
    border-radius: 8px;
    background: var(--color-surface);
    font-size: 14px;
    margin-bottom: 16px;
    outline: none;
  }

  .input-field:focus {
    border-color: var(--color-text);
  }

  .input-field.error {
    border-color: var(--color-error);
  }

  .input-field:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .error-message {
    display: block;
    font-size: 14px;
    color: var(--color-error);
    margin: -12px 0 12px;
    text-align: left;
  }
</style>
