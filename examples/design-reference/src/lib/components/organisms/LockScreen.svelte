<script lang="ts">
  import Button from '../atoms/Button.svelte';
  import Logo from '../atoms/Logo.svelte';
  
  export let onUnlock: (password: string) => void = () => {};
  export let onForgotPassword: () => void = () => {};
  
  let password = '';
  let error = '';
  let isUnlocking = false;
  
  async function handleUnlock() {
    if (!password) {
      error = 'Password is required';
      return;
    }
    
    error = '';
    isUnlocking = true;
    
    // Simulate unlock attempt
    setTimeout(() => {
      // In a real app, you would verify the password here
      onUnlock(password);
      isUnlocking = false;
    }, 500);
  }
  
  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      handleUnlock();
    }
  }
</script>

<div class="lock-screen fixed-screen">
  <div class="lock-content">
    <Logo size="large" showText={false} />
    <h2 class="lock-title">Wallet Locked</h2>
    <p class="lock-subtitle">Enter your password to unlock</p>
    
    <div class="unlock-form">
      <input 
        type="password" 
        class="input-field"
        class:error
        placeholder="Enter password"
        bind:value={password}
        on:keydown={handleKeydown}
        disabled={isUnlocking}
      >
      
      {#if error}
        <span class="error-message">{error}</span>
      {/if}
      
      <Button 
        variant="primary" 
        fullWidth={true}
        disabled={isUnlocking}
        on:click={handleUnlock}
      >
        {isUnlocking ? 'Unlocking...' : 'Unlock'}
      </Button>
      
      <button 
        class="btn-ghost"
        on:click={onForgotPassword}
      >
        Forgot password?
      </button>
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
  
  .lock-content :global(.logo-container) {
    margin-bottom: 24px;
    color: var(--color-text);
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
  
  .unlock-form {
    width: 80%;
    display: flex;
    flex-direction: column;
    gap: 0;
  }
  
  .input-field {
    width: 100%;
    padding: 16px;
    border: 1px solid var(--color-border)782;
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
  
  .unlock-form :global(.button) {
    margin-bottom: 10px;
  }
  
  .btn-ghost {
    background: none;
    border: none;
    color: var(--color-text-secondary);
    font-size: 14px;
    cursor: pointer;
    text-decoration: underline;
    padding: 8px;
  }
  
  .btn-ghost:hover {
    color: var(--color-text);
  }
</style>