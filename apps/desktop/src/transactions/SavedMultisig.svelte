<script lang="ts" context="module">
  import { writable } from 'svelte/store';
  import type { SavedMultisig } from './multisig-wallets';

  // Shared by every picker on the page; loaded from storage once.
  const saved = writable<SavedMultisig[]>([]);
  let loading: Promise<void> | undefined;
</script>

<script lang="ts">
  import { onMount } from 'svelte';
  import {
    loadFromStorage,
    persistToStorage
  } from '../../../../packages/wallet/src/lib/utils/storage';
  import { hardwareCrypto, download } from '../hardware/crypto';
  import {
    addMultisigs,
    findMultisig,
    multisigFromSetup,
    removeMultisig,
    savedMultisig,
    setupFile,
    setupFileName,
    storedMultisigs
  } from './multisig-wallets';

  export let id: string;
  export let threshold: string;
  export let signers: string;

  let wasm: Awaited<ReturnType<typeof hardwareCrypto>> | undefined;
  let name = '';
  let message = '';
  let error = '';
  let copied = false;

  $: current = wasm
    ? findMultisig(
        $saved,
        threshold,
        signers,
        wasm.is_valid_pkh,
        wasm.compose_tx_v1_recipient_address
      )
    : undefined;
  $: address = lockAddress(wasm, threshold, signers);

  function lockAddress(crypto: typeof wasm, required: string, list: string): string {
    if (!crypto || !list.trim()) return '';
    try {
      return savedMultisig(
        '-',
        required,
        list,
        crypto.is_valid_pkh,
        crypto.compose_tx_v1_recipient_address
      ).address;
    } catch {
      return '';
    }
  }

  onMount(async () => {
    try {
      wasm = await hardwareCrypto();
      const crypto = wasm;
      await (loading ??= loadFromStorage('multisigWallets').then(value =>
        saved.set(
          storedMultisigs(value, crypto.is_valid_pkh, crypto.compose_tx_v1_recipient_address)
        )
      ));
    } catch (cause) {
      fail(cause);
    }
  });

  function fail(cause: unknown) {
    message = '';
    error = cause instanceof Error ? cause.message : String(cause);
  }

  async function store(next: SavedMultisig[], note: string) {
    await persistToStorage('multisigWallets', next);
    saved.set(next);
    error = '';
    message = note;
  }

  function choose(event: Event) {
    const wallet = $saved.find(
      item => item.address === (event.currentTarget as HTMLSelectElement).value
    );
    message = error = '';
    if (!wallet) return;
    signers = wallet.pkhs.join('\n');
    threshold = String(wallet.m);
  }

  async function save() {
    try {
      if (!wasm) throw new Error('The wallet engine is still loading.');
      const wallet = savedMultisig(
        name,
        threshold,
        signers,
        wasm.is_valid_pkh,
        wasm.compose_tx_v1_recipient_address
      );
      await store(addMultisigs($saved, [wallet]), `Saved “${wallet.name}”.`);
      name = '';
    } catch (cause) {
      fail(cause);
    }
  }

  async function remove() {
    if (!current) return;
    try {
      await store(removeMultisig($saved, current.address), `Removed “${current.name}”.`);
    } catch (cause) {
      fail(cause);
    }
  }

  function exportSetup() {
    if (!current) return;
    download(
      new TextEncoder().encode(setupFile(current)),
      setupFileName(current),
      'application/json'
    );
  }

  async function importSetup(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    try {
      if (!wasm) throw new Error('The wallet engine is still loading.');
      const wallet = multisigFromSetup(
        await file.text(),
        wasm.is_valid_pkh,
        wasm.compose_tx_v1_recipient_address
      );
      await store(addMultisigs($saved, [wallet]), `Imported “${wallet.name}”.`);
      signers = wallet.pkhs.join('\n');
      threshold = String(wallet.m);
    } catch (cause) {
      fail(cause);
    }
  }

  async function copy() {
    await navigator.clipboard.writeText(address);
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }
</script>

<div class="saved-multisig">
  <label for="{id}-saved"
    >Saved multisig<select id="{id}-saved" value={current?.address || ''} on:change={choose}>
      <option value="">{$saved.length ? 'Not saved' : 'None saved yet'}</option>
      {#each $saved as wallet}<option value={wallet.address}
          >{wallet.name} · {wallet.m} of {wallet.pkhs.length}</option
        >{/each}
    </select></label
  >
  {#if address}
    <p class="mono source-address">
      {address}
      <button type="button" class="subtle" on:click={copy}
        >{copied ? 'Copied' : 'Copy address'}</button
      >
    </p>
  {/if}
  <div class="saved-multisig-actions">
    {#if current}
      <button type="button" class="subtle" on:click={exportSetup}>Export setup</button>
      <button type="button" class="subtle" on:click={remove}>Remove</button>
    {:else if address}
      <label for="{id}-name" class="saved-multisig-name"
        >Name<input id="{id}-name" bind:value={name} maxlength="64" placeholder="Treasury" /></label
      >
      <button type="button" class="subtle" disabled={!name.trim()} on:click={save}
        >Save multisig</button
      >
    {/if}
    <label for="{id}-import" class="saved-multisig-import"
      >Import setup<input
        id="{id}-import"
        type="file"
        accept=".json,application/json"
        on:change={importSetup}
      /></label
    >
  </div>
  {#if error}<p class="transaction-error" role="alert">{error}</p>{/if}
  {#if message}<p class="field-hint" role="status">{message}</p>{/if}
  <p class="field-hint">
    A setup file holds only the signer addresses and required signatures. Share it with co-signers
    so everyone spends from the same address.
  </p>
</div>
