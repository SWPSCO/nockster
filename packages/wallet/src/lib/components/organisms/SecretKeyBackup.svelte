<script lang="ts">
  import Header from '../molecules/Header.svelte';
  import Button from '../atoms/Button.svelte';
  export let secretKey: string;
  export let address: string;
  export let onContinue: () => void;
  export let onBack: () => void;
  let saved = false;
  let copied = false;
  let error = '';
  async function copy() {
    try {
      await navigator.clipboard.writeText(secretKey);
      copied = true;
    } catch {
      error = 'Could not copy. Select and save the key below.';
    }
  }
</script>

<div class="secret-backup fixed-screen">
  <Header title="" showBack={true} showLogo={false} on:click={onBack} />
  <div class="backup-content">
    <h2>Back Up Your Secret Key</h2>
    <p>This wallet has no seed phrase. Save this key somewhere private so you can restore it.</p>
    <label for="secret-key-backup">Secret key · hex</label>
    <textarea
      id="secret-key-backup"
      value={secretKey}
      readonly
      rows="3"
      spellcheck="false"
      autocomplete="off"
    ></textarea>
    <button class="copy" on:click={copy}>{copied ? 'Copied' : 'Copy secret key'}</button>
    {#if error}<p role="alert">{error}</p>{/if}
    <p class="address"><span>Wallet address</span>{address}</p>
    <label class="saved"
      ><input type="checkbox" bind:checked={saved} /> I saved my secret key somewhere private.</label
    >
  </div>
  <div class="button-footer">
    <Button variant="primary" fullWidth={true} disabled={!saved} on:click={onContinue}
      >Continue</Button
    >
  </div>
</div>

<style>
  .button-footer {
    position: static;
    flex-shrink: 0;
  }
  .secret-backup {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-background);
  }
  .backup-content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px 24px 24px;
  }
  h2 {
    font-size: 22px;
    margin: 0 0 12px;
  }
  p {
    font-size: 14px;
    color: var(--color-text-secondary);
    line-height: 1.6;
  }
  label {
    font-size: 13px;
  }
  textarea {
    display: block;
    width: 100%;
    margin-top: 8px;
    padding: 14px;
    font: 14px/1.6 monospace;
    color: var(--color-text);
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    resize: none;
    word-break: break-all;
  }
  .copy {
    background: transparent;
    color: var(--color-text);
    border: 0;
    padding: 12px 0;
    font: inherit;
    font-size: 13px;
    text-decoration: underline;
    cursor: pointer;
  }
  .address {
    font-family: monospace;
    overflow-wrap: anywhere;
    margin: 18px 0 24px;
  }
  .address span {
    display: block;
    font: 12px var(--font-family, sans-serif);
    margin-bottom: 6px;
  }
  .saved {
    display: flex;
    gap: 10px;
    align-items: start;
    line-height: 1.5;
  }
  input {
    accent-color: var(--color-primary);
  }
</style>
