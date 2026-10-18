<script lang="ts">
  import { onMount } from 'svelte';
  import { walletStore } from '../../stores/wallet';
  import Button from '../atoms/Button.svelte';
  import {
    readPublishedAddress,
    publishAddress,
    unpublishAddress,
    lookupShipAddress,
    type PublishedAddress
  } from '../../../platform/urbitSocial';

  export let onSave: (contact: { ship: string; address: string }) => Promise<void>;
  export let onUse: (contact: { name: string; address: string }) => void;

  let profile: PublishedAddress | null = null;
  let lookup = '';
  let found: PublishedAddress | null = null;
  let busy = false;
  let error = '';
  let status = '';
  $: activeAddress = $walletStore.activeWallet?.addresses[0];
  $: activeName = $walletStore.activeWallet?.name || 'Selected wallet';

  async function run(action: () => Promise<void>) {
    busy = true;
    error = '';
    status = '';
    try {
      await action();
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure);
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    void run(async () => {
      profile = await readPublishedAddress();
    });
  });

  async function findShip() {
    found = null;
    await run(async () => {
      found = await lookupShipAddress(lookup);
      if (!found.address) status = `${found.ship} has no published address.`;
    });
  }
</script>

<section class="ship-contacts" aria-label="Urbit contacts">
  <div class="social-card">
    <h2>Your public address</h2>
    <p>Let people find your Nockchain address by your ship name.</p>
    {#if profile}
      <strong>{profile.ship}</strong>
      {#if profile.address}
        <p class="address" aria-label="Published address">{profile.address}</p>
      {:else}
        <p>No address published.</p>
      {/if}
    {/if}
    {#if activeAddress}
      <p class="wallet-label">{activeName}</p>
      <p class="address" aria-label="Selected wallet address">{activeAddress}</p>
    {/if}
    <p class="detail">
      Publishing publicly links this address to your ship. Copies can remain after you remove it.
    </p>
    <div class="actions">
      <Button
        disabled={busy || !profile || !activeAddress || profile.address === activeAddress}
        on:click={() => {
          const address = activeAddress;
          if (address)
            void run(async () => {
              profile = await publishAddress(address);
              status = 'Your address is published.';
            });
        }}>{profile?.address ? 'Publish selected address' : 'Publish my address'}</Button
      >
      {#if profile?.address}
        <Button
          variant="secondary"
          disabled={busy}
          on:click={() =>
            run(async () => {
              profile = await unpublishAddress();
              status = 'Your address is no longer published.';
            })}>Remove published address</Button
        >
      {/if}
    </div>
  </div>

  <div class="social-card">
    <h2>Find a ship</h2>
    <form on:submit|preventDefault={findShip}>
      <label for="contact-ship">Ship name</label>
      <div class="lookup-row">
        <input
          id="contact-ship"
          placeholder="~sampel-palnet"
          autocomplete="off"
          autocapitalize="none"
          spellcheck={false}
          bind:value={lookup}
          on:input={() => {
            found = null;
            status = '';
          }}
          disabled={busy}
        />
        <Button type="submit" variant="secondary" disabled={busy || !lookup.trim()}>Look up</Button>
      </div>
    </form>
    {#if found?.address}
      <div class="lookup-result">
        <strong>{found.ship}</strong>
        <p class="address" aria-label="Ship address">{found.address}</p>
        <p class="detail">Published by this ship. Confirm the recipient before sending.</p>
        <div class="actions">
          <Button
            disabled={busy}
            on:click={() =>
              run(async () => {
                if (!found?.address) return;
                await onSave({ ship: found.ship, address: found.address });
                status = `${found.ship} saved to your contacts on this device.`;
              })}>Save contact</Button
          >
          <Button
            variant="secondary"
            disabled={busy}
            on:click={() => {
              if (found?.address) onUse({ name: found.ship, address: found.address });
            }}>Use address</Button
          >
        </div>
      </div>
    {/if}
  </div>
  {#if error}<p role="alert">{error}</p>{/if}
  <p role="status">{busy ? 'Working…' : status}</p>
</section>

<style>
  .ship-contacts {
    margin-bottom: 24px;
  }
  .social-card {
    padding: 20px;
    border: 1px solid var(--color-border);
    border-radius: 12px;
    margin-bottom: 12px;
  }
  h2 {
    font-size: 18px;
    margin: 0 0 8px;
  }
  p {
    margin: 8px 0 12px;
  }
  .detail,
  .wallet-label {
    color: var(--color-text-secondary);
    font-size: 14px;
  }
  .wallet-label {
    margin-bottom: 4px;
  }
  .address {
    overflow-wrap: anywhere;
    font-family: monospace;
    font-size: 14px;
  }
  .actions,
  .lookup-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  label {
    display: block;
    margin: 16px 0 8px;
  }
  input {
    flex: 1;
    min-width: 0;
    width: 160px;
    padding: 10px 12px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-surface);
    color: var(--color-text);
    font-size: 16px;
  }
  .lookup-result {
    margin-top: 20px;
  }
  [role='alert'] {
    color: var(--color-error);
  }
  @media (max-width: 480px) {
    .social-card {
      padding: 16px;
    }
    .actions :global(button) {
      width: 100%;
    }
  }
</style>
