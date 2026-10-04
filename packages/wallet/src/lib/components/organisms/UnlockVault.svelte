<script>
  import { unlockVault } from '../lib/stores/wallet';

  let password = '';
  let error = '';
  let isUnlocking = false;

  async function handleUnlock() {
    error = '';
    isUnlocking = true;

    try {
      const success = await unlockVault(password);
      if (!success) {
        error = 'Invalid password';
      }
    } catch (err) {
      error = err.message;
    } finally {
      isUnlocking = false;
    }
  }
</script>

<div class="unlock-vault">
  <div class="unlock-card">
    <div class="lock-icon">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
        <path d="M7 11V7a5 5 0 0110 0v4" />
      </svg>
    </div>

    <h2>Unlock Your Vault</h2>
    <p class="description">Enter your password to access your wallets</p>

    <form on:submit|preventDefault={handleUnlock}>
      <div class="form-group">
        <input
          type="password"
          bind:value={password}
          placeholder="Enter your password"
          autofocus
          required
        />
      </div>

      {#if error}
        <div class="error-message">
          {error}
        </div>
      {/if}

      <button type="submit" class="btn-primary" disabled={isUnlocking}>
        {isUnlocking ? 'Unlocking...' : 'Unlock'}
      </button>
    </form>
  </div>
</div>

<style>
  .unlock-vault {
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 450px;
  }

  .unlock-card {
    background: rgba(255, 255, 255, 0.05);
    backdrop-filter: blur(10px);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 12px;
    padding: 2rem;
    width: 100%;
    text-align: center;
  }

  .lock-icon {
    width: 64px;
    height: 64px;
    margin: 0 auto 1.5rem;
    padding: 1rem;
    background: rgba(79, 195, 247, 0.1);
    border-radius: 50%;
    color: #4fc3f7;
  }

  .lock-icon svg {
    width: 100%;
    height: 100%;
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
