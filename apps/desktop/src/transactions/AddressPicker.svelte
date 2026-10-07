<script lang="ts">
  export let id: number;
  export let outputNumber: number;
  export let value = '';
  export let choices: { name: string; address: string; kind: string }[] = [];
  export let loading = false;
  export let error = '';
  export let onOpen: () => void;
  export let onRetry: () => void;
  let open = false;
  let query = '';
  $: filtered = choices.filter(choice =>
    `${choice.name} ${choice.address}`.toLowerCase().includes(query.toLowerCase())
  );
  function select(address: string) {
    value = address;
    open = false;
    document.getElementById(`output-address-${id}`)?.focus();
  }
</script>

<div class="recipient-picker">
  <div class="recipient-label">
    <label for="output-address-{id}">Recipient address</label>
    <button
      type="button"
      class="subtle"
      aria-label="Address book for output {outputNumber}"
      aria-expanded={open}
      aria-controls="output-contacts-{id}"
      on:click={() => {
        open = !open;
        if (open) onOpen();
      }}>Address book</button
    >
  </div>
  <input id="output-address-{id}" bind:value placeholder="Paste an address or choose a contact" />
  {#if open}
    <section class="contact-picker" id="output-contacts-{id}" aria-label="Choose a recipient">
      <input
        type="search"
        aria-label="Search contacts"
        placeholder="Search contacts or wallets"
        bind:value={query}
      />
      {#if loading}<p class="field-hint" role="status">Loading contacts…</p>{/if}
      {#if error}<p class="contact-error">
          {error} <button type="button" class="subtle" on:click={onRetry}>Retry</button>
        </p>{/if}
      <div class="contact-choices">
        {#each filtered as choice (choice.address)}
          <button type="button" class="contact-choice" on:click={() => select(choice.address)}>
            <span><strong>{choice.name}</strong><small>{choice.kind}</small></span>
            <span class="mono">{choice.address.slice(0, 9)}…{choice.address.slice(-7)}</span>
          </button>
        {:else}{#if !loading}<p class="field-hint">
              {query
                ? 'No matching contacts.'
                : 'No saved contacts yet. You can paste an address above.'}
            </p>{/if}{/each}
      </div>
      <button type="button" class="subtle" on:click={() => (open = false)}
        >Close address book</button
      >
    </section>
  {/if}
</div>
