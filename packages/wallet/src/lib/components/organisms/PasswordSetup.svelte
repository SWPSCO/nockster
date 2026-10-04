<script>
  import { createVault } from '../lib/stores/wallet';

  let password = '';
  let confirmPassword = '';
  let error = '';
  let isCreating = false;

  async function handleSubmit() {
    error = '';

    if (password.length < 8) {
      error = 'Password must be at least 8 characters';
      return;
    }

    if (password !== confirmPassword) {
      error = 'Passwords do not match';
      return;
    }

    isCreating = true;

    try {
      await createVault(password);
    } catch (err) {
      error = err.message;
    } finally {
      isCreating = false;
    }
  }
</script>

<div class="password-setup">
  <div class="setup-card">
    <h2>Create Your Vault</h2>
    <p class="description">
      Set a strong password to protect your wallet vault. This password will encrypt all your
      wallets locally.
    </p>

    <form on:submit|preventDefault={handleSubmit}>
      <div class="form-group">
        <label for="password">Password</label>
        <input
          id="password"
          type="password"
          bind:value={password}
          placeholder="Enter password (min 8 characters)"
          required
        />
      </div>

      <div class="form-group">
        <label for="confirm">Confirm Password</label>
        <input
          id="confirm"
          type="password"
          bind:value={confirmPassword}
          placeholder="Confirm your password"
          required
        />
      </div>

      {#if error}
        <div class="error-message">
          {error}
        </div>
      {/if}

      <div class="security-note">
        <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
        </svg>
        <span>Your password is stored locally and never sent to any server</span>
      </div>

      <button type="submit" class="btn-primary" disabled={isCreating}>
        {isCreating ? 'Creating Vault...' : 'Create Vault'}
      </button>
    </form>
  </div>
</div>

<style>
  .password-setup {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 450px;
  }

  .setup-card {
    background: rgba(255, 255, 255, 0.05);
    backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 12px;
    padding: 2rem;
    width: 100%;
  }

  h2 {
    margin: 0 0 0.5rem 0;
    font-size: 1.5rem;
    background: linear-gradient(135deg, #4fc3f7, #29b6f6);
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
  }

  .description {
    color: rgba(255, 255, 255, 0.7);
    font-size: 0.875rem;
    margin-bottom: 1.5rem;
  }

  .form-group {
    margin-bottom: 1rem;
  }

  label {
    display: block;
    margin-bottom: 0.5rem;
    font-size: 0.875rem;
    color: rgba(255, 255, 255, 0.9);
  }

  input {
    width: 100%;
    padding: 0.75rem;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 8px;
    color: #fff;
    font-size: 0.875rem;
    transition: all 0.2s;
  }

  input::placeholder {
    color: rgba(255, 255, 255, 0.4);
  }

  input:focus {
    outline: none;
    border-color: #4fc3f7;
    background: rgba(255, 255, 255, 0.15);
  }

  .error-message {
    background: rgba(244, 67, 54, 0.1);
    border: 1px solid rgba(244, 67, 54, 0.3);
    color: #ff6b6b;
    padding: 0.75rem;
    border-radius: 8px;
    margin-bottom: 1rem;
    font-size: 0.875rem;
  }

  .security-note {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem;
    background: rgba(76, 175, 80, 0.1);
    border: 1px solid rgba(76, 175, 80, 0.2);
    border-radius: 8px;
    margin-bottom: 1.5rem;
    font-size: 0.75rem;
    color: rgba(255, 255, 255, 0.7);
  }

  .icon {
    width: 16px;
    height: 16px;
    color: #4caf50;
  }

  .btn-primary {
    width: 100%;
    padding: 0.875rem;
    background: linear-gradient(135deg, #4fc3f7, #29b6f6);
    color: white;
    border: none;
    border-radius: 8px;
    font-size: 1rem;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s;
  }

  .btn-primary:hover:not(:disabled) {
    transform: translateY(-2px);
    box-shadow: 0 8px 16px rgba(79, 195, 247, 0.3);
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
