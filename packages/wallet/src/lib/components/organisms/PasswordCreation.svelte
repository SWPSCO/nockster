<script lang="ts">
  import { onMount } from 'svelte';
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';
  import { LINKS, ERROR_MESSAGES } from '../../constants';

  export let onCreateWallet: (password: string) => void = () => {};
  export let onBack: () => void = () => {};

  // Development mode detection
  const isDevelopment = import.meta.env.MODE === 'development';
  const DEV_PASSWORD = 'devpass123';

  let password = isDevelopment ? DEV_PASSWORD : '';
  let confirmPassword = isDevelopment ? DEV_PASSWORD : '';
  let agreedToTerms = isDevelopment ? true : false;
  let passwordError = '';
  let confirmError = '';
  let showPassword = false;
  let showConfirmPassword = false;

  // Auto-proceed in development mode after a short delay
  onMount(() => {
    if (isDevelopment) {
      console.log('🚀 Development mode: Auto-filling password and proceeding...');
      setTimeout(() => {
        handleCreateWallet();
      }, 1000); // Give user 1 second to see the screen before proceeding
    }
  });

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
    // Skip validation in development mode
    if (isDevelopment) {
      passwordError = '';
      return true;
    }

    if (password.length < 8) {
      passwordError = 'Password must be at least 8 characters';
      return false;
    }
    passwordError = '';
    return true;
  }

  function validateConfirmPassword() {
    // Skip validation in development mode
    if (isDevelopment) {
      confirmError = '';
      return true;
    }

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

    if (isDevelopment || (validPassword && validConfirm && agreedToTerms)) {
      onCreateWallet(password);
    }
  }
</script>

<div class="password-creation fixed-screen">
  <Header title="" showBack={true} showLogo={false} on:click={onBack} />

  <div class="content">
    <h2 class="title">Set Your Password</h2>
    <p class="subtitle">This password will unlock your wallet on this device.</p>

    {#if isDevelopment}
      <div class="dev-mode-notice">
        🚀 Development Mode: Password auto-filled and will proceed automatically
      </div>
    {/if}

    <div class="input-group">
      <label class="input-label" for="password-input">Password</label>
      <div class="input-wrapper">
        <input
          id="password-input"
          type={showPassword ? 'text' : 'password'}
          class="input-field"
          class:error={passwordError}
          placeholder="Enter password"
          bind:value={password}
          on:blur={validatePassword}
        />
        <button
          type="button"
          class="toggle-password"
          on:click={() => (showPassword = !showPassword)}
          aria-label={showPassword ? 'Hide password' : 'Show password'}
        >
          <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
            {#if showPassword}
              <path
                d="M10 4C4 4 1 10 1 10s3 6 9 6 9-6 9-6-3-6-9-6z"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
              <circle cx="10" cy="10" r="3" stroke="currentColor" stroke-width="1.5" />
            {:else}
              <path
                d="M10 4C4 4 1 10 1 10s3 6 9 6 9-6 9-6-3-6-9-6z"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
              <circle cx="10" cy="10" r="3" stroke="currentColor" stroke-width="1.5" />
              <path
                d="M3 3l14 14"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
              />
            {/if}
          </svg>
        </button>
      </div>
      {#if passwordError}
        <span class="error-message">{passwordError}</span>
      {/if}
    </div>

    <div class="input-group">
      <label class="input-label" for="confirm-password-input">Confirm Password</label>
      <div class="input-wrapper">
        <input
          id="confirm-password-input"
          type={showConfirmPassword ? 'text' : 'password'}
          class="input-field"
          class:error={confirmError}
          placeholder="Confirm password"
          bind:value={confirmPassword}
          on:blur={validateConfirmPassword}
        />
        <button
          type="button"
          class="toggle-password"
          on:click={() => (showConfirmPassword = !showConfirmPassword)}
          aria-label={showConfirmPassword ? 'Hide password' : 'Show password'}
        >
          <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
            {#if showConfirmPassword}
              <path
                d="M10 4C4 4 1 10 1 10s3 6 9 6 9-6 9-6-3-6-9-6z"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
              <circle cx="10" cy="10" r="3" stroke="currentColor" stroke-width="1.5" />
            {:else}
              <path
                d="M10 4C4 4 1 10 1 10s3 6 9 6 9-6 9-6-3-6-9-6z"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
              <circle cx="10" cy="10" r="3" stroke="currentColor" stroke-width="1.5" />
              <path
                d="M3 3l14 14"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
              />
            {/if}
          </svg>
        </button>
      </div>
      {#if confirmError}
        <span class="error-message">{confirmError}</span>
      {/if}
    </div>

    {#if password}
      <div class="password-strength">
        <div class="strength-bar">
          <div
            class="strength-fill"
            style="width: {(passwordStrength.level / 6) *
              100}%; background: {passwordStrength.color};"
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
        type="button"
        role="checkbox"
        aria-checked={agreedToTerms}
        aria-label="Agree to terms"
        class="checkbox"
        class:checked={agreedToTerms}
        on:click={() => (agreedToTerms = !agreedToTerms)}
      ></button>
      <div class="checkbox-label">
        I agree to the <a href={LINKS.TERMS} class="link" target="_blank" rel="noopener"
          >Terms of Service</a
        >
        and <a href={LINKS.PRIVACY} class="link" target="_blank" rel="noopener">Privacy Policy</a>
      </div>
    </div>

    <Button
      variant="primary"
      fullWidth={true}
      disabled={!password || !confirmPassword || !agreedToTerms}
      on:click={handleCreateWallet}
    >
      Finish Setup
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
  }

  :global([data-theme='dark']) .dev-mode-notice,
  :global([data-theme='dark-blue']) .dev-mode-notice,
  :global([data-theme='purple']) .dev-mode-notice {
    background: rgba(251, 191, 36, 0.1);
    color: #fbbf24;
    border-color: rgba(251, 191, 36, 0.2);
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

  .input-wrapper {
    position: relative;
    display: flex;
    align-items: center;
  }

  .input-field {
    width: 100%;
    padding: 14px 48px 14px 16px;
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

  .toggle-password {
    position: absolute;
    right: 12px;
    background: none;
    border: none;
    padding: 4px;
    cursor: pointer;
    color: var(--color-text-secondary);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: color 0.15s ease;
  }

  .toggle-password:hover {
    color: var(--color-text);
  }

  .toggle-password svg {
    width: 20px;
    height: 20px;
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
    width: 20px;
    height: 20px;
    min-width: 20px;
    border: 2px solid var(--color-text-secondary);
    border-radius: 4px;
    background: var(--color-surface);
    cursor: pointer;
    margin-top: 1px;
    position: relative;
    transition: all 0.15s ease;
  }

  .checkbox.checked {
    background: var(--color-primary);
    border-color: var(--color-primary);
  }

  .checkbox.checked::after {
    content: '';
    position: absolute;
    top: 50%;
    left: 50%;
    width: 5px;
    height: 9px;
    border: solid white;
    border-width: 0 2px 2px 0;
    transform: translate(-50%, -60%) rotate(45deg);
  }

  .checkbox-label {
    font-size: 14px;
    color: var(--color-text);
    line-height: 1.5;
    opacity: 0.9;
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
