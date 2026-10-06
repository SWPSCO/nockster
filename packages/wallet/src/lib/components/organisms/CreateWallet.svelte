<script lang="ts">
  import { onDestroy, tick } from 'svelte';
  import { get } from 'svelte/store';
  import { walletStore } from '../../stores/wallet';
  import { nextWalletName } from '../../utils/walletName';
  import { generateMnemonic } from '../../utils/vaultBridge';
  import { validateWalletKey } from '../../../vaultController';
  import {
    VanitySearch,
    initialVanityProgress,
    type WalletCandidate,
    type RecoveryKind
  } from '../../services/vanity';
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';

  export let onReady: (candidate: WalletCandidate, name: string) => Promise<void>;
  export let onBack: () => void;
  let name = nextWalletName(get(walletStore).wallets.map(wallet => wallet.name));
  let custom = false;
  let prefix = '';
  let insensitive = false;
  let keyMode: RecoveryKind = 'mnemonic';
  let backend: 'auto' | 'cpu' = 'auto';
  let lanes = 4096;
  let steps = 1;
  let maxAttempts = 0;
  let progress = initialVanityProgress();
  let busy = false;
  let error = '';
  let disposed = false;
  let advanced = false;
  let progressElement: HTMLDivElement | undefined;
  const search = new VanitySearch(value => {
    const changedStatus = progress.status !== value.status;
    progress = value;
    if (changedStatus && ['mining', 'found', 'exhausted', 'error'].includes(value.status)) {
      void tick().then(() => {
        if (!disposed) progressElement?.scrollIntoView({ block: 'nearest' });
      });
    }
  });
  $: nameExists = $walletStore.wallets.some(wallet => wallet.name === name.trim());
  $: mining = progress.status === 'mining';
  $: found = progress.status === 'found';
  $: canStart = !!name.trim() && !nameExists && !busy;

  function resetSearch() {
    search.clear();
    error = '';
  }
  function changeRecovery() {
    lanes = keyMode === 'raw' ? 64 : 4096;
    resetSearch();
  }
  async function continueWith(candidate: WalletCandidate) {
    busy = true;
    error = '';
    try {
      const address = await validateWalletKey(candidate.key);
      if (disposed) return;
      if (candidate.address && candidate.address !== address)
        throw new Error('The wallet address does not match the mining result.');
      await onReady({ ...candidate, address }, name.trim());
    } catch (failure) {
      error = failure instanceof Error ? failure.message : 'Unable to create wallet.';
    } finally {
      candidate.key = '';
      busy = false;
    }
  }
  async function generate() {
    if (!canStart) return;
    busy = true;
    const result = await generateMnemonic();
    busy = false;
    if (disposed) {
      result.mnemonic.fill('');
      return;
    }
    if (!result.success) {
      error = result.error || 'Unable to generate recovery phrase.';
      return;
    }
    await continueWith({ kind: 'mnemonic', key: result.mnemonic.join(' ') });
    result.mnemonic.fill('');
  }
  function start() {
    error = '';
    advanced = false;
    try {
      search.start({ prefix, insensitive, keyMode, backend, lanes, steps, maxAttempts });
    } catch (failure) {
      error = failure instanceof Error ? failure.message : 'Unable to start the search.';
    }
  }
  onDestroy(() => {
    disposed = true;
    search.clear();
  });
</script>

