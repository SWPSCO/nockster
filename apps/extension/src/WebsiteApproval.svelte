<script lang="ts">
  import { onMount } from 'svelte';
  import Button from '../../../packages/wallet/src/lib/components/atoms/Button.svelte';
  import { vaultStatus, unlockVault, getWallets, type WalletSummaryPayload } from '../../../packages/wallet/src/vaultApi';

  const id = new URLSearchParams(location.search).get('request');
  let request: { origin: string; method: string; message?: string; address?: string } | null = null;
  let wallets: WalletSummaryPayload[] = [];
  let address = '';
  let password = '';
  let locked = true;
  let exists = true;
  let busy = true;
  let error = '';

  async function loadWallets() {
    const status = await vaultStatus();
    if (!status.success || !status.data)
      throw new Error(status.error || 'Unable to open Nockster.');
    exists = status.data.exists;
    locked = !status.data.unlocked;
    if (!locked) {
      const result = await getWallets();
      if (!result.success) throw new Error(result.error);
      wallets = result.data || [];
      address = request?.address || wallets[0]?.publicKey || '';
    }
  }

  onMount(async () => {
    try {
      const response = await chrome.runtime.sendMessage({ type: 'website:details', id });
      if (!response.success) throw new Error(response.error);
      request = response.data;
      await loadWallets();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  });

  async function unlock() {
    busy = true;
    error = '';
    try {
      const result = await unlockVault(password);
      password = '';
      if (!result.success) throw new Error(result.error || 'Unable to unlock Nockster.');
      await loadWallets();
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      busy = false;
    }
  }

  async function approve(approved: boolean) {
    busy = true;
    error = '';
    try {
      const result = await chrome.runtime.sendMessage({
        type: 'website:approve',
        id,
        approved,
        address
      });
      if (!result.success) throw new Error(result.error);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : String(cause);
      await loadWallets().catch(() => {});
    } finally {
      busy = false;
    }
  }
</script>

<main>
  <header>
    <img src="./icons/icon-48.png" width="32" height="32" alt="" /><span>Nockster</span>
  </header>
  <section aria-busy={busy}>
    <h1>{request?.method === 'nock_signMessage' ? 'Sign message' : 'Connect wallet'}</h1>
    {#if request}
      <p class="origin">{request.origin}</p>
      <p class="description">
        {request.method === 'nock_signMessage'
          ? 'Review the message this site wants you to sign. Signing proves you control this wallet.'
          : wallets.length > 1
            ? 'Choose the wallet address to share with this site.'
            : 'Review the wallet address to share with this site.'}
      </p>
      {#if !exists}
        <p>Create or import a wallet in Nockster, then return to the site and try again.</p>
      {:else if locked}
        <form on:submit|preventDefault={unlock}>
          <label for="password">Unlock Nockster</label>
          <input
            id="password"
            type="password"
            autocomplete="current-password"
            bind:value={password}
            disabled={busy}
          />
          <Button type="submit" fullWidth disabled={busy || !password}
            >{busy ? 'Unlocking…' : 'Unlock'}</Button
          >
        </form>
      {:else if wallets.length === 0}
        <p>Add a software wallet in Nockster, then try again.</p>
      {:else}
        {#if wallets.length === 1 || request.method === 'nock_signMessage'}
          <div role="group" aria-label="Wallet">
            <p class="wallet-label">Wallet</p>
            <p class="wallet-name">
              {wallets.find(wallet => wallet.publicKey === address)?.nickname}
            </p>
            <p class="address">{address}</p>
          </div>
        {:else}
          <fieldset disabled={busy}>
            <legend>Wallet</legend>
            <div class="wallet-options">
              {#each wallets as wallet (wallet.publicKey)}
                <label class="wallet-option">
                  <input type="radio" name="wallet" value={wallet.publicKey} bind:group={address} />
                  <span>
                    <span class="wallet-name">{wallet.nickname}</span>
                    <span class="address">{wallet.publicKey}</span>
                  </span>
                </label>
              {/each}
            </div>
          </fieldset>
        {/if}
        {#if request.message}<pre aria-label="Message to sign">{request.message}</pre>{/if}
      {/if}
    {:else if busy}<p>Loading request…</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </section>
  <footer>
    <Button
      variant="secondary"
      fullWidth
      disabled={busy}
      on:click={() => (request ? approve(false) : window.close())}>Cancel</Button
    >
    {#if request && !locked && wallets.length > 0}
      <Button fullWidth disabled={busy || !address} on:click={() => approve(true)}>
        {busy ? 'Approving…' : request.method === 'nock_signMessage' ? 'Sign message' : 'Connect'}
      </Button>
    {/if}
  </footer>
</main>

<style>
  :global(body) {
    width: 100%;
    height: 100vh;
    min-width: 280px;
    overflow: auto;
    background: var(--color-background);
    color: var(--color-text);
  }
  main {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    padding: 24px;
    gap: 28px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 600;
  }
  section {
    flex: 1;
    min-width: 0;
  }
  h1 {
    font-size: 24px;
    line-height: 1.2;
    margin-bottom: 12px;
  }
  .origin {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .description {
    margin: 12px 0 24px;
    color: var(--color-text-secondary);
    font-size: 14px;
  }
  label,
  legend,
  .wallet-label {
    display: block;
    font-weight: 600;
    font-size: 14px;
    margin-bottom: 8px;
  }
  input[type='password'] {
    width: 100%;
    padding: 12px;
    font: inherit;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    color: var(--color-text);
  }
  form {
    display: grid;
    gap: 12px;
  }
  fieldset {
    min-width: 0;
    border: 0;
    padding: 0;
    margin: 0 0 20px;
  }
  .wallet-options {
    max-height: 240px;
    overflow-y: auto;
    overscroll-behavior: contain;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }
  .wallet-option {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px;
    margin: 0;
    cursor: pointer;
    font-weight: 400;
  }
  .wallet-option + .wallet-option {
    border-top: 1px solid var(--color-border);
  }
  .wallet-option:hover,
  .wallet-option:has(input:checked) {
    background: var(--color-surface);
  }
  .wallet-option:has(input:focus-visible) {
    outline: 2px solid var(--color-text);
    outline-offset: -3px;
  }
  .wallet-option:has(input:disabled) {
    cursor: default;
    opacity: 0.6;
  }
  .wallet-option > span {
    min-width: 0;
  }
  input[type='radio'] {
    flex-shrink: 0;
    width: 16px;
    height: 16px;
    margin-top: 3px;
    accent-color: var(--color-text);
  }
  .wallet-name {
    display: block;
    overflow-wrap: anywhere;
    font-size: 16px;
  }
  .address {
    display: block;
    overflow-wrap: anywhere;
    font-size: 13px;
    margin: 10px 0 20px;
    color: var(--color-text-secondary);
  }
  .wallet-option .address {
    margin: 6px 0 0;
  }
  pre {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font: inherit;
    font-size: 13px;
    background: var(--color-surface);
    border-radius: var(--radius-md);
    padding: 16px;
    max-height: 240px;
    overflow: auto;
  }
  .error {
    color: var(--color-error);
    margin-top: 16px;
    overflow-wrap: anywhere;
  }
  footer {
    display: flex;
    gap: 12px;
  }
  :global(button:focus-visible),
  input[type='password']:focus-visible {
    outline: 2px solid var(--color-text);
    outline-offset: 3px;
  }
</style>
