<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { get } from 'svelte/store';
  import { activeWallet, walletStore } from '../../../../packages/wallet/src/lib/stores/wallet';
  import { nockPrice } from '../../../../packages/wallet/src/lib/stores/price';
  import { getRPCClientV1 } from '../../../../packages/wallet/src/lib/utils/rpc';
  import { collectReservedNoteIds } from '../../../../packages/wallet/src/lib/utils/noteIds';
  import { getAddressBook } from '../../../../packages/wallet/src/lib/utils/addressBook';
  import { loadFromStorage } from '../../../../packages/wallet/src/lib/utils/storage';
  import AddressPicker from './AddressPicker.svelte';
  import TransactionPreview from './TransactionPreview.svelte';
  import NoteList from './NoteList.svelte';
  import { largestNotesFirst, notesForAmount } from './note-selection';
  import { createComposerWorker } from './composer-worker';
  import { formatUsdEstimate } from '../../../../packages/wallet/src/lib/utils/usd';
  import { hardwareCrypto, download } from '../hardware/crypto';
  import LockFields from './LockFields.svelte';
  import TransactionReview from './TransactionReview.svelte';
  import { bytesToBase64 } from './files';
  import {
    address,
    amountInNicks,
    formatComposerAmount,
    htlcBranches,
    multisig,
    newOutput,
    noteId,
    outputFromForm,
    validateNotes,
    wholeNumber,
    type ComposeInput,
    type ComposeSummary,
    type Note,
    type SourceForm,
    type Units
  } from './composer';

  let source: SourceForm = {
    kind: 'wallet',
    walletId: get(activeWallet)?.id || '',
    address: '',
    threshold: '2',
    signers: '',
    claimAddress: '',
    commitments: '',
    refundAddress: '',
    refundHeight: '',
    branch: 'claim'
  };
  let outputs = [newOutput(1)];
  let nextOutputId = 2;
  let units: Units = 'NOCK';
  let contacts: { name: string; address: string; kind: string }[] = [];
  let contactsLoading = false;
  let contactsLoaded = false;
  let contactsError = '';
  $: addressChoices = [
    ...new Map(
      [
        ...$walletStore.wallets.flatMap(wallet =>
          wallet.addresses.map(address => ({ name: wallet.name, address, kind: 'My wallet' }))
        ),
        ...contacts
      ].map(choice => [choice.address, choice])
    ).values()
  ];

  async function loadContacts(retry = false) {
    if (contactsLoading || (contactsLoaded && !retry)) return;
    contactsLoading = true;
    contactsError = '';
    const [local, remote] = await Promise.allSettled([
      loadFromStorage('addressBook'),
      getAddressBook()
    ]);
    if (!alive) return;
    const cached = local.status === 'fulfilled' && Array.isArray(local.value) ? local.value : [];
    const online = remote.status === 'fulfilled' ? remote.value : [];
    contacts = [
      ...cached
        .filter(contact => typeof contact.name === 'string' && typeof contact.address === 'string')
        .map(contact => ({ name: contact.name, address: contact.address, kind: 'Contact' })),
      ...online.map(contact => ({ name: contact.alias, address: contact.address, kind: 'Contact' }))
    ];
    contactsError =
      remote.status === 'rejected'
        ? 'Online contacts are unavailable. Saved contacts and wallets are shown.'
        : '';
    contactsLoading = false;
    contactsLoaded = true;
  }

  let notes: Note[] = [];
  let selected = new Set<string>();
  let selectionMode: 'automatic' | 'manual' = 'automatic';
  let selecting = false;
  let selectionError = '';
  let selectionHint = '';
  let selectionGeneration = 0;
  let selectionTimer: ReturnType<typeof setTimeout> | undefined;
  const composerWorker = createComposerWorker();
  let height = '';
  let manual = { first: '', last: '', origin: '', amount: '' };
  let notesLoading = false;
  let building = false;
  let error = '';
  let noteError = '';
  let ready = false;
  let alive = true;
  let noteRequest = 0;
  let sourceKey = '';
  let draft: {
    key: string;
    bytes: Uint8Array;
    summary: ComposeSummary;
    sourceAddress: string;
    txId: string;
  } | null = null;
  let reviewElement: HTMLDivElement;

  $: sourceWallet = $walletStore.wallets.find(wallet => wallet.id === source.walletId);
  $: sourceAddress =
    source.kind === 'wallet' ? sourceWallet?.addresses[0] || '' : source.address.trim();
  $: reserved = collectReservedNoteIds(
    $walletStore.wallets.flatMap(wallet => wallet.pendingTransactions || [])
  );
  $: eligibleNotes = notes.filter(note => !reserved.has(noteId(note)));
  $: selectedNotes = notes.filter(
    note => selected.has(noteId(note)) && !reserved.has(noteId(note))
  );
  $: selectedTotal = selectedNotes.reduce((sum, note) => sum + BigInt(note.assets), 0n);
  $: normalizedOutputs = outputs.map(output => {
    try {
      return { ...output, amount: amountInNicks(output.amount, units) };
    } catch {
      return { ...output, amount: `${units}:${output.amount}` };
    }
  });
  $: formKey = JSON.stringify({
    source,
    sourceAddress,
    outputs: normalizedOutputs,
    notes: selectedNotes,
    height
  });
  $: autoSelectionKey = JSON.stringify({
    source,
    sourceAddress,
    outputs: normalizedOutputs,
    notes: eligibleNotes,
    height
  });
  $: currentDraft = draft?.key === formKey ? draft : null;
  $: outputTotal = outputs.reduce((sum, output) => {
    try {
      return sum + BigInt(amountInNicks(output.amount, units));
    } catch {
      return sum;
    }
  }, 0n);
  $: if (ready) scheduleSelection(autoSelectionKey, selectionMode, outputTotal, notesLoading);
  $: refundTotal =
    currentDraft?.summary.spends.reduce((sum, spend) => sum + BigInt(spend.refund), 0n) ?? null;
  $: {
    const key = JSON.stringify([
      source.kind,
      source.walletId,
      sourceAddress,
      source.signers,
      source.threshold,
      source.claimAddress,
      source.commitments,
      source.refundAddress,
      source.refundHeight,
      source.branch
    ]);
    if (key !== sourceKey) {
      sourceKey = key;
      noteRequest++;
      notes = [];
      selected = new Set();
      selectionMode = 'automatic';
      noteError = '';
      notesLoading = false;
      if (ready && source.kind === 'wallet') void syncNotes();
    }
  }

  $: money = (nicks: bigint | number) => formatComposerAmount(nicks, units);
  function usd(nicks: bigint | number | null) {
    return nicks === null ? '' : formatUsdEstimate(Number(nicks) / 65536, $nockPrice) || '';
  }

  async function syncNotes() {
    const request = ++noteRequest;
    notesLoading = true;
    noteError = '';
    try {
      const wasm = await hardwareCrypto();
      let lookup = sourceAddress;
      if (source.kind === 'multisig')
        lookup = wasm.compose_tx_v1_recipient_address(
          multisig(source.threshold, source.signers, wasm.is_valid_pkh)
        );
      if (source.kind === 'htlc')
        throw new Error(
          'Add the HTLC note below using its two name hashes, block height, and amount.'
        );
      address(lookup, 'source address', wasm.is_valid_pkh);
      const rpc = getRPCClientV1();
      const [loaded, tip] = await Promise.all([rpc.getNotesByAddress(lookup), rpc.getTipHeight()]);
      if (!alive || request !== noteRequest) return;
      notes = largestNotesFirst(
        loaded
          .filter(note => Number(note.version) === 1)
          .map(note => ({
            name_first: note.firstName,
            name_last: note.lastName,
            origin_page: note.originPage,
            assets: note.assets,
            version: 1
          }))
      );
      height = String(tip);
    } catch (failure) {
      if (alive && request === noteRequest)
        noteError = failure instanceof Error ? failure.message : String(failure);
    } finally {
      if (alive && request === noteRequest) notesLoading = false;
    }
  }

  async function addNote() {
    noteError = '';
    try {
      const wasm = await hardwareCrypto();
      const note: Note = {
        name_first: manual.first.trim(),
        name_last: manual.last.trim(),
        origin_page: wholeNumber(manual.origin, 'Origin block height'),
        assets: amountInNicks(manual.amount, units),
        version: 1
      };
      validateNotes([note], wasm.is_valid_pkh);
      if (notes.some(existing => noteId(existing) === noteId(note)))
        throw new Error('Each input note can only be selected once.');
      if (reserved.has(noteId(note)))
        throw new Error('This note is reserved by a pending transaction.');
      notes = largestNotesFirst([...notes, note]);
      if (selectionMode === 'manual') selected = new Set([...selected, noteId(note)]);
      manual = { first: '', last: '', origin: '', amount: '' };
    } catch (failure) {
      noteError = failure instanceof Error ? failure.message : String(failure);
    }
  }

  function selectNote(note: Note, checked: boolean) {
    selectionMode = 'manual';
    const next = new Set(selected);
    if (checked) next.add(noteId(note));
    else next.delete(noteId(note));
    selected = next;
  }

  function changeUnits(event: Event) {
    const next = (event.currentTarget as HTMLSelectElement).value as Units;
    const convert = (value: string) => {
      if (!value.trim()) return value;
      try {
        const nicks = BigInt(amountInNicks(value, units));
        if (next === 'nicks') return nicks.toString();
        const fraction = (((nicks % 65536n) * 10n ** 16n) / 65536n)
          .toString()
          .padStart(16, '0')
          .replace(/0+$/, '');
        return `${nicks / 65536n}${fraction ? '.' + fraction : ''}`;
      } catch {
        return value;
      }
    };
    outputs = outputs.map(output => ({ ...output, amount: convert(output.amount) }));
    manual = { ...manual, amount: convert(manual.amount) };
    units = next;
  }

  function composeInput(
    wasm: Awaited<ReturnType<typeof hardwareCrypto>>,
    inputNotes: Note[]
  ): ComposeInput {
    const valid = wasm.is_valid_pkh;
    const input: ComposeInput = {
      source_pkh: sourceAddress,
      notes: inputNotes,
      outputs: [],
      current_height: wholeNumber(height, 'Current block height', 1)
    };
    if (source.kind === 'multisig') {
      input.source_multisig = multisig(source.threshold, source.signers, valid);
      input.source_pkh = wasm.compose_tx_v1_recipient_address(input.source_multisig);
    } else {
      address(input.source_pkh, 'source / change address', valid);
    }
    if (source.kind === 'htlc')
      input.source_or_lock = {
        branches: htlcBranches(
          address(source.claimAddress, 'claim address', valid),
          source.commitments,
          source.refundAddress,
          source.refundHeight,
          valid
        ),
        spend_branch: source.branch === 'claim' ? 0 : 1
      };
    input.outputs = outputs.map((output, index) => {
      try {
        return outputFromForm(
          output.condition === 'burn'
            ? { ...output, recipientKind: 'address', address: input.source_pkh }
            : output,
          units,
          valid
        );
      } catch (failure) {
        throw new Error(
          `Output ${index + 1}: ${failure instanceof Error ? failure.message : String(failure)}`
        );
      }
    });
    if (!input.outputs.length) throw new Error('Add at least one output.');
    if (!inputNotes.length) throw new Error('Select at least one input note.');
    return input;
  }

  function scheduleSelection(
    _key: string,
    mode: typeof selectionMode,
    total: bigint,
    loading: boolean
  ) {
    clearTimeout(selectionTimer);
    composerWorker.cancel();
    const request = ++selectionGeneration;
    selecting = false;
    selectionError = '';
    selectionHint = '';
    if (mode !== 'automatic') return;
    selected = new Set(notesForAmount(eligibleNotes, total).map(noteId));
    if (total <= 0n || loading) return;
    selecting = true;
    selectionTimer = setTimeout(async () => {
      let input: ComposeInput;
      try {
        const wasm = await hardwareCrypto();
        if (!alive || request !== selectionGeneration) return;
        input = composeInput(wasm, eligibleNotes);
      } catch {
        if (alive && request === selectionGeneration) {
          selecting = false;
          selectionHint = eligibleNotes.length
            ? 'Enter recipients and lock details to include the network fee.'
            : 'No spendable notes are available.';
        }
        return;
      }
      try {
        const result = await composerWorker.run(input);
        if (!alive || request !== selectionGeneration) return;
        selected = new Set(
          result.summary.inputs_used.map(note => `${note.name_first}/${note.name_last}`)
        );
      } catch (failure) {
        if (alive && request === selectionGeneration)
          selectionError = failure instanceof Error ? failure.message : String(failure);
      } finally {
        if (alive && request === selectionGeneration) selecting = false;
      }
    }, 250);
  }

  async function buildDraft() {
    if (building || selecting) return;
    building = true;
    error = '';
    draft = null;
    const key = formKey;
    try {
      const wasm = await hardwareCrypto();
      const input = composeInput(wasm, selectedNotes);
      const result = await composerWorker.run(input);
      if (!alive || key !== formKey) return;
      // Reflect only the notes included by the transaction engine.
      const used = new Set(
        result.summary.inputs_used.map(note => `${note.name_first}/${note.name_last}`)
      );
      selected = used;
      const usedNotes = notes.filter(note => used.has(noteId(note)) && !reserved.has(noteId(note)));
      draft = {
        ...result,
        key: JSON.stringify({
          source,
          sourceAddress,
          outputs: normalizedOutputs,
          notes: usedNotes,
          height
        }),
        sourceAddress: input.source_pkh
      };
    } catch (failure) {
      if (alive) error = failure instanceof Error ? failure.message : String(failure);
    } finally {
      if (alive) building = false;
    }
  }

  onMount(() => {
    ready = true;
    if (source.kind === 'wallet') void syncNotes();
  });
  onDestroy(() => {
    alive = false;
    noteRequest++;
    selectionGeneration++;
    clearTimeout(selectionTimer);
    composerWorker.cancel();
  });
