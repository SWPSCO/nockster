<script lang="ts">
  import Logo from '../atoms/Logo.svelte';
  import Button from '../atoms/Button.svelte';

  export let onCreateWallet: () => void = () => {};
  export let onImportWallet: () => void = () => {};
  export let onConnectHardware: () => void = () => {};
</script>

<div class="welcome-screen fixed-screen">
  <div class="welcome-content">
    <div class="welcome-center">
      <div class="logo-row">
        <Logo size="large" showText={false} />
        <img src="./nockster-logotype.svg" alt="Nockster" class="logotype" />
      </div>
      <p class="welcome-text">Your secure gateway to Nockchain</p>
    </div>
  </div>
  <div class="button-footer">
    <Button variant="primary" fullWidth={true} on:click={onCreateWallet}>Create New Wallet</Button>

    <Button variant="secondary" fullWidth={true} on:click={onConnectHardware}>
      Connect Hardware Wallet
    </Button>

    <Button variant="secondary" fullWidth={true} on:click={onImportWallet}>Import Wallet</Button>
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

  .logo-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
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

  .welcome-text {
    font-size: 15px;
    color: var(--color-text-secondary);
    font-weight: 400;
    letter-spacing: 0.1px;
  }
</style>
