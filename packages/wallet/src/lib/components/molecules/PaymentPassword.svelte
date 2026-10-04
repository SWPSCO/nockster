<script lang="ts">
  import { onMount } from 'svelte';
  import { verifyPassword } from '../../utils/vaultBridge';
  export let onConfirm: () => Promise<void>;
  export let onCancel: () => void;
  let dialog: HTMLDialogElement;
  let input: HTMLInputElement;
  let password = '';
  let busy = false;
  let error = '';
  onMount(() => {
    dialog.showModal();
    input.focus();
    return () => {
      password = '';
      dialog.close();
    };
  });
  async function confirm(event: SubmitEvent) {
    event.preventDefault();
    if (busy || !password) return;
    busy = true;
    const credential = password;
    password = '';
    try {
      if (!(await verifyPassword(credential))) throw new Error('Incorrect wallet password');
      await onConfirm();
    } catch (failure) {
      error = failure instanceof Error ? failure.message : 'Unable to confirm payment';
    } finally {
      busy = false;
    }
  }
</script>

<dialog
  bind:this={dialog}
  oncancel={event => {
    event.preventDefault();
    if (!busy) onCancel();
  }}
>
  <form onsubmit={confirm}>
    <h2>Confirm payment</h2>
    <label for="payment-password">Wallet password</label>
    <input
      id="payment-password"
      type="password"
      autocomplete="current-password"
      bind:value={password}
      disabled={busy}
      bind:this={input}
    />
    {#if error}<p role="alert">{error}</p>{/if}
    <button type="submit" disabled={busy || !password}
      >{busy ? 'Confirming…' : 'Confirm and Send'}</button
    >
    <button type="button" onclick={onCancel} disabled={busy}>Cancel</button>
  </form>
</dialog>

<style>
  dialog {
    width: min(320px, calc(100vw - 40px));
    border: 1px solid var(--color-border, #ddd);
    border-radius: 16px;
    padding: 24px;
    color: var(--color-text, #111);
    background: var(--color-background, white);
  }
  dialog::backdrop {
    background: #0008;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  h2 {
    margin: 0 0 8px;
    font-size: 20px;
  }
  input,
  button {
    padding: 12px;
    border-radius: 8px;
    font: inherit;
  }
  input {
    border: 1px solid var(--color-border, #ccc);
    background: transparent;
    color: inherit;
  }
  button {
    border: 0;
    cursor: pointer;
  }
  button[type='submit'] {
    background: #111;
    color: white;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  p {
    color: #b42318;
    margin: 0;
  }
</style>