</script>

<div class="transaction-page composer">
  <p class="intro">Enter your outputs to choose inputs automatically, or select notes yourself.</p>
  <fieldset disabled={building} class="composer-form">
    <section class="transaction-section">
      <div class="section-heading">
        <h2><span class="step">1</span>Inputs</h2>
        <button type="button" disabled={notesLoading || source.kind === 'htlc'} on:click={syncNotes}
          >{notesLoading ? 'Refreshing…' : 'Refresh notes'}</button
        >
      </div>
      <div class="field-pair">
        <label for="composer-source-type"
          >Spend from<select id="composer-source-type" bind:value={source.kind}>
            <option value="wallet">My wallet</option><option value="address">Another address</option
            ><option value="multisig">Multisig</option><option value="htlc"
              >HTLC · claim or refund</option
            >
          </select></label
        >
        <label for="composer-units"
          >Amount units<select id="composer-units" value={units} on:change={changeUnits}
            ><option>NOCK</option><option>nicks</option></select
          ></label
        >
      </div>
      {#if source.kind === 'wallet'}
        <label for="composer-wallet"
          >Source wallet<select id="composer-wallet" bind:value={source.walletId}>
            {#each $walletStore.wallets as wallet}<option value={wallet.id}
                >{wallet.name}{wallet.hardware
                  ? ' · Nockster'
                  : wallet.watchOnly
                    ? ' · Watch only'
                    : ''}</option
              >{/each}
          </select></label
        >
        <p class="mono source-address">{sourceAddress}</p>
      {:else if source.kind === 'multisig'}
        <label for="source-signers"
          >Source signer addresses<textarea
            id="source-signers"
            rows="3"
            bind:value={source.signers}
            placeholder="One address per line"
          ></textarea></label
        >
        <label for="source-threshold"
          >Source required signatures<input
            id="source-threshold"
            inputmode="numeric"
            bind:value={source.threshold}
          /></label
        >
      {:else}
        <label for="source-address"
          >{source.kind === 'htlc' ? 'Change address' : 'Source address'}<input
            id="source-address"
            bind:value={source.address}
            placeholder="Nockchain address"
          /></label
        >
      {/if}
      {#if source.kind === 'htlc'}
        <label for="htlc-branch"
          >Spend path<select id="htlc-branch" bind:value={source.branch}
            ><option value="claim">Claim with preimage</option><option value="refund"
              >Refund after timeout</option
            ></select
          ></label
        >
        <label for="htlc-claim"
          >Claim address<input id="htlc-claim" bind:value={source.claimAddress} /></label
        >
        <label for="htlc-commitments"
          >Claim preimage commitments<textarea
            id="htlc-commitments"
            rows="2"
            bind:value={source.commitments}
          ></textarea></label
        >
        <div class="field-pair">
          <label for="htlc-refund"
            >Refund address<input id="htlc-refund" bind:value={source.refundAddress} /></label
          >
          <label for="htlc-height"
            >Refund block height<input
              id="htlc-height"
              inputmode="numeric"
              bind:value={source.refundHeight}
            /></label
          >
        </div>
        <p class="field-hint">
          Use the lock parameters from the original HTLC. For a claim, save the matching preimage on
          your Nockster before signing.
        </p>
      {/if}

      <div class="note-selection-controls">
        <label for="note-selection-mode"
          >Input selection<select id="note-selection-mode" bind:value={selectionMode}
            ><option value="automatic">Automatic · fewest notes</option><option value="manual"
              >Choose manually</option
            ></select
          ></label
        >
        <div class="selection-count">
          <span
            >{selectedNotes.length.toLocaleString()} of {notes.length.toLocaleString()} selected</span
          ><button
            type="button"
            class="subtle"
            on:click={() => {
              selectionMode = 'manual';
              selected = new Set();
            }}>Clear selection</button
          >
        </div>
        <p class="selection-hint" aria-live="polite">
          {selecting
            ? 'Selecting inputs and checking the fee…'
            : selectionHint ||
              (selectionMode === 'automatic'
                ? outputTotal > 0n
                  ? 'Largest notes first. Change a checkbox to choose manually.'
                  : 'Enter an amount to select only the notes needed.'
                : 'Select the notes available for this transaction.')}
        </p>
      </div>
      {#if notes.length}
        <NoteList {notes} {selected} {reserved} {units} onSelect={selectNote} />
      {:else}<p class="empty-notes">
          {notesLoading
            ? 'Loading spendable notes…'
            : 'No notes available. Refresh from the network or add a note manually.'}
        </p>{/if}
      {#if selectionError}<p class="transaction-error" role="alert">{selectionError}</p>{/if}
      {#if noteError}<p class="transaction-error" role="alert">{noteError}</p>{/if}
      <details>
        <summary>Add an input note manually</summary>
        <label for="note-first"
          >First note hash<input id="note-first" bind:value={manual.first} /></label
        >
        <label for="note-last"
          >Last note hash<input id="note-last" bind:value={manual.last} /></label
        >
        <div class="field-pair">
          <label for="note-origin"
            >Origin block height<input
              id="note-origin"
              inputmode="numeric"
              bind:value={manual.origin}
            /></label
          >
          <label for="note-amount"
            >Note amount ({units})<input
              id="note-amount"
              inputmode={units === 'nicks' ? 'numeric' : 'decimal'}
              bind:value={manual.amount}
              placeholder={units === 'nicks' ? '0' : '0.00'}
            /></label
          >
        </div>
        <button type="button" on:click={addNote}>Add input note</button>
      </details>
      <details>
        <summary>Chain context</summary><label for="composer-height"
          >Current block height<input
            id="composer-height"
            inputmode="numeric"
            bind:value={height}
          /></label
        >
      </details>
    </section>
    <section class="transaction-section">
      <div class="section-heading">
        <h2><span class="step">2</span>Outputs</h2>
        <button type="button" on:click={() => (outputs = [...outputs, newOutput(nextOutputId++)])}
          >Add output</button
        >
      </div>
      {#each outputs as output, index (output.id)}
        <div class="output-card">
          <div class="section-heading">
            <h3>Output {index + 1}</h3>
            {#if outputs.length > 1}<button
                class="subtle"
                aria-label="Remove output {index + 1}"
                on:click={() => (outputs = outputs.filter(item => item.id !== output.id))}
                >Remove</button
              >{/if}
          </div>
          {#if output.recipientKind === 'address' && output.condition !== 'burn'}
            <AddressPicker
              id={output.id}
              outputNumber={index + 1}
              bind:value={output.address}
              choices={addressChoices}
              loading={contactsLoading}
              error={contactsError}
              onOpen={() => {
                void loadContacts();
              }}
              onRetry={() => {
                void loadContacts(true);
              }}
            />
          {/if}
          <label for="output-amount-{output.id}"
            >Amount ({units})<input
              id="output-amount-{output.id}"
              inputmode={units === 'nicks' ? 'numeric' : 'decimal'}
              bind:value={output.amount}
              placeholder={units === 'nicks' ? '0' : '0.00'}
            /><span class="usd-value"
              >{#if output.amount}{usd(
                  (() => {
                    try {
                      return amountInNicks(output.amount, units);
                    } catch {
                      return null;
                    }
                  })()
                )}{/if}</span
            ></label
          >
          <details>
            <summary>Lock conditions & privacy</summary><LockFields
              bind:value={output}
              id="output-{output.id}"
            />
          </details>
        </div>
      {/each}
    </section>
    <section class="transaction-section">
      <div class="summary" aria-label="Draft summary">
        <h2>Draft summary</h2>
        <div class="summary-row">
          <span>Selected inputs · {selectedNotes.length}</span><span class="value"
            >{money(selectedTotal)}<span class="usd-value">{usd(selectedTotal)}</span></span
          >
        </div>
        <div class="summary-row">
          <span>Outputs · {outputs.length}</span><span class="value"
            >{outputTotal ? money(outputTotal) : '—'}<span class="usd-value"
              >{outputTotal ? usd(outputTotal) : ''}</span
            ></span
          >
        </div>
        <div class="summary-row">
          <span>Network fee</span><span class="value"
            >{currentDraft ? money(currentDraft.summary.total_fees) : '—'}<span class="usd-value"
              >{usd(currentDraft?.summary.total_fees ?? null)}</span
            ></span
          >
        </div>
        <div class="summary-row">
          <span>Change returned</span><span class="value"
            >{refundTotal !== null ? money(refundTotal) : '—'}<span class="usd-value"
              >{usd(refundTotal)}</span
            ></span
          >
        </div>
        <div class="summary-row total">
          <span>Total sent + fee</span><span class="value"
            >{currentDraft
              ? money(outputTotal + BigInt(currentDraft.summary.total_fees))
              : '—'}<span class="usd-value"
              >{currentDraft
                ? usd(outputTotal + BigInt(currentDraft.summary.total_fees))
                : ''}</span
            ></span
          >
        </div>
      </div>
      {#if error}<p class="transaction-error" role="alert">{error}</p>{/if}
      <div class="actions">
        <button
          class="primary"
          disabled={building ||
            notesLoading ||
            selecting ||
            !selectedNotes.length ||
            !!selectionError}
          on:click={buildDraft}>{building ? 'Building…' : 'Build Draft'}</button
        >
        {#if currentDraft}<button
            on:click={() =>
              currentDraft && download(currentDraft.bytes, `${currentDraft.txId}.psnt`)}
            >Download Draft</button
          ><button
            on:click={() => reviewElement?.scrollIntoView({ block: 'start', behavior: 'smooth' })}
            >Review & Sign</button
          >{/if}
      </div>
    </section>
  </fieldset>
  {#if currentDraft}
    <div bind:this={reviewElement}>
      <TransactionReview
        jam={bytesToBase64(currentDraft.bytes)}
        sourceAddress={currentDraft.sourceAddress}
        embedded
        {units}
      />
    </div>
  {/if}
  <TransactionPreview
    notes={selectedNotes}
    {outputs}
    {units}
    fee={currentDraft?.summary.total_fees ?? null}
    change={refundTotal}
    sourceName={source.kind === 'wallet'
      ? sourceWallet?.name || ''
      : source.kind === 'multisig'
        ? 'Multisig'
        : source.kind === 'htlc'
          ? 'HTLC'
          : 'Input note'}
    choices={addressChoices}
  />
</div>
