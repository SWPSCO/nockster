<script lang="ts">
  import { tick } from 'svelte';
  import { FEATURE_SEED_LABELS, FEATURE_MASTER_PUBKEY_EXPORT } from '@swps/nockster-js';
  import { walletStore } from '../../../../packages/wallet/src/lib/stores/wallet';
  import { router } from '../../../../packages/wallet/src/lib/stores/router';
  import type { HardwareAddress } from '../../../../packages/wallet/src/lib/utils/hardwareDevice';
  import { hardwareSession, perform } from './session';
  import { hardwareCrypto, download } from './crypto';
  import { asciiLabel, readFile, seedFromPhrase } from './inputs';

  let adding = false;
  let source = '';
  let passphrase = '';
  let pin = '';
  let pinConfirm = '';
  let rename: string | null = null;
  let label = '';
  let deleting: string | null = null;
  let deleteWord = '';
  let exported = '';
  let notice = '';
  let importForm: HTMLFormElement;
  let seedInput: HTMLTextAreaElement;
  async function openImport() {
    adding = true;
    await tick();
    importForm?.scrollIntoView({ block: 'start' });
    seedInput?.focus({ preventScroll: true });
  }
  $: roots = $hardwareSession.addresses.filter(entry => entry.path.length === 0);
  $: initial = !$hardwareSession.info?.has_seed;
  $: busy = Boolean($hardwareSession.busy);
  $: features = $hardwareSession.info?.features ?? 0;

  function useWallet(entry: HardwareAddress) {
    walletStore.upsertHardwareWallet(
      entry.address,
      $hardwareSession.labels[entry.slot] || `Nockster · wallet ${entry.slot + 1}`,
      { product: $hardwareSession.descriptor?.name, transport: 'tauri' },
      true
    );
    router.navigate('dashboard');
  }
  async function verify(entry: HardwareAddress) {
    await hardwareSession.run('Verify this address on your Nockster', async current => {
      await hardwareSession.refresh(current);
      const live = $hardwareSession.addresses.find(item => item.address === entry.address);
      if (!live) throw new Error('This wallet is no longer on the connected device');
      await current.showAddress(live.slot, live.path);
    });
    notice = 'Address verified on your device.';
  }
  async function saveLabel(entry: HardwareAddress) {
    const name = asciiLabel(label);
    await hardwareSession.run('Saving wallet name…', async current => {
      await hardwareSession.refresh(current);
      const live = $hardwareSession.addresses.find(
        item => item.address === entry.address && !item.path.length
      );
      if (!live) throw new Error('This wallet is no longer on the device');
      await current.setSeedLabel(live.slot, name);
      await hardwareSession.refresh(current);
    });
    rename = null;
  }
  async function remove(entry: HardwareAddress) {
    if (deleteWord !== 'DELETE') return;
    await hardwareSession.run('Confirm wallet deletion on your Nockster', async current => {
      await hardwareSession.refresh(current);
      const live = $hardwareSession.addresses.find(
        item => item.address === entry.address && !item.path.length
      );
      if (!live) throw new Error('This wallet is no longer on the device');
      await current.deleteSeed(live.slot);
      await hardwareSession.refresh(current);
    });
    deleting = null;
    deleteWord = '';
    notice = 'Wallet removed from the device. Its public account remains in the app.';
  }
  async function exportPublic(entry: HardwareAddress) {
    exported = '';
    await hardwareSession.run('Approve public key export on your Nockster', async current => {
      await hardwareSession.refresh(current);
      const live = $hardwareSession.addresses.find(
        item => item.address === entry.address && !item.path.length
      );
      if (!live) throw new Error('This wallet is no longer on the device');
      const pub = await current.getMasterPubkey(live.slot);
      const crypto = await hardwareCrypto();
      exported = crypto.master_pubkey_to_zpub(pub.x.map(String), pub.y.map(String), pub.chain_code);
      download(
        crypto.build_master_pubkey_export(pub.x.map(String), pub.y.map(String), pub.chain_code),
        'master-pubkey.export'
      );
    });
  }
  function clearImport() {
    source = '';
    passphrase = '';
    pin = '';
    pinConfirm = '';
    adding = false;
  }
  async function importSeed() {
    const input = source.trim();
    const phrasePassphrase = passphrase;
    const enteredPin = pin;
    if (initial && (!enteredPin || enteredPin !== pinConfirm))
      throw new Error('Enter and confirm the same device PIN');
    await hardwareSession.run(
      'Importing wallet. Follow the instructions on your Nockster.',
      async current => {
        let secret: Uint8Array | undefined;
        try {
          if (input.startsWith('zprv')) {
            if (initial)
              throw new Error(
                'Set up the device with a recovery phrase and PIN before adding an extended key'
              );
            const crypto = await hardwareCrypto();
            const info = crypto.extended_key_info(input) as {
              protocol_version: number;
              kind: string;
            };
            if (info.protocol_version !== 1)
              throw new Error('Use a Nockchain protocol v1 extended key');
            secret = crypto.zprv_to_coil(input);
            await current.addCoil(secret);
          } else {
            secret = await seedFromPhrase(input, phrasePassphrase);
            if (initial) await current.initializePIN(enteredPin, secret);
            else await current.addSeed(secret);
          }
          await hardwareSession.refresh(current);
          notice = 'Wallet imported into your Nockster.';
        } finally {
          secret?.fill(0);
          clearImport();
        }
      }
    );
  }
  async function loadKeyfile(file: File) {
    const bytes = await readFile(file);
    try {
      const crypto = await hardwareCrypto();
      const parsed = crypto.parse_wallet_keyfile(bytes) as { seedphrases: string[] };
      if (parsed.seedphrases.length !== 1)
        throw new Error(
          'Choose a keyfile containing exactly one recovery phrase, or enter the phrase yourself'
        );
      source = parsed.seedphrases[0];
      notice = 'Recovery phrase loaded. Review it before importing.';
    } finally {
      bytes.fill(0);
    }
  }
