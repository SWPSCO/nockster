<script lang="ts">
  import Button from '../atoms/Button.svelte';
  import ProgressBar from '../atoms/ProgressBar.svelte';

  export let onContinue: () => void = () => {};
  export let walletName: string = 'Main Wallet';
</script>

<div class="wallet-created fixed-screen">
  <div class="progress-wrapper">
    <ProgressBar currentStep={3} totalSteps={3} />
  </div>

  <div class="content">
    <div class="success-container">
      <div class="success-icon">
        <svg width="64" height="64" viewBox="0 0 64 64" fill="none">
          <circle cx="32" cy="32" r="30" stroke="#16a34a" stroke-width="2" />
          <path
            d="M20 32L28 40L44 24"
            stroke="#16a34a"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </div>

      <h1 class="title">Wallet Created</h1>
      <p class="message">
        Your wallet "{walletName}" has been successfully created and secured.
      </p>

      <div class="reminder">
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" class="reminder-icon">
          <path d="M8 4V8L11 11" stroke="#666" stroke-width="1.5" stroke-linecap="round" />
          <circle cx="8" cy="8" r="7" stroke="#666" stroke-width="1.5" />
        </svg>
        <p class="reminder-text">
          Remember to keep your seed phrase safe. It's the only way to recover your wallet.
        </p>
      </div>
    </div>
  </div>

  <div class="button-footer">
    <Button variant="primary" fullWidth={true} on:click={onContinue}>Enter Wallet</Button>
  </div>
</div>

<style>
  .wallet-created {
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
    align-items: center;
    justify-content: center;
    padding: 24px;
  }

  .success-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    max-width: 320px;
  }

  .success-icon {
    margin-bottom: 32px;
    animation: scaleIn 0.4s cubic-bezier(0.34, 1.56, 0.64, 1);
  }

  @keyframes scaleIn {
    from {
      transform: scale(0);
      opacity: 0;
    }
    to {
      transform: scale(1);
      opacity: 1;
    }
  }

  .title {
    font-size: 24px;
    font-weight: 600;
    color: var(--color-text);
    margin-bottom: 12px;
  }

  .message {
    font-size: 15px;
    color: var(--color-text-secondary);
    line-height: 1.5;
    margin-bottom: 32px;
  }

  .reminder {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 16px;
    background: var(--color-surface);
    border-radius: 10px;
    margin-top: auto;
  }

  .reminder-icon {
    flex-shrink: 0;
    margin-top: 2px;
  }

  .reminder-text {
    font-size: 13px;
    color: var(--color-text-secondary);
    line-height: 1.4;
    text-align: left;
    margin: 0;
  }
</style>
