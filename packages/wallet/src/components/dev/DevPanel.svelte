<script lang="ts">
  import { onMount } from 'svelte';
  import { settingsStore } from '../../lib/stores/settings';
  import type {
    ApiResponse,
    VaultStatusPayload,
    WalletSummaryPayload,
    WalletPayload,
    TxRecipient
  } from '../../vaultApi';
  import {
    vaultStatus,
    newVault,
    wipeVault,
    checkPassword,
    unlockVault,
    lockVault,
    showLogs,
    generateKey,
    importWallet as apiImportWallet,
    renameWallet as apiRenameWallet,
    deleteWallet as apiDeleteWallet,
    getPubkey as apiGetPubkey,
    getWallets as apiGetWallets,
    exportWallet as apiExportWallet,
    createAndSignTx as apiCreateAndSignTx
  } from '../../vaultApi';
  import { NockchainRPC, type NoteV1 } from '../../lib/utils/rpc';

  type LogEntry = { timestamp: number; operation: string };

  type ActionLogEntry = {
    label: string;
    timestamp: number;
    response: ApiResponse<unknown>;
  };

  let statusResponse: ApiResponse<VaultStatusPayload> | null = null;
  let logs: LogEntry[] = [];
  let logsVisible = false;
  let generatedKey: string[] | null = null;
  let localPassword = '';
  let actionHistory: ActionLogEntry[] = [];
  let importNickname = '';
  let importKey = '';
  let renameOldNickname = '';
  let renameNewNickname = '';
  let deleteNickname = '';
  let lookupNickname = '';
  let exportNickname = '';
  let lookedUpWallet: WalletSummaryPayload | null = null;
  let walletSummaries: WalletSummaryPayload[] | null = null;
  let exportedWallet: WalletPayload | null = null;
  let txNickname = '';
  let txSimResult: string | null = null;
  let txSimError: string | null = null;
  let txSimLoading = false;
  let txAddress = '';
  let txNotes: NoteV1[] = [];
  let txNotesLoading = false;
  let txNotesError: string | null = null;
  let txRecipientsInput = `[
  {
    "address": "",
    "gift": 0
  }
]`;
  let txRecipientsError: string | null = null;
  $: txNotesTotalValue = txNotes.reduce(
    (sum, note) => sum + (typeof note.assets === 'number' ? Number(note.assets) : 0),
    0
  );
  const rpcV1 = new NockchainRPC({ version: 'v1' });
  let v1Address = '';
  let v1Notes: NoteV1[] = [];
  let v1NotesLoading = false;
  let v1NotesError: string | null = null;
  let v1RawResponse: unknown = null;

  const formatByteArray = (values: number[]) =>
    values.map(value => value.toString(16).padStart(2, '0')).join('');

  onMount(() => {
    refreshStatus();
  });

  async function refreshStatus(logResult = false) {
    const response = await vaultStatus();
    statusResponse = response;
    if (logResult) {
      actionHistory = [
        { label: 'Vault Status', timestamp: Date.now(), response },
        ...actionHistory
      ].slice(0, 6);
    }
  }

  async function runAction<T>(label: string, action: () => Promise<ApiResponse<T>>) {
    const response = await action();
    actionHistory = [{ label, timestamp: Date.now(), response }, ...actionHistory].slice(0, 6);
    await refreshStatus();
    return response;
  }

  async function createVault() {
    await runAction('Create Vault', () => newVault(localPassword));
  }

  async function wipe() {
    await runAction('Wipe Vault', wipeVault);
  }

  async function checkVaultPassword() {
    await runAction('Check Password', () => checkPassword(localPassword));
  }

  async function unlock() {
    await runAction('Unlock Vault', () => unlockVault(localPassword));
  }

  async function lock() {
    await runAction('Lock Vault', lockVault);
  }

  async function fetchLogs() {
    const response = await runAction('Show Logs', showLogs);
    if (response.success && response.data) {
      logs = response.data;
      logsVisible = true;
    }
  }

  function hideLogs() {
    logs = [];
    logsVisible = false;
  }

  async function requestKey() {
    const response = await runAction('Generate Key', generateKey);
    generatedKey = response.success && response.data ? response.data : null;
  }

  async function copyGeneratedKey() {
    if (!generatedKey) return;
    const phrase = generatedKey.join(' ');
    if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(phrase);
    }
  }

  async function fetchTxNotesForAddress() {
    const address = txAddress.trim();
    if (!address) {
      txNotesError = 'Enter an address to fetch notes.';
      return;
    }
    txNotesLoading = true;
    txNotesError = null;
    try {
      const response = await rpcV1.request<NoteV1[]>('getNotesByAddress', [
        { address, showSpent: false }
      ]);
      txNotes = Array.isArray(response) ? response : [];
      if (!txNotes.length) {
        txNotesError = 'RPC returned no notes for that address.';
      }
    } catch (error) {
      txNotes = [];
      txNotesError = error instanceof Error ? error.message : 'Failed to fetch notes';
    } finally {
      txNotesLoading = false;
    }
  }

  function parseRecipientsInput(): TxRecipient[] | null {
    txRecipientsError = null;
    const raw = txRecipientsInput.trim();
    if (!raw) {
      txRecipientsError = 'Recipients JSON cannot be empty.';
      return null;
    }
    try {
      const parsed = JSON.parse(raw);
      if (!Array.isArray(parsed)) {
        throw new Error('Recipients JSON must be an array of { address, gift } objects.');
      }
      if (parsed.length === 0) {
        throw new Error('Provide at least one recipient.');
      }
      const normalized: TxRecipient[] = parsed.map((item, index) => {
        if (!item || typeof item !== 'object') {
          throw new Error(`Recipient #${index + 1} must be an object.`);
        }
        const address =
          typeof item.address === 'string' && item.address.trim().length > 0
            ? item.address.trim()
            : '';
        const gift = Number(item.gift);
        if (!address) {
          throw new Error(`Recipient #${index + 1} is missing an address.`);
        }
        if (!Number.isFinite(gift) || gift <= 0) {
          throw new Error(`Recipient #${index + 1} must have a positive gift amount.`);
        }
        return { address, gift: Math.floor(gift) };
      });
      return normalized;
    } catch (error) {
      txRecipientsError =
        error instanceof Error ? error.message : 'Failed to parse recipients JSON.';
      return null;
    }
  }

  async function handleCreateAndSignTx() {
    const nickname = txNickname.trim();
    if (!nickname) {
      return;
    }
    if (!txNotes.length) {
      txSimError = 'Fetch notes for an address before constructing a transaction.';
      return;
    }
    const recipients = parseRecipientsInput();
    if (!recipients) {
      return;
    }
    txSimLoading = true;
    txSimResult = null;
    txSimError = null;
    const response = await runAction('createAndSignTx', () =>
      apiCreateAndSignTx(nickname, txNotes, recipients)
    );
    if (response.success && response.data) {
      txSimResult = JSON.stringify(response.data, null, 2);
    } else {
      txSimError = response.error ?? 'Unknown error';
    }
    txSimLoading = false;
  }

  function clearHistory() {
    actionHistory = [];
  }

  async function importWallet() {
    if (!importNickname || !importKey) {
      return;
    }

    const response = await runAction('Import Wallet', () =>
      apiImportWallet(importNickname, importKey)
    );
    if (response.success) {
      importNickname = '';
      importKey = '';
    }
  }

  async function renameWalletAction() {
    if (!renameOldNickname || !renameNewNickname) {
      return;
    }

    const response = await runAction('Rename Wallet', () =>
      apiRenameWallet(renameOldNickname, renameNewNickname)
    );
    if (response.success) {
      renameOldNickname = '';
      renameNewNickname = '';
    }
  }

  async function deleteWalletAction() {
    if (!deleteNickname) {
      return;
    }
    const response = await runAction('Delete Wallet', () => apiDeleteWallet(deleteNickname));
    if (response.success) {
      deleteNickname = '';
    }
  }

  async function fetchPubkey() {
    if (!lookupNickname) {
      return;
    }
    const response = await runAction('Get Pubkey', () => apiGetPubkey(lookupNickname));
    lookedUpWallet = response.success && response.data ? response.data : null;
  }

  async function fetchWalletsList() {
    const response = await runAction('Get Wallets', () => apiGetWallets());
    walletSummaries = response.success && response.data ? response.data : null;
  }

  async function exportWalletAction() {
    if (!exportNickname) {
      return;
    }
    const response = await runAction('Export Wallet', () => apiExportWallet(exportNickname));
    exportedWallet = response.success && response.data ? response.data : null;
  }

  async function fetchNotesByAddressV1() {
    const address = v1Address.trim();
    if (!address) {
      return;
    }
    v1NotesLoading = true;
    v1NotesError = null;
    v1RawResponse = null;
    try {
      const response = await rpcV1.request<NoteV1[]>('getNotesByAddress', [
        { address, showSpent: true }
      ]);
      v1Notes = Array.isArray(response) ? response : [];
      v1RawResponse = response;
    } catch (error) {
      v1Notes = [];
      v1RawResponse = null;
      v1NotesError = error instanceof Error ? error.message : 'Failed to fetch notes';
    } finally {
      v1NotesLoading = false;
    }
  }
