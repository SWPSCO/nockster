<script lang="ts">
  import { hardwareSession, perform } from './session';
  let expanded = false;
  let confirmation = '';
  let done = false;
  $: busy = Boolean($hardwareSession.busy);
  async function reset() {
    if (confirmation !== 'RESET') return;
    await hardwareSession.run('Confirm the factory reset on your Nockster', async current => {
      await current.reset();
      await hardwareSession.refresh(current);
    });
    confirmation = '';
    expanded = false;
    done = true;
  }
</script>

<div class="hw-reset">
  <button
    class="hw-link hw-danger"
    disabled={busy}
    onclick={() => {
      expanded = true;
      confirmation = '';
    }}>Factory reset</button
  >
  {#if done}<p class="hw-notice" role="status">
      Device reset. Set up a wallet in the Wallets tab.
    </p>{/if}
  {#if expanded}
    <form
      class="hw-form hw-confirm"
      onsubmit={event => {
        event.preventDefault();
        void perform(reset);
      }}
    >
      <h3>Erase this Nockster?</h3>
      <p>
        This removes all device wallets, the PIN, and stored secrets. Keep recovery phrases and
        backups before continuing.
      </p>
      <label for="reset-device-confirm">Type RESET to confirm</label><input
        id="reset-device-confirm"
        bind:value={confirmation}
        autocomplete="off"
        disabled={busy}
      />
      <div class="hw-actions">
        <button class="hw-button hw-danger" disabled={busy || confirmation !== 'RESET'}
          >Erase device</button
        ><button type="button" class="hw-link" disabled={busy} onclick={() => (expanded = false)}
          >Cancel</button
        >
      </div>
    </form>
  {/if}
</div>
