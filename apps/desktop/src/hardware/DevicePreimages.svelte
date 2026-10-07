<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import {
    FEATURE_PREIMAGE_VAULT,
    MAX_VAULT_PREIMAGE_LEN,
    type VaultEntryInfo
  } from '@swps/nockster-js';
  import { hardwareSession, perform } from './session';
  import { hardwareCrypto } from './crypto';
  import { asciiLabel, parseHex } from './inputs';
  let entries: (VaultEntryInfo & { hash: string })[] = [];
  let loaded = false;
  let label = '';
  let secret = '';
  let format = 'bytes';
  let revealed = '';
  let revealedBytes = '';
  let preview = '';
  let revealHash = '';
  let removing: string | null = null;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  let destroyed = false;
  $: busy = Boolean($hardwareSession.busy);
  $: supported = Boolean(($hardwareSession.info?.features ?? 0) & FEATURE_PREIMAGE_VAULT);
  async function setEntries(values: VaultEntryInfo[]) {
    const crypto = await hardwareCrypto();
    entries = values.map(entry => ({
      ...entry,
      hash: crypto.tip5_limbs_b58(entry.commitment.map(String))
    }));
    loaded = true;
  }
  async function refresh() {
    await hardwareSession.run('Reading preimage vault…', async current =>
      setEntries(await current.vaultList())
    );
  }
  async function store() {
    const name = asciiLabel(label);
    const bytes = parseHex(secret);
    let jam: Uint8Array | undefined;
    try {
      const crypto = await hardwareCrypto();
      jam = format === 'bytes' ? crypto.jam_byte_atom(bytes) : bytes;
      if (jam.length > MAX_VAULT_PREIMAGE_LEN)
        throw new Error(`The preimage must fit in ${MAX_VAULT_PREIMAGE_LEN} bytes`);
      preview = crypto.noun_commitment_b58(jam);
      await hardwareSession.run(
        'Compare the commitment and approve storage on your Nockster',
        async current => setEntries(await current.vaultStore(name, jam!))
      );
      label = '';
    } finally {
      bytes.fill(0);
      jam?.fill(0);
      secret = '';
    }
  }
  function hide() {
    revealed = '';
    revealedBytes = '';
    revealHash = '';
    clearTimeout(hideTimer);
  }
  async function reveal(entry: VaultEntryInfo & { hash: string }) {
    hide();
    await hardwareSession.run('Approve revealing this secret on your Nockster', async current => {
      const crypto = await hardwareCrypto();
      const live = (await current.vaultList()).find(
        item =>
          item.slot === entry.slot &&
          crypto.tip5_limbs_b58(item.commitment.map(String)) === entry.hash
      );
      if (!live) throw new Error('This vault entry changed. Refresh and try again.');
      const result = await current.vaultReveal(live.slot);
      try {
        if (
          crypto.noun_commitment_b58(result.preimage) !== entry.hash ||
          crypto.tip5_limbs_b58(result.commitment.map(String)) !== entry.hash
        )
          throw new Error('The revealed secret does not match its commitment');
        if (destroyed) return;
        revealed = Array.from(result.preimage, byte => byte.toString(16).padStart(2, '0')).join('');
        try {
          const bytes = crypto.cue_byte_atom(result.preimage);
          revealedBytes = Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('');
          bytes.fill(0);
        } catch {
          revealedBytes = '';
        }
        revealHash = entry.hash;
        hideTimer = setTimeout(hide, 60000);
      } finally {
        result.preimage.fill(0);
      }
    });
  }
  async function remove(entry: VaultEntryInfo & { hash: string }) {
    hide();
    await hardwareSession.run('Approve deleting this secret on your Nockster', async current => {
      const crypto = await hardwareCrypto();
      const live = (await current.vaultList()).find(
        item =>
          item.slot === entry.slot &&
          crypto.tip5_limbs_b58(item.commitment.map(String)) === entry.hash
      );
      if (!live) throw new Error('This vault entry changed. Refresh and try again.');
      await setEntries(await current.vaultDelete(live.slot));
    });
    removing = null;
  }
  onMount(() => {
    if (supported && !busy) void perform(refresh);
  });
  onDestroy(() => {
    destroyed = true;
    hide();
    secret = '';
  });
</script>

<section>
  <div class="hw-section-heading">
    <div>
      <h3>Preimage vault</h3>
      <p>Keep hashlock secrets on your device. Reveal them only with device approval.</p>
    </div>
    {#if supported}<button class="hw-button" disabled={busy} onclick={() => perform(refresh)}
        >Refresh</button
      >{/if}
  </div>
  {#if !supported}<p class="hw-notice">This firmware does not support the preimage vault.</p>{:else}
    {#each entries as entry (entry.hash)}<article class="hw-wallet-row">
        <h3>{entry.label || `Secret ${entry.slot + 1}`}</h3>
        <p class="hw-muted">Commitment</p>
        <code class="hw-address">{entry.hash}</code>
        <div class="hw-actions">
          <button class="hw-button" disabled={busy} onclick={() => perform(() => reveal(entry))}
            >Reveal secret</button
          ><button class="hw-link hw-danger" disabled={busy} onclick={() => (removing = entry.hash)}
            >Delete</button
          >
        </div>
        {#if removing === entry.hash}<div class="hw-confirm">
            <h3>Delete this secret?</h3>
            <p>Keep a backup if you need it to unlock funds.</p>
            <div class="hw-actions">
              <button
                class="hw-button hw-danger"
                disabled={busy}
                onclick={() => perform(() => remove(entry))}>Delete secret</button
              ><button class="hw-link" disabled={busy} onclick={() => (removing = null)}
                >Cancel</button
              >
            </div>
          </div>{/if}
      </article>{:else}<p class="hw-empty">
        {loaded ? 'No secrets stored on this device.' : 'Refresh to load vault entries.'}
      </p>{/each}
    {#if revealed}<div class="hw-result">
        <h3>Revealed secret</h3>
        <p>Jammed preimage for <code>{revealHash}</code>. Hidden automatically after one minute.</p>
        <code class="hw-address">{revealed}</code>
        {#if revealedBytes}<p>Raw bytes (hex)</p>
          <code class="hw-address">{revealedBytes}</code>{/if}
        <button class="hw-button" onclick={hide}>Hide secret</button>
      </div>{/if}
    <form
      class="hw-form"
      onsubmit={event => {
        event.preventDefault();
        void perform(store);
      }}
    >
      <h3>Store a secret</h3>
      <label for="preimage-label">Label</label><input
        id="preimage-label"
        bind:value={label}
        maxlength="32"
        disabled={busy}
      /><label for="preimage-format">Format</label><select
        id="preimage-format"
        bind:value={format}
        disabled={busy}
        ><option value="bytes">Raw bytes (hex)</option><option value="jam">Jammed noun (hex)</option
        ></select
      ><label for="preimage-secret">Secret</label><textarea
        id="preimage-secret"
        bind:value={secret}
        rows="3"
        spellcheck="false"
        autocomplete="off"
        disabled={busy}
      ></textarea>
      {#if preview}<p>Commitment to compare on your device</p>
        <code class="hw-address">{preview}</code>{/if}
      <button class="hw-button hw-primary" disabled={busy || !secret.trim() || !label.trim()}
        >Store on device</button
      >
    </form>
  {/if}
</section>
