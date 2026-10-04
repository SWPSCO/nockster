<script lang="ts">
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';

  export let errorMessage: string =
    'Insufficient balance. You need 50.02 but only have 45.00 available.';
  export let onTryAgain: () => void = () => {};
  export let onGoBack: () => void = () => {};
  export let onBack: () => void = () => {};
</script>

<div class="transaction-failed fixed-screen">
  <Header title="Transaction Failed" showBack={true} on:click={onBack} />

  <div class="content">
    <div class="error-container">
      <div class="error-icon">❌</div>
      <h2 class="error-title">Transaction Failed</h2>
      <p class="error-subtitle">Unable to complete your transaction</p>
    </div>

    <div class="error-message">
      <strong>Error:</strong>
      {errorMessage}
    </div>

    <div class="card">
      <h3 class="suggestions-title">Suggested Actions:</h3>
      <ul class="suggestions-list">
        <li>Reduce the transaction amount</li>
        <li>Consolidate notes to reduce fees</li>
        <li>Wait for pending transactions to confirm</li>
      </ul>
    </div>
  </div>

  <div class="button-footer">
    <Button variant="primary" fullWidth={true} on:click={onTryAgain}>Try Again</Button>

    <Button variant="secondary" fullWidth={true} on:click={onGoBack}>Go Back</Button>
  </div>
</div>

<style>
  .transaction-failed {
    height: 100%;
    background: var(--color-background);
    display: flex;
    flex-direction: column;
  }

  .content {
    flex: 1;
    padding: 16px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .error-container {
    text-align: center;
    padding: 24px 0;
  }

  .error-icon {
    font-size: 40px;
    margin-bottom: 12px;
  }

  .error-title {
    font-size: 18px;
    font-weight: 600;
    margin-bottom: 6px;
    color: var(--color-text);
  }

  .error-subtitle {
    font-size: 15px;
    color: var(--color-text-secondary);
  }

  .error-message {
    background: var(--color-error-light);
    border: 1px solid var(--color-error) 616;
    border-radius: 8px;
    padding: 16px;
    margin-bottom: 16px;
    font-size: 15px;
    color: var(--color-error);
  }

  .error-message strong {
    color: #b91c1c;
  }

  .card {
    background: var(--color-surface);
    border: 1px solid var(--color-border) 864;
    border-radius: 8px;
    padding: 16px;
    margin-bottom: 16px;
  }

  .suggestions-title {
    font-size: 15px;
    font-weight: 600;
    margin-bottom: 8px;
    color: var(--color-text);
  }

  .suggestions-list {
    font-size: 14px;
    color: var(--color-text-secondary);
    padding-left: 20px;
    margin: 0;
  }

  .suggestions-list li {
    margin-bottom: 4px;
  }

  .suggestions-list li:last-child {
    margin-bottom: 0;
  }
</style>
