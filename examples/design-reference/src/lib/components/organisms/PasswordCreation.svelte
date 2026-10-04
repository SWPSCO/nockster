<script lang="ts">
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';
  import ProgressBar from '../atoms/ProgressBar.svelte';
  
  export let onCreateWallet: () => void = () => {};
  export let onBack: () => void = () => {};
  export let isImportFlow: boolean = false;
  
  let password = '';
  let confirmPassword = '';
  let agreedToTerms = false;
  let passwordError = '';
  let confirmError = '';
  
  $: passwordStrength = getPasswordStrength(password);
  
  function getPasswordStrength(pwd: string) {
    if (!pwd) return { level: 0, text: '', color: '#e5e7eb' };
    
    let strength = 0;
    if (pwd.length >= 8) strength++;
    if (pwd.length >= 12) strength++;
    if (/[a-z]/.test(pwd)) strength++;
    if (/[A-Z]/.test(pwd)) strength++;
    if (/[0-9]/.test(pwd)) strength++;
    if (/[^a-zA-Z0-9]/.test(pwd)) strength++;
    
    const levels = [
      { level: 0, text: '', color: '#e5e7eb' },
      { level: 1, text: 'Weak', color: '#dc2626' },
      { level: 2, text: 'Weak', color: '#dc2626' },
      { level: 3, text: 'Medium', color: '#fde047' },
      { level: 4, text: 'Medium', color: '#fde047' },
      { level: 5, text: 'Strong', color: '#16a34a' },
      { level: 6, text: 'Strong', color: '#16a34a' }
    ];
    
    return levels[strength];
  }
  
  function validatePassword() {
    if (password.length < 8) {
      passwordError = 'Password must be at least 8 characters';
      return false;
    }
    passwordError = '';
    return true;
  }
  
  function validateConfirmPassword() {
    if (confirmPassword !== password) {
      confirmError = 'Passwords do not match';
      return false;
    }
    confirmError = '';
    return true;
  }
  
  function handleCreateWallet() {
    const validPassword = validatePassword();
    const validConfirm = validateConfirmPassword();
    
    if (validPassword && validConfirm && agreedToTerms) {
      onCreateWallet();
    }
  }
</script>

<div class="password-creation fixed-screen">
  <Header title="" showBack={true} showLogo={false} on:click={onBack} />
  <div class="progress-wrapper">
    <ProgressBar currentStep={isImportFlow ? 2 : 3} totalSteps={isImportFlow ? 2 : 3} />
  </div>
  
  <div class="content">
    <h2 class="title">Set Your Password</h2>
    <p class="subtitle">
      This password will unlock your wallet on this device.
    </p>
    
    <div class="input-group">
      <label class="input-label">Password</label>
      <input 
        type="password" 
        class="input-field"
        class:error={passwordError}
        placeholder="Enter password"
        bind:value={password}
        on:blur={validatePassword}
      >
      {#if passwordError}
        <span class="error-message">{passwordError}</span>
      {/if}
    </div>
    
    <div class="input-group">
      <label class="input-label">Confirm Password</label>
      <input 
        type="password" 
        class="input-field"
        class:error={confirmError}
        placeholder="Confirm password"
        bind:value={confirmPassword}
        on:blur={validateConfirmPassword}
      >
      {#if confirmError}
        <span class="error-message">{confirmError}</span>
      {/if}
    </div>
    
    {#if password}
      <div class="password-strength">
        <div class="strength-bar">
          <div 
            class="strength-fill" 
            style="width: {(passwordStrength.level / 6) * 100}%; background: {passwordStrength.color};"
          ></div>
        </div>
        <p class="strength-text" style="color: {passwordStrength.color};">
          {passwordStrength.text ? `${passwordStrength.text} password` : ''}
        </p>
      </div>
    {/if}
  </div>

  <div class="footer-section">
    <div class="checkbox-group">
      <button
        class="checkbox"
        class:checked={agreedToTerms}
        on:click={() => agreedToTerms = !agreedToTerms}
      ></button>
      <label class="checkbox-label">
        I agree to the <a href="#" class="link">Terms of Service</a> and <a href="#" class="link">Privacy Policy</a>
      </label>
    </div>

    <Button
      variant="primary"
      fullWidth={true}
      disabled={!password || !confirmPassword || !agreedToTerms}
      on:click={handleCreateWallet}
    >
      Create Wallet
    </Button>
  </div>
</div>

<style>
  .password-creation {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-background);
    position: relative;
  }

  .progress-wrapper {
    position: absolute;
    top: 28px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 10;
  }
  
  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 20px 16px 16px;
    overflow-y: auto;
    overflow-x: hidden;
  }
  
  .title {
    font-size: 20px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 8px;
    margin-top: 0;
    text-align: center;
  }
  
  .subtitle {
    font-size: 14px;
    color: var(--color-text-secondary);
    margin-bottom: 24px;
    text-align: center;
    line-height: 1.4;
  }
  
  .input-group {
    margin-bottom: 16px;
    flex-shrink: 0;
  }
  
  .input-label {
    display: block;
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 8px;
  }
  
  .input-field {
    width: 100%;
    padding: 14px 16px;
    border: 1.5px solid transparent;
    border-radius: 10px;
    background: var(--color-surface);
    font-size: 15px;
    outline: none;
    transition: all 0.15s ease;
  }
  
  .input-field:focus {
    border-color: var(--color-border);
    background: var(--color-background);
  }
  
  .input-field.error {
    border-color: var(--color-border);
    background: var(--color-surface);
  }
  
  .error-message {
    display: block;
    font-size: 13px;
    color: var(--color-error);
    margin-top: 6px;
    font-weight: 400;
  }
  
  .password-strength {
    margin-bottom: 20px;
  }
  
  .strength-bar {
    height: 4px;
    background: var(--color-border);
    border-radius: 2px;
    margin-bottom: 8px;
    overflow: hidden;
  }
  
  .strength-fill {
    height: 100%;
    transition: all 300ms ease;
    border-radius: 2px;
  }
  
  .strength-text {
    font-size: 14px;
    margin: 0;
  }
  
  .footer-section {
    position: absolute;
    bottom: 0;
    left: 0;
    right: 0;
    padding: 16px;
    background: var(--color-background);
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .checkbox-group {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }
  
  .checkbox {
    width: 18px;
    height: 18px;
    min-width: 18px;
    border: 1.5px solid var(--color-border)2246;
    border-radius: 4px;
    background: var(--color-background);
    cursor: pointer;
    margin-top: 1px;
    position: relative;
    transition: all 0.15s ease;
  }
  
  .checkbox.checked {
    background: var(--color-text);
    border-color: var(--color-text);
  }
  
  .checkbox.checked::after {
    content: '';
    position: absolute;
    top: 3px;
    left: 5px;
    width: 6px;
    height: 9px;
    border: solid white;
    border-width: 0 2px 2px 0;
    transform: rotate(45deg);
  }
  
  .checkbox-label {
    font-size: 14px;
    color: var(--color-text-secondary);
    line-height: 1.5;
  }
  
  .link {
    color: var(--color-text);
    text-decoration: none;
    font-weight: 500;
    border-bottom: 1px solid transparent;
    transition: border-color 0.15s ease;
  }

  .link:hover {
    border-bottom-color: var(--color-text);
  }
  
  .footer-section :global(.button--primary) {
    margin-top: 0;
  }
  
  .content :global(.button--disabled) {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>