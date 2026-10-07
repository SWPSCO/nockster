<script lang="ts">
  import SendTransaction from '../../../../packages/wallet/src/lib/components/organisms/SendTransaction.svelte';
  import { activeWallet } from '../../../../packages/wallet/src/lib/stores/wallet';
  import { router } from '../../../../packages/wallet/src/lib/stores/router';
  import { nicksToNocks } from '../../../../packages/wallet/src/lib/utils/nicks';
  import Composer from './Composer.svelte';
  import './transactions.css';

  let mode: 'payment' | 'compose' = 'payment';
  let composerOpened = false;
  $: if (mode === 'compose') composerOpened = true;
  function changeTab(event: KeyboardEvent) {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    mode =
      event.key === 'Home'
        ? 'payment'
        : event.key === 'End'
          ? 'compose'
          : mode === 'payment'
            ? 'compose'
            : 'payment';
    document.getElementById(`${mode}-tab`)?.focus();
  }
</script>

<div class="desktop-send fixed-screen" class:composing={mode === 'compose'}>
  <header class="transaction-heading">
    <h1>Send</h1>
    <div class="transaction-tabs" role="tablist" aria-label="Send mode">
      <button
        role="tab"
        on:keydown={changeTab}
        tabindex={mode === 'payment' ? 0 : -1}
        aria-selected={mode === 'payment'}
        aria-controls="payment-panel"
        id="payment-tab"
        on:click={() => (mode = 'payment')}>Payment</button
      >
      <button
        role="tab"
        on:keydown={changeTab}
        tabindex={mode === 'compose' ? 0 : -1}
        aria-selected={mode === 'compose'}
        aria-controls="compose-panel"
        id="compose-tab"
        on:click={() => (mode = 'compose')}>Compose</button
      >
    </div>
  </header>
  <div id="payment-panel" role="tabpanel" aria-labelledby="payment-tab" hidden={mode !== 'payment'}>
    <SendTransaction
      desktopLayout
      balance={nicksToNocks(BigInt(Math.trunc($activeWallet?.balance || 0)))}
      onBack={() => router.navigate('dashboard')}
      onSend={data => router.navigate('confirm-transaction', data)}
    />
  </div>
  <div id="compose-panel" role="tabpanel" aria-labelledby="compose-tab" hidden={mode !== 'compose'}>
    {#if composerOpened}<Composer />{/if}
  </div>
</div>
