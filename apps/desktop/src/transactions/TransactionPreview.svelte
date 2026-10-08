<script lang="ts">
  import {
    amountInNicks,
    formatComposerAmount,
    type Note,
    type OutputForm,
    type Units
  } from './composer';
  export let notes: Note[];
  export let outputs: OutputForm[];
  export let units: Units;
  export let fee: number | null = null;
  export let change: bigint | null = null;
  export let sourceName = '';
  export let choices: { name: string; address: string }[] = [];

  function amount(value: string, units: Units) {
    try {
      return formatComposerAmount(amountInNicks(value, units), units);
    } catch {
      return value.trim() ? 'Check amount' : '—';
    }
  }
  function destination(output: OutputForm, choices: { name: string; address: string }[]) {
    if (output.condition === 'burn') return 'Burn · cannot be recovered';
    if (output.recipientKind === 'multisig') return `${output.threshold || '—'} signature multisig`;
    return (
      choices.find(choice => choice.address === output.address)?.name ||
      (output.address
        ? `${output.address.slice(0, 8)}…${output.address.slice(-6)}`
        : 'Choose a recipient')
    );
  }
  function condition(output: OutputForm) {
    const parts = [];
    if (output.condition === 'timelock') parts.push(`Unlocks at block ${output.height || '—'}`);
    if (output.condition === 'hashlock') parts.push('Preimage required');
    if (output.condition === 'htlc')
      parts.push(`Claim or refund at block ${output.refundHeight || '—'}`);
    if (output.privateOutput) parts.push('Private lock');
    return parts.join(' · ');
  }
  const row = 86;
  $: shownNotes = notes.slice(0, 4);
  $: groupedCount = Math.max(0, notes.length - shownNotes.length);
  $: groupedTotal = notes
    .slice(shownNotes.length)
    .reduce((sum, note) => sum + BigInt(note.assets), 0n);
  $: inputRows = Math.max(1, shownNotes.length + (groupedCount ? 1 : 0));
  $: rows = Math.max(inputRows, outputs.length + 2);
  $: height = rows * row;
  $: inputOffset = (height - inputRows * row) / 2;
  $: outputOffset = (height - (outputs.length + 2) * row) / 2;
</script>

<section class="transaction-preview" aria-label="Transaction preview">
  <div class="preview-heading">
    <div>
      <h2>Transaction preview</h2>
      <p>Follow the flow from your notes to their destinations.</p>
    </div>
    <span class="status-label">{fee === null ? 'Editing' : 'Draft built'}</span>
  </div>
  <div class="preview-labels">
    <span>Inputs · {notes.length}</span><span>Outputs · {outputs.length}</span>
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard focus makes every note reachable in this scroll region.) -->
  <div class="preview-scroll" role="region" tabindex="0" aria-label="Transaction flow diagram">
    <div class="preview-canvas" style:height="{height}px">
      <svg viewBox="0 0 1000 {height}" preserveAspectRatio="none" aria-hidden="true">
        {#each Array(inputRows) as _, index}
          <path
            class:unfilled={!notes.length}
            d="M 400 {inputOffset + index * row + 36} C 465 {inputOffset +
              index * row +
              36}, 465 {height / 2}, 500 {height / 2}"
          />
        {/each}
        {#each [...outputs, null, null] as output, index}
          <path
            class:unfilled={index >= outputs.length && fee === null}
            d="M 500 {height / 2} C 535 {height / 2}, 535 {outputOffset +
              index * row +
              36}, 600 {outputOffset + index * row + 36}"
          />
        {/each}
      </svg>
      <div class="preview-hub" style:top="{height / 2}px" title="Transaction">
        <svg viewBox="0 0 24 24" aria-hidden="true"
          ><path d="M5 8h14m-4-4 4 4-4 4M19 16H5m4-4-4 4 4 4" /></svg
        >
      </div>
      {#each shownNotes as note, index}
        <div class="flow-node input-node" style:top="{inputOffset + index * row}px">
          <div>
            <strong>{sourceName || 'Input note'}</strong><span
              >{formatComposerAmount(note.assets, units)}</span
            >
          </div>
          <small class="mono"
            >{note.name_first.slice(0, 6)}… / {note.name_last.slice(0, 6)}… · Block {note.origin_page.toLocaleString()}</small
          >
        </div>
      {:else}<div class="flow-node input-node empty-node" style:top="{inputOffset}px">
          <strong>Select input notes</strong><small>Enter an amount or select notes above.</small>
        </div>{/each}
      {#if groupedCount}
        <div
          class="flow-node input-node grouped-notes"
          style:top="{inputOffset + shownNotes.length * row}px"
        >
          <div>
            <strong>{groupedCount.toLocaleString()} more notes</strong><span
              >{formatComposerAmount(groupedTotal, units)}</span
            >
          </div>
          <small>See selected notes in the input list above.</small>
        </div>
      {/if}
      {#each outputs as output, index (output.id)}
        <div
          class="flow-node output-node"
          style:top="{outputOffset + index * row}px"
          data-output-id={output.id}
        >
          <div>
            <strong>{destination(output, choices)}</strong><span
              >{amount(output.amount, units)}</span
            >
          </div>
          <small title={condition(output)}>{condition(output) || `Output ${index + 1}`}</small>
        </div>
      {/each}
      <div
        class="flow-node output-node secondary-node"
        style:top="{outputOffset + outputs.length * row}px"
      >
        <div>
          <strong>Change</strong><span
            >{change === null ? '—' : formatComposerAmount(change, units)}</span
          >
        </div>
        <small
          >{change === null
            ? 'Calculated when you build the draft'
            : 'Returned to the source'}</small
        >
      </div>
      <div
        class="flow-node output-node secondary-node"
        style:top="{outputOffset + (outputs.length + 1) * row}px"
      >
        <div>
          <strong>Network fee</strong><span
            >{fee === null ? '—' : formatComposerAmount(fee, units)}</span
          >
        </div>
        <small
          >{fee === null ? 'Calculated when you build the draft' : 'Included in this draft'}</small
        >
      </div>
    </div>
  </div>
  <slot />
</section>
