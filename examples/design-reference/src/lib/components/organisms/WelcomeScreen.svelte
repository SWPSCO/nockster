<script lang="ts">
  import Logo from '../atoms/Logo.svelte';
  import Button from '../atoms/Button.svelte';
  
  export let onCreateWallet: () => void = () => {};
  export let onImportWallet: () => void = () => {};
</script>

<div class="welcome-screen fixed-screen">
  <div class="welcome-content">
    <div class="welcome-center">
      <Logo size="large" showText={false} />
      <h1 class="fletch-brand welcome-title">Fletch</h1>
      <p class="welcome-text">Your secure gateway to Nockchain</p>
    </div>
  </div>
  <div class="button-footer">
    <Button
      variant="primary"
      fullWidth={true}
      on:click={onCreateWallet}
    >
      Create New Wallet
    </Button>

    <Button
      variant="secondary"
      fullWidth={true}
      on:click={onImportWallet}
    >
      Import Wallet
    </Button>
  </div>
</div>

<style>
  .welcome-screen {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-background);
    position: relative;
  }

  .welcome-content {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
    padding-bottom: 80px; /* Shift optical center upward */
  }

  .welcome-center {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    margin-top: -40px; /* Fine-tune vertical position */
  }

  .welcome-center :global(.logo-container) {
    transform: scale(0.85); /* Refined proportion */
    margin-bottom: -8px; /* Tighter logo-to-text relationship */
  }

  .welcome-title {
    font-size: 36px;
    font-weight: 800;
    margin: 16px 0 10px;
    color: var(--color-text);
    letter-spacing: -0.3px; /* Tighter, more confident */
  }

  .welcome-text {
    font-size: 15px;
    color: var(--color-text-secondary);
    font-weight: 400;
    letter-spacing: 0.1px;
  }
  
</style>