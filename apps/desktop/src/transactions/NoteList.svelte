<script lang="ts">
  import { noteId, formatComposerAmount, type Note, type Units } from './composer';
  export let notes: Note[];
  export let selected: Set<string>;
  export let reserved: Set<string>;
  export let units: Units;
  export let onSelect: (note: Note, checked: boolean) => void;
  let query = '';
  let selectedOnly = false;
  let scrollTop = 0;
  let viewport: HTMLDivElement;
  const rowHeight = 44;
  const overscan = 4;
  $: filtered = notes.filter(
    note =>
      (!selectedOnly || selected.has(noteId(note))) &&
      noteId(note).toLowerCase().includes(query.trim().toLowerCase())
  );
  $: viewportHeight = Math.min(264, Math.max(rowHeight, filtered.length * rowHeight));
  $: start = Math.max(
    0,
    Math.min(
      Math.floor(scrollTop / rowHeight) - overscan,
      filtered.length - Math.ceil(viewportHeight / rowHeight)
    )
  );
  $: end = Math.min(filtered.length, start + Math.ceil(viewportHeight / rowHeight) + overscan * 2);
  $: visible = filtered.slice(start, end);
  $: if (query !== undefined || selectedOnly !== undefined) {
    scrollTop = 0;
    if (viewport) viewport.scrollTop = 0;
  }
</script>

<div class="compact-notes">
  <div class="note-filter">
    <input
      type="search"
      aria-label="Search input notes"
      placeholder="Search note hashes"
      bind:value={query}
    />
    <label class="check-field"
      ><input type="checkbox" bind:checked={selectedOnly} />Selected only</label
    >
  </div>
  <div class="note-list-heading">
    <span>{filtered.length.toLocaleString()} notes</span><span>Largest first</span>
  </div>
  {#if filtered.length}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex (The virtual list needs keyboard scrolling.) -->
    <div
      class="note-viewport"
      role="region"
      aria-label="Available input notes"
      tabindex="0"
      bind:this={viewport}
      style:height="{viewportHeight}px"
      on:scroll={event => (scrollTop = event.currentTarget.scrollTop)}
    >
      <div
        style:padding-top="{start * rowHeight}px"
        style:padding-bottom="{(filtered.length - end) * rowHeight}px"
      >
        {#each visible as note (noteId(note))}
          <div class="note-row" style:height="{rowHeight}px">
            <label class="check-field" title={noteId(note)}>
              <input
                type="checkbox"
                aria-label="Select note {noteId(note)}"
                checked={selected.has(noteId(note)) && !reserved.has(noteId(note))}
                disabled={reserved.has(noteId(note))}
                on:change={event => onSelect(note, event.currentTarget.checked)}
              />
              <span class="note-identity"
                ><span class="mono"
                  >{note.name_first.slice(0, 6)}… / {note.name_last.slice(0, 6)}…</span
                ><small
                  >Block {note.origin_page.toLocaleString()}{reserved.has(noteId(note))
                    ? ' · Reserved'
                    : ''}</small
                ></span
              >
            </label>
            <span class="note-amount">{formatComposerAmount(note.assets, units)}</span>
          </div>
        {/each}
      </div>
    </div>
  {:else}<p class="empty-notes">
      {selectedOnly ? 'No selected notes match.' : 'No matching notes.'}
    </p>{/if}
</div>
