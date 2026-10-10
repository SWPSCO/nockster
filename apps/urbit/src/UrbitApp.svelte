<script lang="ts">
  import App from '../../../packages/wallet/src/App.svelte';
  import { browserStorage } from '../../../packages/wallet/src/platform/browserStorage';
  import {
    encryptBackup,
    decryptBackup,
    restoreBackup,
    readShipBackup,
    writeShipBackup,
    vaultKey,
    maxBackupBytes
  } from './backup';

  let showBackup = false;
  let passphrase = '';
  let confirmation = '';
  let busy = false;
  let status = '';
  const onShip = location.pathname.startsWith('/apps/nockster');

  async function run(action: () => Promise<void>) {
    busy = true;
    status = '';
    try {
      await action();
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    } finally {
      busy = false;
      passphrase = '';
      confirmation = '';
    }
  }

  function download(text: string) {
    const url = URL.createObjectURL(new Blob([text], { type: 'application/octet-stream' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = 'nockster-backup.json';
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  async function createBackup(): Promise<string> {
    if (passphrase !== confirmation) throw new Error('Backup passphrases do not match.');
    const stored = browserStorage.getItem(vaultKey);
    if (!stored) throw new Error('Create or import a wallet first.');
    return encryptBackup(JSON.parse(stored), passphrase);
  }

  async function saveToShip() {
    const remote = await readShipBackup();
    if (
      remote.text &&
      !confirm('Replace the encrypted backup on this ship? Keep a downloaded copy first.')
    )
      return;
    const text = await createBackup();
    await writeShipBackup(text, remote.revision);
    status = 'Encrypted backup saved to your ship.';
  }

  async function restore(text: string) {
    const vault = await decryptBackup(text, passphrase);
    restoreBackup(browserStorage, vault);
    location.reload();
  }

  function selectFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    void run(async () => {
      if (file.size > maxBackupBytes) throw new Error('Wallet backup is too large.');
      await restore(await file.text());
    });
    input.value = '';
  }
</script>

<div class="urbit-shell">
  <header class="urbit-toolbar">
    <span class="nockster-brand">nockster</span>
    <button
      disabled={busy}
      onclick={() => {
        showBackup = !showBackup;
        passphrase = '';
        confirmation = '';
        status = '';
      }}
    >
      {showBackup ? 'Wallet' : 'Backups'}
    </button>
  </header>
  <!-- Keep the wallet lifecycle mounted while the backup panel is open. -->
  <div class="urbit-wallet" hidden={showBackup}><App /></div>
  {#if showBackup}
    <section class="backup-panel" aria-label="Encrypted backups">
      <h1>Encrypted backups</h1>
      <p>
        Back up your software wallets. Keep your seed phrases separately. Settings, watch-only
        wallets, and hardware wallets stay in this browser.
      </p>
      <p>
        Your backup passphrase stays in this browser. Restoring also requires your wallet password
        when you unlock.
      </p>
      {#if onShip}
        <p>
          The ship stores encrypted data, but also serves this page. If you do not trust the ship to
          serve safe code, download the encrypted file and restore it using a trusted copy of
          Nockster.
        </p>
      {/if}
      <label for="backup-passphrase">Backup passphrase (at least 16 characters)</label>
      <input
        id="backup-passphrase"
        type="password"
        autocomplete="off"
        bind:value={passphrase}
        disabled={busy}
      />
      <label for="backup-confirmation">Confirm passphrase when creating a backup</label>
      <input
        id="backup-confirmation"
        type="password"
        autocomplete="off"
        bind:value={confirmation}
        disabled={busy}
      />
      <div class="backup-actions">
        <button
          disabled={busy}
          onclick={() =>
            run(async () => {
              download(await createBackup());
              status = 'Encrypted backup downloaded.';
            })}>Download backup</button
        >
        {#if onShip}
          <button disabled={busy} onclick={() => run(saveToShip)}>Save to ship</button>
          <button
            disabled={busy}
            onclick={() =>
              run(async () => {
                const remote = await readShipBackup();
                if (!remote.text) throw new Error('No backup on this ship.');
                download(remote.text);
                status = 'Ship backup downloaded. No password is needed to download ciphertext.';
              })}>Download ship backup</button
          >
          <button
            disabled={busy}
            onclick={() =>
              run(async () => {
                const remote = await readShipBackup();
                if (!remote.text) throw new Error('No backup on this ship.');
                await restore(remote.text);
              })}>Restore from ship</button
          >
        {/if}
      </div>
      <p>Restore only into a browser with no local vault.</p>
      <label for="backup-file">Restore encrypted file</label>
      <input
        id="backup-file"
        type="file"
        accept=".json,application/json,application/octet-stream"
        disabled={busy}
        onchange={selectFile}
      />
      <p role="status">{busy ? 'Working…' : status}</p>
    </section>
  {/if}
</div>
