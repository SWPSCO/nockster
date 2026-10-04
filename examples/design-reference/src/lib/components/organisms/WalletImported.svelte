<script lang="ts">
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';
  import ProgressBar from '../atoms/ProgressBar.svelte';

  export let onContinue: () => void = () => {};
  export let walletName: string = 'Main Wallet';
  export let onBack: () => void = () => {};
</script>

<div class="wallet-imported fixed-screen">
  <Header title="" showBack={true} showLogo={false} on:click={onBack} />
  <div class="progress-wrapper">
    <ProgressBar currentStep={2} totalSteps={2} />
  </div>

  <div class="content">
    <div class="success-container">
      <div class="import-icon">
        <svg width="64" height="64" viewBox="0 0 64 64" fill="none">
          <circle cx="32" cy="32" r="30" stroke="#666" stroke-width="2" stroke-dasharray="4 4" opacity="0.3"/>
          <circle cx="32" cy="32" r="30" stroke="#16a34a" stroke-width="2" stroke-dasharray="4 4" stroke-dashoffset="2" class="rotating"/>
          <path d="M32 20V32M32 32L26 26M32 32L38 26" stroke="#16a34a" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
          <path d="M20 40H44" stroke="#16a34a" stroke-width="2" stroke-linecap="round"/>
        </svg>
      </div>

      <h1 class="title">Wallet Imported</h1>
      <p class="message">
        Your wallet "{walletName}" has been successfully imported from your seed phrase.
      </p>

      <div class="info-box">
        <div class="info-item">
          <span class="info-label">Status</span>
          <span class="info-value success">Active</span>
        </div>
        <div class="info-item">
          <span class="info-label">Network</span>
          <span class="info-value">Nockchain</span>
        </div>
      </div>
    </div>
  </div>

  <div class="button-footer">
    <Button
      variant="primary"
      fullWidth={true}
      on:click={onContinue}
    >
      Access Wallet
    </Button>
  </div>
</div>

<style>
  .wallet-imported {
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

  .import-icon {
    margin-bottom: 32px;
    animation: fadeIn 0.6s ease;
  }

  .rotating {
    animation: rotate 8s linear infinite;
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
      transform: translateY(20px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @keyframes rotate {
    from {
      transform: rotate(0deg);
      transform-origin: center;
    }
    to {
      transform: rotate(360deg);
      transform-origin: center;
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

  .info-box {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 20px;
    background: var(--color-surface);
    border-radius: 10px;
    width: 100%;
  }

  .info-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .info-label {
    font-size: 14px;
    color: var(--color-text-tertiary);
    font-weight: 400;
  }

  .info-value {
    font-size: 14px;
    color: var(--color-text);
    font-weight: 500;
  }

  .info-value.success {
    color: var(--color-success);
  }
</style>