<div class="create-wallet fixed-screen">
  <Header title="" showBack={true} showLogo={false} on:click={onBack} />
  <div class="create-content">
    <h2>Create Wallet</h2>
    <p class="subtitle">A new wallet, with an address of your own.</p>
    <label class="field" for="new-wallet-name"
      >Wallet name
      <input
        id="new-wallet-name"
        bind:value={name}
        maxlength="20"
        disabled={busy || mining}
        aria-invalid={nameExists}
      />
    </label>
    {#if nameExists}<p class="error" role="status">A wallet with this name already exists.</p>{/if}
    <div class="custom-choice">
      <label class="switch-row"
        ><span><strong>Custom address</strong><small>Choose how your address starts</small></span
        ><input
          type="checkbox"
          bind:checked={custom}
          on:change={resetSearch}
          disabled={busy || mining}
        /></label
      >
    </div>
    {#if custom}
      <fieldset disabled={mining || busy} class="mining-options">
        <label class="field" for="vanity-prefix"
          >Address starts with
          <input
            id="vanity-prefix"
            class="mono"
            bind:value={prefix}
            on:input={resetSearch}
            placeholder="e.g. nock"
            maxlength="55"
            autocomplete="off"
            spellcheck="false"
          />
        </label>
        <label class="check-row"
          ><input type="checkbox" bind:checked={insensitive} on:change={resetSearch} /> Ignore case and
          match letter / digit equivalents</label
        >
        {#if insensitive}<p class="hint">
            a / 4 · b / 8 · e / 3 · i / 1 · l / 1 · o / 0 · s / 5 · t / 7 · z / 2<br />i and l stay
            distinct.
          </p>{/if}
        <label class="field" for="vanity-recovery"
          >Recovery
          <select
            id="vanity-recovery"
            aria-label="Recovery"
            bind:value={keyMode}
            on:change={changeRecovery}
            ><option value="mnemonic">24-word seed phrase</option><option value="raw"
              >Secret key (no phrase)</option
            ></select
          >
        </label>
        <p class="hint">Seed phrases take longer to find. A secret key has no recovery phrase.</p>
        <details bind:open={advanced}>
          <summary>Search settings</summary>
          <div class="advanced">
            <label class="field" for="vanity-backend"
              >Compute with<select
                id="vanity-backend"
                aria-label="Compute with"
                bind:value={backend}
                on:change={resetSearch}
                ><option value="auto">Automatic · GPU or CPU</option><option value="cpu"
                  >CPU only</option
                ></select
              ></label
            >
            <div class="settings-grid">
              <label class="field" for="vanity-lanes"
                >GPU lanes<input
                  id="vanity-lanes"
                  type="number"
                  min="1"
                  max={keyMode === 'raw' ? 256 : 4096}
                  bind:value={lanes}
                  on:input={resetSearch}
                /></label
              >
              {#if keyMode === 'raw'}<label class="field" for="vanity-steps"
                  >Steps per batch<input
                    id="vanity-steps"
                    type="number"
                    min="1"
                    max="16"
                    bind:value={steps}
                    on:input={resetSearch}
                  /></label
                >{/if}
            </div>
            <label class="field" for="vanity-limit"
              >Attempt limit<input
                id="vanity-limit"
                type="number"
                min="0"
                max={Number.MAX_SAFE_INTEGER}
                bind:value={maxAttempts}
                on:input={resetSearch}
              /></label
            >
            <p class="hint">0 means no limit. Shorter prefixes are faster to find.</p>
          </div>
        </details>
      </fieldset>
      {#if progress.status !== 'idle'}
        <div class="search-status" role="status" aria-live="polite" bind:this={progressElement}>
          <p>{progress.message}</p>
          {#if progress.attempts > 0}<small
              >{progress.attempts.toLocaleString()} tried · {Math.round(
                progress.rate
              ).toLocaleString()}/s{progress.backend ? ` · ${progress.backend}` : ''}</small
            >{/if}
          {#if found}<p class="address">{progress.address}</p>{/if}
        </div>
      {/if}
      <p class="hint local-note">Search runs on this device. Your keys stay here.</p>
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </div>
  <div class="button-footer">
    {#if !custom}<Button variant="primary" fullWidth={true} on:click={generate} disabled={!canStart}
        >{busy ? 'Preparing wallet…' : 'Generate Wallet'}</Button
      >
    {:else if mining}<Button variant="secondary" fullWidth={true} on:click={() => search.stop()}
        >Stop Search</Button
      >
    {:else if found}<Button
        variant="primary"
        fullWidth={true}
        on:click={() => continueWith(search.take())}
        disabled={!canStart}>{busy ? 'Preparing wallet…' : 'Use This Address'}</Button
      >
    {:else}<Button
        variant="primary"
        fullWidth={true}
        on:click={start}
        disabled={!canStart || !prefix.trim()}>Find Address</Button
      >{/if}
  </div>
</div>

<style>
  .button-footer {
    position: static;
    flex-shrink: 0;
  }
  .create-wallet {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-background);
  }
  .create-content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 24px 20px;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 24px;
    font-weight: 600;
    color: var(--color-text);
  }
  .subtitle {
    color: var(--color-text-secondary);
    font-size: 14px;
    margin: 0 0 24px;
  }
  .field {
    display: grid;
    gap: 8px;
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text);
  }
  input:not([type='checkbox']),
  select {
    width: 100%;
    min-width: 0;
    padding: 11px 12px;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    background: var(--color-surface);
    color: var(--color-text);
    font: inherit;
  }
  input[aria-invalid='true'] {
    border-color: var(--color-error);
  }
  .mono {
    font-family: monospace !important;
    font-size: 17px !important;
  }
  input[type='checkbox'] {
    accent-color: var(--color-primary);
    width: 17px;
    height: 17px;
    flex-shrink: 0;
  }
  .custom-choice {
    border-block: 1px solid var(--color-border);
    margin: 22px 0;
    padding: 16px 0;
  }
  .switch-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    cursor: pointer;
  }
  strong {
    font-size: 14px;
    font-weight: 500;
  }
  small {
    display: block;
    font-size: 12px;
    color: var(--color-text-secondary);
    line-height: 1.5;
  }
  .mining-options {
    display: grid;
    gap: 16px;
    min-width: 0;
    border: 0;
    padding: 0;
    margin: 0;
  }
  .check-row {
    display: flex;
    align-items: start;
    gap: 9px;
    font-size: 12px;
    color: var(--color-text-secondary);
    line-height: 1.5;
  }
  .hint {
    font-size: 12px;
    line-height: 1.6;
    color: var(--color-text-secondary);
    margin: -8px 0 0;
  }
  details {
    border-top: 1px solid var(--color-border);
    padding-top: 14px;
  }
  summary {
    cursor: pointer;
    font-size: 13px;
    color: var(--color-text-secondary);
  }
  .advanced {
    display: grid;
    gap: 16px;
    padding-top: 16px;
  }
  .settings-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(110px, 1fr));
    gap: 12px;
  }
  .search-status {
    background: var(--color-surface);
    border-radius: 10px;
    padding: 12px;
    margin-top: 18px;
    font-size: 13px;
  }
  .search-status p {
    margin: 0 0 6px;
  }
  .address {
    font-family: monospace;
    overflow-wrap: anywhere;
    padding-top: 8px;
  }
  .local-note {
    margin-top: 18px;
  }
  .error {
    color: var(--color-error);
    font-size: 13px;
    line-height: 1.5;
  }
  input:disabled,
  select:disabled {
    opacity: 0.55;
  }
  :focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: 3px;
  }
</style>
