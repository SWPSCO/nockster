<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import Logo from '../atoms/Logo.svelte';
  import Icon from '../atoms/Icon.svelte';
  import Button from '../atoms/Button.svelte';

  export let title: string = '';
  export let showBack: boolean = false;
  export let showLogo: boolean = false;
  export let showActions: boolean = false;

  const dispatch = createEventDispatcher();

  function handleBack() {
    dispatch('click');
  }
</script>

<header class="header">
  <div class="header-left">
    {#if showBack}
      <Button variant="ghost" size="small" on:click={handleBack}>
        <Icon name="chevronLeft" />
      </Button>
    {/if}
    {#if showLogo}
      <Logo size="small" showText={true} />
    {/if}
  </div>
  
  {#if title}
    <h1 class="header-title">{title}</h1>
  {/if}
  
  <div class="header-right">
    {#if showActions}
      <slot name="actions" />
    {/if}
  </div>
</header>

<style>
  .header {
    height: 60px;
    background: var(--color-background);
    border-bottom: 1px solid var(--color-border)76;
    display: flex;
    align-items: center;
    padding: 0 20px;
    justify-content: space-between;
    position: sticky;
    top: 0;
    z-index: 20;
  }
  
  .header-left {
    display: flex;
    align-items: center;
    gap: 12px;
    flex: 1;
  }
  
  .header-title {
    font-size: 18px;
    font-weight: 600;
    color: var(--color-text);
    text-align: center;
    flex: 0 0 auto;
  }
  
  .header-right {
    display: flex;
    align-items: center;
    gap: 12px;
    justify-content: flex-end;
    flex: 1;
  }
</style>