</script>

<div class="dev-panel">
  <div class="dev-panel-header">
    <div class="dev-panel-title">Debug Panel</div>
    <button
      class="secondary"
      type="button"
      on:click={() => settingsStore.updateSetting('developerMode', false)}
    >
      Hide
    </button>
  </div>

  <section class="panel">
    <div class="panel-header">
      <h3>Vault Status</h3>
      <button class="secondary" on:click={() => refreshStatus(true)}>Refresh</button>
    </div>
    {#if statusResponse === null}
      <p>Loading...</p>
    {:else if statusResponse.success && statusResponse.data}
      <div class="status-values">
        <div>
          <span class="label">Exists</span>
          <strong>{statusResponse.data.exists ? 'Yes' : 'No'}</strong>
        </div>
        <div>
          <span class="label">Unlocked</span>
          <strong>{statusResponse.data.unlocked ? 'Yes' : 'No'}</strong>
        </div>
      </div>
    {:else}
      <p class="error">Error: {statusResponse.error}</p>
    {/if}
  </section>

  <section class="panel">
    <h3>RPC v1: getNotesByAddress</h3>
    <p class="muted">
      Queries the `/rpc/v1` endpoint directly using the new JSON-RPC surface. Requires a full
      Nockchain address (base58).
    </p>
    <label class="field">
      <span>Address</span>
      <input
        type="text"
        placeholder="nock1..."
        bind:value={v1Address}
        spellcheck="false"
        autocomplete="off"
      />
    </label>
    <button on:click={fetchNotesByAddressV1} disabled={!v1Address.trim() || v1NotesLoading}>
      {v1NotesLoading ? 'Fetching…' : 'Fetch Notes'}
    </button>
    {#if v1NotesError}
      <p class="error">{v1NotesError}</p>
    {/if}
    {#if v1Notes.length}
      <div class="notes-list">
        {#each v1Notes as note, index}
          <div class="note-card">
            <div class="note-header">
              <strong>Note #{index + 1}</strong>
              <span class="tag">v{note.version}</span>
            </div>
            <div class="note-fields">
              <div>
                <span class="label">First / Last</span>
                <strong>{note.firstName} / {note.lastName}</strong>
              </div>
              <div>
                <span class="label">Origin Page</span>
                <strong>{note.originPage}</strong>
              </div>
              <div>
                <span class="label">Assets</span>
                <strong>{note.assets}</strong>
              </div>
              <div>
                <span class="label">Coinbase</span>
                <strong>{note.isCoinbase ? 'Yes' : 'No'}</strong>
              </div>
              {#if note.version === '0'}
                <div>
                  <span class="label">Block ID</span>
                  <strong>{note.blockId}</strong>
                </div>
                <div>
                  <span class="label">Source Hash</span>
                  <strong>{note.sourceHash}</strong>
                </div>
              {:else}
                <div>
                  <span class="label">Note Keys</span>
                  <strong>
                    {#if note.noteData && Object.keys(note.noteData).length}
                      {Object.keys(note.noteData).join(', ')}
                    {:else}
                      None
                    {/if}
                  </strong>
                </div>
              {/if}
            </div>
            <details>
              <summary>Raw JSON</summary>
              <pre>{JSON.stringify(note, null, 2)}</pre>
            </details>
          </div>
        {/each}
      </div>
    {:else if v1RawResponse}
      <p class="muted">No notes returned for that address.</p>
    {/if}
  </section>

  <section class="panel">
    <h3>Wallet Import</h3>
    <label class="field">
      <span>Nickname</span>
      <input type="text" placeholder="nickname" bind:value={importNickname} />
    </label>
    <label class="field">
      <span>Seed Phrase or zprv</span>
      <input
        type="text"
        placeholder="24-word mnemonic or extended private key"
        bind:value={importKey}
      />
    </label>
    <button on:click={importWallet} disabled={!importNickname || !importKey}>Import Wallet</button>
    <p class="muted">Results appear in API log below.</p>
  </section>

  <section class="panel">
    <h3>Wallet Maintenance</h3>
    <div class="maintenance-grid">
      <div>
        <label class="field">
          <span>Old nickname</span>
          <input type="text" placeholder="current nickname" bind:value={renameOldNickname} />
        </label>
        <label class="field">
          <span>New nickname</span>
          <input type="text" placeholder="new nickname" bind:value={renameNewNickname} />
        </label>
        <button on:click={renameWalletAction} disabled={!renameOldNickname || !renameNewNickname}>
          Rename Wallet
        </button>
      </div>
      <div>
        <label class="field">
          <span>Delete nickname</span>
          <input type="text" placeholder="nickname to delete" bind:value={deleteNickname} />
        </label>
        <button on:click={deleteWalletAction} disabled={!deleteNickname}>Delete Wallet</button>
      </div>
      <div>
        <label class="field">
          <span>Lookup nickname</span>
          <input type="text" placeholder="nickname to fetch" bind:value={lookupNickname} />
        </label>
        <button on:click={fetchPubkey} disabled={!lookupNickname}>Get Pubkey</button>
        {#if lookedUpWallet}
          <p class="muted">
            Found {lookedUpWallet.nickname}: <strong>{lookedUpWallet.publicKey}</strong>
            <br />
            <small>Extended: {lookedUpWallet.extendedPublicKey}</small>
          </p>
        {:else}
          <p class="muted">No lookup result yet.</p>
        {/if}
      </div>
      <div>
        <label class="field">
          <span>Export nickname</span>
          <input type="text" placeholder="nickname to export" bind:value={exportNickname} />
        </label>
        <button on:click={exportWalletAction} disabled={!exportNickname}>Export Wallet</button>
        {#if exportedWallet}
          <div class="wallet-export">
            <p>
              <strong>Public:</strong>
              {exportedWallet.publicKey}<br />
              <strong>Extended:</strong>
              {exportedWallet.extendedPublicKey}<br />
              <strong>Private:</strong>
              {exportedWallet.privateKey}<br />
              <strong>Extended Private:</strong>
              {exportedWallet.extendedPrivateKey}<br />
              <strong>Chain Code:</strong>
              {formatByteArray(exportedWallet.chainCode)}<br />
              <strong>Depth:</strong>
              {exportedWallet.depth} &middot;
              <strong>Index:</strong>
              {exportedWallet.index}<br />
              <strong>Parent FP:</strong>
              {formatByteArray(exportedWallet.parentFingerprint)}<br />
              <strong>Version:</strong>
              {exportedWallet.version}
            </p>
            {#if exportedWallet.seedphrase}
              <div>
                <span class="label">Seedphrase</span>
                <ol class="seedphrase-grid">
                  {#each exportedWallet.seedphrase as word, index}
                    <li>
                      <span class="key-index">{index + 1}.</span>
                      <span>{word}</span>
                    </li>
                  {/each}
                </ol>
              </div>
            {:else}
              <p class="muted">Seedphrase unavailable for this wallet.</p>
            {/if}
          </div>
        {:else}
          <p class="muted">No export result.</p>
        {/if}
      </div>
    </div>
    <div class="wallet-list">
      <button class="secondary" on:click={fetchWalletsList}>Fetch Wallets</button>
      {#if walletSummaries}
        {#if walletSummaries.length}
          <ul>
            {#each walletSummaries as wallet}
              <li>
                <div>
                  <strong>{wallet.nickname}</strong>
                </div>
                <div class="wallet-keys">
                  <span>Public: {wallet.publicKey}</span>
                  <span>Extended: {wallet.extendedPublicKey}</span>
                </div>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="muted">Vault has no wallets.</p>
        {/if}
      {:else}
        <p class="muted">Wallet list not loaded.</p>
      {/if}
    </div>
  </section>

  <section class="panel">
    <h3>Transaction Sandbox (RPC)</h3>
    <p class="muted">
      Fetch spendable notes from the RPC and call
      <code>createAndSignTx</code> with real data
    </p>
    <label class="field">
      <span>Wallet nickname</span>
      <input
        type="text"
        placeholder="existing nickname"
        bind:value={txNickname}
        spellcheck="false"
      />
    </label>
    <label class="field">
      <span>Address for notes</span>
      <input
        type="text"
        placeholder="nock1..."
        bind:value={txAddress}
        spellcheck="false"
        autocomplete="off"
      />
    </label>
    <button
      class="secondary"
      on:click={fetchTxNotesForAddress}
      disabled={!txAddress.trim() || txNotesLoading}
    >
      {txNotesLoading ? 'Fetching notes…' : 'Fetch Notes'}
    </button>
    {#if txNotesError}
      <p class="error">{txNotesError}</p>
    {:else if txNotes.length}
      <p class="muted">
        Loaded {txNotes.length} note{txNotes.length === 1 ? '' : 's'} totaling {txNotesTotalValue} nicks.
      </p>
      <div class="notes-summary">
        {#each txNotes as note, index}
          <div class="notes-summary-row">
            <strong>#{index + 1}</strong>
            <span>{note.firstName}/{note.lastName}</span>
            <span>{note.assets} nicks</span>
            <span>{note.isCoinbase ? 'coinbase' : 'regular'}</span>
          </div>
        {/each}
      </div>
      <details>
        <summary>Show resposne</summary>
        <pre>{JSON.stringify(txNotes, null, 2)}</pre>
      </details>
    {/if}
    <label class="field">
      <span>Recipients (array of objects with address and gift)</span>
      <textarea rows="6" bind:value={txRecipientsInput} spellcheck="false" class="monospace"
      ></textarea>
    </label>
    {#if txRecipientsError}
      <p class="error">{txRecipientsError}</p>
    {/if}
    <button
      on:click={handleCreateAndSignTx}
      disabled={!txNickname.trim() || !txNotes.length || txSimLoading}
    >
      {txSimLoading ? 'Creating…' : 'Create + Sign Tx'}
    </button>
    {#if txSimResult}
      <p class="muted wrap">
        b64 tx: <code>{txSimResult}</code>
      </p>
    {:else if txSimError}
      <p class="error">{txSimError}</p>
    {/if}
  </section>

  <section class="panel">
    <h3>Vault Actions</h3>
    <label class="field">
      <span>Password</span>
      <input type="password" placeholder="local password" bind:value={localPassword} />
    </label>
    <div class="actions-grid">
      <button on:click={createVault}>Create</button>
      <button on:click={unlock}>Unlock</button>
      <button on:click={lock}>Lock</button>
      <button on:click={checkVaultPassword}>Check Password</button>
      <button on:click={wipe}>Wipe Vault</button>
    </div>
  </section>

  <section class="panel">
    <h3>Key Generation</h3>
    <button on:click={requestKey}>Generate 24-word key</button>
    {#if generatedKey}
      <button class="secondary" on:click={copyGeneratedKey}>Copy key</button>
      <ol class="key-list">
        {#each generatedKey as word, index}
          <li>
            <span class="key-index">{index + 1}.</span>
            <span>{word}</span>
          </li>
        {/each}
      </ol>
    {:else}
      <p class="muted">No key generated yet.</p>
    {/if}
  </section>

  <section class="panel">
    <div class="panel-header">
      <h3>Logs</h3>
      <div class="panel-actions">
        <button on:click={fetchLogs}>Show</button>
        <button class="secondary" on:click={hideLogs} disabled={!logsVisible}>Hide</button>
      </div>
    </div>
    <div class="log-container">
      {#if logsVisible}
        {#if logs.length}
          <ul class="log-list">
            {#each logs as logEntry}
              <li>
                <span class="label">{new Date(logEntry.timestamp).toLocaleString()}</span>
                <strong>{logEntry.operation}</strong>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="muted">No log entries.</p>
        {/if}
      {:else}
        <p class="muted">Logs hidden.</p>
      {/if}
    </div>
  </section>

  {#if actionHistory.length}
    <section class="panel">
      <div class="panel-header">
        <h3>API Results</h3>
        <button class="secondary" on:click={clearHistory}>Clear</button>
      </div>
      <ul class="result-list">
        {#each actionHistory as entry}
          <li class={`result-block ${entry.response.success ? 'success' : 'error'}`}>
            <div class="result-header">
              <strong>{entry.label}</strong>
              <span class="label">{new Date(entry.timestamp).toLocaleTimeString()}</span>
            </div>
            <pre>{JSON.stringify(entry.response, null, 2)}</pre>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</div>

<style>
  .dev-panel {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    color: #111;
    font-size: 0.85rem;
    width: 100%;
    max-width: 100%;
    max-height: 100%;
    overflow-y: auto;
    box-sizing: border-box;
    padding-bottom: 0.5rem;
  }

  .dev-panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0 0.25rem;
  }

  .dev-panel-title {
    font-size: 0.95rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .panel {
    border: 1px solid rgba(0, 0, 0, 0.1);
    border-radius: 8px;
    padding: 1rem;
    background: #fff;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
  }

  h3 {
    margin: 0 0 0.75rem;
    font-size: 0.9rem;
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }

  .status-values {
    display: flex;
    gap: 1rem;
    flex-wrap: wrap;
  }

  .label {
    display: block;
    font-size: 0.75rem;
    text-transform: uppercase;
    color: #555;
  }

  .error {
    color: #b00020;
  }

  .muted {
    color: #666;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin-bottom: 0.75rem;
    font-size: 0.75rem;
    text-transform: uppercase;
    color: #555;
  }

  input[type='password'] {
    padding: 0.5rem;
    border-radius: 6px;
    border: 1px solid rgba(0, 0, 0, 0.2);
    font-size: 0.95rem;
  }

  .actions-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    gap: 0.4rem;
  }

  button {
    padding: 0.35rem 0.6rem;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    background: #111;
    color: #fff;
    font-size: 0.8rem;
  }

  button.secondary {
    background: transparent;
    border: 1px solid rgba(0, 0, 0, 0.2);
    color: #111;
  }

  button:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .key-list {
    margin-top: 0.75rem;
    padding-left: 1rem;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 0.25rem 0.75rem;
  }

  .key-list li {
    display: flex;
    gap: 0.4rem;
    padding: 0.1rem 0;
  }

  .key-index {
    font-weight: 600;
    color: #555;
  }

  .log-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .log-list li {
    padding: 0.35rem 0;
    border-bottom: 1px solid rgba(0, 0, 0, 0.05);
  }

  .log-container {
    max-height: 220px;
    overflow-y: auto;
    padding-right: 0.25rem;
  }

  .panel-actions {
    display: flex;
    gap: 0.5rem;
  }

  pre {
    margin: 0;
    font-size: 0.85rem;
    max-width: 100%;
    overflow: auto;
    white-space: pre;
  }

  .result-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .result-header {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .result-block {
    background: #f7f7f7;
    border-radius: 6px;
    padding: 0.5rem;
    border-left: 4px solid rgba(0, 0, 0, 0.1);
  }

  .result-block.success {
    border-left-color: #2e7d32;
  }

  .result-block.error {
    border-left-color: #c62828;
  }

  .maintenance-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 1rem;
    margin-bottom: 1rem;
  }

  .wallet-list ul {
    list-style: none;
    padding: 0;
    margin: 0.5rem 0 0;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .wallet-list li {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.85rem;
    border-bottom: 1px solid rgba(0, 0, 0, 0.05);
    padding: 0.35rem 0;
  }

  .wallet-keys {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    color: #555;
    font-size: 0.8rem;
  }

  .wallet-export {
    margin-top: 0.5rem;
    padding: 0.5rem;
    border: 1px solid rgba(0, 0, 0, 0.1);
    border-radius: 6px;
    font-size: 0.8rem;
  }

  .seedphrase-grid {
    list-style: none;
    padding: 0;
    margin: 0.5rem 0 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    gap: 0.2rem 0.6rem;
  }

  .seedphrase-grid li {
    display: flex;
    gap: 0.25rem;
  }

  .notes-summary {
    margin: 0.75rem 0;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.8rem;
  }

  .notes-summary-row {
    display: grid;
    grid-template-columns: 40px 1fr 100px 90px;
    gap: 0.35rem;
    align-items: center;
    padding: 0.25rem 0;
    border-bottom: 1px solid rgba(0, 0, 0, 0.05);
  }

  textarea.monospace {
    font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace;
    font-size: 0.85rem;
  }

  .wrap {
    word-break: break-word;
  }
</style>