</script>

<section>
  <div class="hw-section-heading">
    <div>
      <h2>Device wallets</h2>
      <p>Use a wallet in the app or verify its address on your device.</p>
    </div>
    <button class="hw-button" disabled={busy} onclick={() => perform(hardwareSession.refreshInfo)}
      >Refresh</button
    >
  </div>
  {#if notice}<p class="hw-notice" role="status">{notice}</p>{/if}
  {#each roots as entry (entry.address)}
    <article class="hw-wallet-row">
      <div class="hw-wallet-title">
        <h3>{$hardwareSession.labels[entry.slot] || `Wallet ${entry.slot + 1}`}</h3>
        <small>Slot {entry.slot}</small>
      </div>
      <code class="hw-address">{entry.address}</code>
      <div class="hw-actions">
        <button class="hw-button hw-primary" disabled={busy} onclick={() => useWallet(entry)}
          >Use wallet</button
        >
        <button class="hw-button" disabled={busy} onclick={() => perform(() => verify(entry))}
          >Verify address</button
        >
        {#if features & FEATURE_SEED_LABELS}<button
            class="hw-link"
            disabled={busy}
            onclick={() => {
              rename = entry.address;
              label = $hardwareSession.labels[entry.slot] || '';
            }}>Rename</button
          >{/if}
      </div>
      <details class="hw-details">
        <summary>Wallet options</summary>
        <div class="hw-actions">
          {#if features & FEATURE_MASTER_PUBKEY_EXPORT}<button
              class="hw-button"
              disabled={busy}
              onclick={() => perform(() => exportPublic(entry))}>Export public key</button
            >{/if}
          <button
            class="hw-link hw-danger"
            disabled={busy}
            onclick={() => {
              deleting = entry.address;
              deleteWord = '';
            }}>Remove from device</button
          >
        </div>
        {#each $hardwareSession.addresses.filter(item => item.slot === entry.slot && item.path.length) as derived}
          <div class="hw-derived">
            <code class="hw-address">{derived.address}</code><button
              class="hw-link"
              disabled={busy}
              onclick={() => perform(() => verify(derived))}>Verify derived address</button
            >
          </div>
        {/each}
      </details>
      {#if rename === entry.address}
        <form
          class="hw-inline-form"
          onsubmit={event => {
            event.preventDefault();
            void perform(() => saveLabel(entry));
          }}
        >
          <label for={`name-${entry.slot}`}>Name on device</label><input
            id={`name-${entry.slot}`}
            bind:value={label}
            maxlength="32"
            disabled={busy}
          />
          <div class="hw-actions">
            <button class="hw-button hw-primary" disabled={busy || !label.trim()}>Save name</button
            ><button class="hw-link" type="button" disabled={busy} onclick={() => (rename = null)}
              >Cancel</button
            >
          </div>
        </form>
      {/if}
      {#if deleting === entry.address}
        <form
          class="hw-inline-form"
          onsubmit={event => {
            event.preventDefault();
            void perform(() => remove(entry));
          }}
        >
          <h3>Remove this wallet from Nockster?</h3>
          <p>
            You need its recovery phrase or private key to restore access. This removes the seed
            from your device.
          </p>
          <label for={`delete-${entry.slot}`}>Type DELETE to confirm</label><input
            id={`delete-${entry.slot}`}
            bind:value={deleteWord}
            autocomplete="off"
            disabled={busy}
          />
          <div class="hw-actions">
            <button class="hw-button hw-danger" disabled={busy || deleteWord !== 'DELETE'}
              >Remove wallet</button
            ><button class="hw-link" type="button" disabled={busy} onclick={() => (deleting = null)}
              >Cancel</button
            >
          </div>
        </form>
      {/if}
    </article>
  {:else}<div class="hw-empty">
      <h3>Set up your first wallet</h3>
      <p>Import a 24-word recovery phrase and choose a device PIN to get started.</p>
    </div>{/each}
  {#if exported}<div class="hw-result">
      <h3>Master public key</h3>
      <p>The export file is downloaded. This key reveals the wallet’s unhardened address tree.</p>
      <code class="hw-address">{exported}</code>
    </div>{/if}
  {#if adding || initial}
    <form
      class="hw-form hw-import"
      bind:this={importForm}
      onsubmit={event => {
        event.preventDefault();
        void perform(importSeed);
      }}
    >
      <h3>{initial ? 'Set up Nockster' : 'Import a wallet'}</h3>
      <label for="device-seed"
        >{initial
          ? '24-word recovery phrase'
          : 'Recovery phrase or zprv extended private key'}</label
      >
      <textarea
        id="device-seed"
        bind:this={seedInput}
        bind:value={source}
        rows="3"
        spellcheck="false"
        autocomplete="off"
        disabled={busy}
      ></textarea>
      <label class="hw-file-label"
        >Load keys.export<input
          type="file"
          accept=".export"
          disabled={busy}
          onchange={event => {
            const file = event.currentTarget.files?.[0];
            if (file) void perform(() => loadKeyfile(file));
            event.currentTarget.value = '';
          }}
        /></label
      >
      {#if !source.trim().startsWith('zprv')}
        <label for="device-passphrase"
          >BIP39 passphrase <span class="hw-muted">(optional)</span></label
        ><input
          id="device-passphrase"
          type="password"
          bind:value={passphrase}
          autocomplete="off"
          disabled={busy}
        />
      {/if}
      {#if initial}<div class="hw-columns">
          <div>
            <label for="new-device-pin">Device PIN</label><input
              id="new-device-pin"
              type="password"
              bind:value={pin}
              autocomplete="new-password"
              maxlength="32"
              disabled={busy}
            />
          </div>
          <div>
            <label for="confirm-device-pin">Confirm PIN</label><input
              id="confirm-device-pin"
              type="password"
              bind:value={pinConfirm}
              autocomplete="new-password"
              maxlength="32"
              disabled={busy}
            />
          </div>
        </div>{/if}
      <p class="hw-muted">Import secrets stay in memory and are cleared after the request.</p>
      <div class="hw-actions">
        <button class="hw-button hw-primary" disabled={busy || !source.trim()}
          >{initial ? 'Set up device' : 'Import wallet'}</button
        >{#if !initial}<button type="button" class="hw-link" disabled={busy} onclick={clearImport}
            >Cancel</button
          >{/if}
      </div>
    </form>
  {:else}<button class="hw-button hw-add" disabled={busy} onclick={openImport}
      >Import another wallet</button
    >{/if}
</section>
