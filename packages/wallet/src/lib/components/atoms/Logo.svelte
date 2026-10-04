<script lang="ts">
  export let size: 'small' | 'medium' | 'large' = 'medium';
  export let showText: boolean = true;

  const sizeMap = {
    small: {
      logo: 24,
      font: 'var(--font-lg)'
    },
    medium: {
      logo: 32,
      font: 'var(--font-2xl)'
    },
    large: {
      logo: 48,
      font: 'var(--font-3xl)'
    }
  };

  $: dimensions = sizeMap[size];
</script>

<div class="logo-container">
  <img
    src="./nockster-logo.svg"
    alt="Nockster Logo"
    width={dimensions.logo}
    height={dimensions.logo}
    class="logo-icon"
  />

  {#if showText}
    <span class="logo-text nockster-brand" style="font-size: {dimensions.font}"> Nockster </span>
  {/if}
</div>

<style>
  .logo-container {
    display: inline-flex;
    align-items: center;
    gap: var(--spacing-sm);
    color: var(--color-text);
  }

  .logo-icon {
    flex-shrink: 0;
    object-fit: contain;
  }

  /* Invert logo color in dark mode */
  :global([data-theme='dark']) .logo-icon,
  :global([data-theme='dark-blue']) .logo-icon,
  :global([data-theme='purple']) .logo-icon {
    filter: invert(1);
  }

  .logo-text {
    line-height: 1;
    letter-spacing: -0.02em;
  }
</style>
