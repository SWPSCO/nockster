<script lang="ts">
  import { onMount } from 'svelte';
  import {
    FEATURE_DEVICE_ADDRESS_BOOK,
    MAX_DEVICE_ADDRESS_BOOK_ENTRIES,
    MAX_ADDRESS_BOOK_LABEL_LEN,
    type DeviceAddressBookEntry
  } from '@swps/nockster-js';
  import { hardwareSession, perform } from './session';
  import { hardwareCrypto } from './crypto';
  import { asciiLabel } from './inputs';
  let entries: DeviceAddressBookEntry[] = [];
  let loaded = false;
  let label = '';
  let address = '';
  let removing: string | null = null;
  $: busy = Boolean($hardwareSession.busy);
  $: supported = Boolean(($hardwareSession.info?.features ?? 0) & FEATURE_DEVICE_ADDRESS_BOOK);
  async function refresh() {
    await hardwareSession.run('Reading device contacts…', async current => {
      entries = await current.getAddressBook();
      loaded = true;
    });
  }
  async function save() {
    const name = asciiLabel(label, MAX_ADDRESS_BOOK_LABEL_LEN);
    const pkh = address.trim();
    const crypto = await hardwareCrypto();
    if (!crypto.is_valid_pkh(pkh)) throw new Error('Enter a valid Nockchain address');
    await hardwareSession.run('Approve contact changes on your Nockster', async current => {
      const fresh = await current.getAddressBook();
      const index = fresh.findIndex(entry => entry.pkh === pkh);
      if (index < 0 && fresh.length >= MAX_DEVICE_ADDRESS_BOOK_ENTRIES)
        throw new Error('The device address book is full');
      if (index >= 0) fresh[index] = { label: name, pkh };
      else fresh.push({ label: name, pkh });
      await current.setAddressBook(fresh);
      entries = await current.getAddressBook();
      loaded = true;
    });
    label = '';
    address = '';
  }
  async function remove(entry: DeviceAddressBookEntry) {
    await hardwareSession.run('Approve contact removal on your Nockster', async current => {
      const fresh = await current.getAddressBook();
      const index = fresh.findIndex(item => item.pkh === entry.pkh && item.label === entry.label);
      if (index < 0)
        throw new Error('This contact changed. Refresh the device address book and try again.');
      fresh.splice(index, 1);
      await current.setAddressBook(fresh);
      entries = await current.getAddressBook();
    });
    removing = null;
  }
  onMount(() => {
    if (supported && !busy) void perform(refresh);
  });
</script>

<section>
  <div class="hw-section-heading">
    <div>
      <h3>Device contacts</h3>
      <p>Names stored on your Nockster appear during transaction review.</p>
    </div>
    {#if supported}<button class="hw-button" disabled={busy} onclick={() => perform(refresh)}
        >Refresh</button
      >{/if}
  </div>
  {#if !supported}<p class="hw-notice">This firmware does not support device contacts.</p>{:else}
    <div class="hw-list">
      {#each entries as entry (entry.pkh)}<div class="hw-list-row">
          <div><strong>{entry.label}</strong><code class="hw-address">{entry.pkh}</code></div>
          <button
            class="hw-link"
            disabled={busy}
            onclick={() => {
              label = entry.label;
              address = entry.pkh;
            }}>Edit</button
          ><button class="hw-link hw-danger" disabled={busy} onclick={() => (removing = entry.pkh)}
            >Remove</button
          >
          {#if removing === entry.pkh}<div class="hw-confirm">
              <p>Remove {entry.label} from your device?</p>
              <div class="hw-actions">
                <button
                  class="hw-button hw-danger"
                  disabled={busy}
                  onclick={() => perform(() => remove(entry))}>Remove contact</button
                ><button class="hw-link" disabled={busy} onclick={() => (removing = null)}
                  >Cancel</button
                >
              </div>
            </div>{/if}
        </div>{:else}<p class="hw-empty">
          {loaded ? 'No contacts saved on your device.' : 'Refresh to load device contacts.'}
        </p>{/each}
    </div>
    <form
      class="hw-form"
      onsubmit={event => {
        event.preventDefault();
        void perform(save);
      }}
    >
      <h3>Save a contact</h3>
      <label for="device-contact-name">Name</label><input
        id="device-contact-name"
        bind:value={label}
        maxlength={MAX_ADDRESS_BOOK_LABEL_LEN}
        disabled={busy}
      />
      <label for="device-contact-address">Nockchain address</label><input
        id="device-contact-address"
        bind:value={address}
        spellcheck="false"
        disabled={busy}
      />
      <p class="hw-muted">Saving an existing address updates its name.</p>
      <button class="hw-button hw-primary" disabled={busy || !label.trim() || !address.trim()}
        >Save on device</button
      >
    </form>
  {/if}
</section>
