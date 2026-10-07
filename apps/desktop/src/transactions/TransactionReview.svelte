<script lang="ts">
  import { onDestroy } from 'svelte';
  import { activeWallet, walletStore } from '../../../../packages/wallet/src/lib/stores/wallet';
  import { nockPrice } from '../../../../packages/wallet/src/lib/stores/price';
  import { formatComposerAmount, type Units } from './composer';
  import { formatUsdEstimate } from '../../../../packages/wallet/src/lib/utils/usd';
  import { getVaultNickname } from '../../../../packages/wallet/src/lib/utils/vaultBridge';
  import {
    inspectTxJam,
    signTxJam,
    vaultStatus,
    checkPassword,
    unlockVault,
    lockVault
  } from '../../../../packages/wallet/src/vaultApi';
  import type { JamInspectResult } from '../../../../packages/wallet/src/vault/types';
  import { ensureVaultReady } from '../../../../packages/wallet/src/vault/wasm';
  import { hardwareCrypto, download } from '../hardware/crypto';
  import { hardwareSession } from '../hardware/session';
  import {
    base64ToBytes,
    bytesToBase64,
    readTransactionFile,
    transactionFileTypes,
    type TransactionTreeNode
  } from './files';
  import TransactionTree from './TransactionTree.svelte';
  import './transactions.css';

  export let jam = '';
  export let origin: string | undefined = undefined;
  export let sourceAddress = '';
  export let embedded = false;
  export let units: Units = 'NOCK';

  type Primitive = {
    kind: string;
    m?: number;
    n?: number;
    abs_min?: number;
    abs_max?: number;
    rel_min?: number;
    rel_max?: number;
  };
  type ReviewOutput = {
    recipient_b58: string;
    gift: number;
    is_refund: boolean;
    or_lock?: number;
    lock?: Primitive[];
    bridge_evm_addr?: string;
  };
  type DeviceReview = {
    outputs: ReviewOutput[];
    fee_total: number;
    external_total: number;
    refund_total: number;
    input_count: number;
  };
  type Loaded = { bytes: Uint8Array; info: JamInspectResult; tree: TransactionTreeNode | null };
  let loaded: Loaded | null = null;
  let filename = '';
  let error = '';
  let message = '';
  let busy = '';
  let walletId = '';
  let password = '';
  let mergeFile: File | null = null;
  let generation = 0;
  let seenJam: string | null = null;
  let alive = true;
  let wasm: Awaited<ReturnType<typeof hardwareCrypto>> | null = null;
  let review: DeviceReview | null = null;

  $: signingWallets = loaded
    ? $walletStore.wallets.filter(
        wallet =>
          (wallet.hardware || !wallet.watchOnly) &&
          loaded!.info.spends.some(spend =>
            spend.lock?.pkh?.pubkeyHashes.some(address => wallet.addresses.includes(address))
          )
      )
    : [];
  $: if (loaded && !signingWallets.some(wallet => wallet.id === walletId)) {
    walletId =
      signingWallets.find(wallet => wallet.id === $activeWallet?.id)?.id ||
      signingWallets[0]?.id ||
      '';
  }
  $: signer = signingWallets.find(wallet => wallet.id === walletId);
  $: signerAddress = signer?.addresses[0] || '';
  $: fullySigned =
    !!loaded?.info.spends.length && loaded.info.spends.every(spend => spend.isFullySigned);
  $: signaturesComplete =
    !!loaded?.info.spends.length &&
    loaded.info.spends.every(spend => {
      const lock = spend.lock?.pkh;
      return (
        lock &&
        lock.m > 0 &&
        spend.signedBy.filter(address => lock.pubkeyHashes.includes(address)).length >= lock.m
      );
    });
  $: canAddSignature =
    !!signer &&
    !!loaded?.info.spends.some(
      spend =>
        spend.lock?.pkh?.pubkeyHashes.includes(signerAddress) &&
        !spend.signedBy.includes(signerAddress)
    );
  $: deviceReady =
    $hardwareSession.connection === 'connected' &&
    !$hardwareSession.locked &&
    !$hardwareSession.busy;
  $: if (loaded && wasm) {
    try {
      review = wasm.review_draft(
        loaded.bytes,
        sourceAddress || signerAddress || $activeWallet?.addresses[0] || ''
      ) as DeviceReview;
    } catch {
      review = null;
    }
  } else review = null;
  $: displayedOutputs =
    review?.outputs ??
    loaded?.info.outputs.map(
      output =>
        ({
          recipient_b58:
            output.lock?.pkh?.m === 1 && output.lock.pkh.pubkeyHashes.length === 1
              ? output.lock.pkh.pubkeyHashes[0]
              : output.lockRoot,
          gift: output.gift,
          is_refund: false
        }) as ReviewOutput
    ) ??
    [];
  $: if (jam !== seenJam) {
    seenJam = jam;
    if (jam) void loadPayload(base64ToBytes(jam), 'Transaction draft');
  }

  $: money = (nicks: number) => formatComposerAmount(nicks, units);
  function usd(nicks: number) {
    return formatUsdEstimate(nicks / 65536, $nockPrice) || '';
  }
  function lockDescription(output: ReviewOutput): string {
    if (output.or_lock) return `${output.or_lock} spend paths · see lock details below`;
    const conditions = (output.lock || [])
      .map(lock => {
        if (lock.kind === 'pkh')
          return (lock.n || 0) > 1 ? `${lock.m} of ${lock.n} signatures` : 'Single signature';
        if (lock.kind === 'hax') return `${lock.n} preimage commitment${lock.n === 1 ? '' : 's'}`;
        if (lock.kind === 'burn') return 'Burn · cannot be spent';
        if (lock.kind === 'timelock')
          return [
            lock.abs_min != null ? `From block ${lock.abs_min}` : '',
            lock.abs_max != null ? `Through block ${lock.abs_max}` : '',
            lock.rel_min != null ? `After ${lock.rel_min} blocks` : '',
            lock.rel_max != null ? `Within ${lock.rel_max} blocks` : ''
          ]
            .filter(Boolean)
            .join(' · ');
        return '';
      })
      .filter(Boolean);
    if (output.bridge_evm_addr) conditions.push(`Bridge destination: ${output.bridge_evm_addr}`);
    return conditions.join(' · ') || 'Lock root only';
  }

  async function parse(bytes: Uint8Array): Promise<Loaded> {
    if (!bytes.length) throw new Error('The selected file is empty.');
    const result = await inspectTxJam(bytesToBase64(bytes));
    if (!result.success || !result.data)
      throw new Error(`This file is not a supported transaction. ${result.error || ''}`.trim());
    wasm = await hardwareCrypto();
    let tree: TransactionTreeNode | null = null;
    if (result.data.format !== 'rawTransactionV0')
      tree = wasm.inspect_tx(bytes) as TransactionTreeNode;
    return { bytes, info: result.data, tree };
  }

  async function loadPayload(bytes: Uint8Array, name: string) {
    const request = ++generation;
    loaded = null;
    review = null;
    password = '';
    mergeFile = null;
    message = '';
    error = '';
    filename = name;
    busy = 'Reading transaction…';
    try {
      const result = await parse(bytes);
      if (alive && request === generation) loaded = result;
    } catch (failure) {
      if (alive && request === generation)
        error = failure instanceof Error ? failure.message : String(failure);
    } finally {
      if (alive && request === generation) busy = '';
    }
  }

  async function importFile(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file || busy) return;
    loaded = null;
    error = '';
    busy = 'Reading transaction…';
    try {
      await loadPayload(await readTransactionFile(file), file.name);
    } catch (failure) {
      error = failure instanceof Error ? failure.message : String(failure);
      busy = '';
    } finally {
      input.value = '';
    }
  }

  async function sign() {
    if (!loaded || !signer || !canAddSignature || busy) return;
    const original = loaded;
    const selectedWallet = signer;
    const address = signerAddress;
    busy = selectedWallet.hardware
      ? 'Review and approve the transaction on your Nockster.'
      : 'Signing transaction…';
    error = '';
    message = '';
    let relock = false;
    try {
      await ensureVaultReady();
      const core = await import('../../../../packages/wallet/src/pkg/nockster_core.js');
      let bytes: Uint8Array;
      if (selectedWallet.hardware) {
        const crypto = await hardwareCrypto();
        bytes = await hardwareSession.signingProvider(address, crypto.cheetah_pkh_b58, device =>
          device.signDraft(original.bytes)
        );
      } else {
        const status = await vaultStatus();
        if (!status.success || !status.data)
          throw new Error(status.error || 'Unable to read vault state.');
        if (!password) throw new Error('Enter your wallet password to sign.');
        if (status.data.unlocked) {
          const checked = await checkPassword(password);
          if (!checked.success || !checked.data) throw new Error('Incorrect wallet password.');
        } else {
          const unlocked = await unlockVault(password);
          if (!unlocked.success) throw new Error(unlocked.error || 'Incorrect wallet password.');
          relock = true;
        }
        password = '';
        const result = await signTxJam(
          getVaultNickname(selectedWallet),
          bytesToBase64(original.bytes)
        );
        if (!result.success || !result.data)
          throw new Error(result.error || 'Unable to sign this transaction.');
        bytes = base64ToBytes(result.data.base64Tx);
      }
      core.verifyPartialSignedDraft(bytesToBase64(original.bytes), bytesToBase64(bytes));
      const signed = await parse(bytes);
      if (
        !signed.info.spends.some(
          spend =>
            spend.signedBy.includes(address) &&
            !original.info.spends.find(item => item.name === spend.name)?.signedBy.includes(address)
        )
      )
        throw new Error('No valid signature was added by the selected wallet.');
      if (alive) {
        loaded = signed;
        message = 'Signature added. Download the transaction to share or submit it.';
      }
    } catch (failure) {
      if (alive) error = failure instanceof Error ? failure.message : String(failure);
    } finally {
      password = '';
      if (relock) await lockVault();
      if (alive) busy = '';
    }
  }

  async function combineSignatures() {
    if (!loaded || !mergeFile || busy) return;
    busy = 'Combining signatures…';
    error = '';
    message = '';
    try {
      await ensureVaultReady();
      const core = await import('../../../../packages/wallet/src/pkg/nockster_core.js');
      const other = await readTransactionFile(mergeFile);
      core.verifyPartialSignedDraft(bytesToBase64(loaded.bytes), bytesToBase64(other));
      const crypto = await hardwareCrypto();
      const merged = crypto.merge_signed_tx(loaded.bytes, other);
      core.verifyPartialSignedDraft(bytesToBase64(loaded.bytes), bytesToBase64(merged));
      const result = await parse(merged);
      if (alive) {
        loaded = result;
        message = 'Signatures combined. Download the transaction to continue.';
        mergeFile = null;
      }
    } catch (failure) {
      if (alive) error = failure instanceof Error ? failure.message : String(failure);
    } finally {
      if (alive) busy = '';
    }
  }

  onDestroy(() => {
    alive = false;
    generation++;
    password = '';
  });
</script>

<div class="transaction-review fixed-screen">
  {#if !embedded}<header class="transaction-heading"><h1>Sign transaction</h1></header>{/if}
  <div class="transaction-page">
    {#if !jam}
      <div class="file-drop">
        <h2>Open a transaction</h2>
        <p>Review a draft or signed transaction from disk.</p>
        <label for="transaction-file"
          >Transaction file<input
            id="transaction-file"
            type="file"
            accept={transactionFileTypes}
            disabled={!!busy}
            on:change={importFile}
          /></label
        >
        <p class="field-hint">.jam, .tx, .signed, .draft, .noun, .psnt, or .wallet</p>
      </div>
    {/if}
    {#if origin}<p class="transaction-notice">Request from {origin}</p>{/if}
    {#if error}<p class="transaction-error" role="alert">{error}</p>{/if}
    {#if busy}<p class="transaction-notice" role="status">{busy}</p>{/if}
    {#if message}<p class="transaction-notice" role="status">{message}</p>{/if}
    {#if loaded}
      <div class="review-heading">
        <h2>Review transaction</h2>
        <p>
          {filename} · {loaded.info.spends.length} input{loaded.info.spends.length === 1 ? '' : 's'}
        </p>
      </div>
      <span class="status-label"
        >{fullySigned
          ? 'Fully signed'
          : signaturesComplete
            ? 'Signatures collected · additional spend conditions'
            : 'Signatures needed'}</span
      >
      <section class="transaction-section">
        <h3>Outputs</h3>
        {#each displayedOutputs as output, index}
          <div class="review-output">
            <div class="section-heading">
              <span>{output.is_refund ? 'Change' : `Output ${index + 1}`}</span><span
                >{money(output.gift)}<span class="usd-value">{usd(output.gift)}</span></span
              >
            </div>
            <p class="mono">{output.recipient_b58}</p>
            {#if review}<p class="lock-description">{lockDescription(output)}</p>{/if}
          </div>
        {/each}
        <div class="summary-row">
          <span>Network fee</span><span class="value"
            >{loaded.info.feePaid === null ? '—' : money(loaded.info.feePaid)}<span
              class="usd-value">{loaded.info.feePaid === null ? '' : usd(loaded.info.feePaid)}</span
            ></span
          >
        </div>
      </section>
      <section class="transaction-section">
        <div class="section-heading"><h3>Sign & export</h3></div>
        {#if signingWallets.length}
          <label for="transaction-signer"
            >Signing wallet<select id="transaction-signer" bind:value={walletId} disabled={!!busy}
              >{#each signingWallets as wallet}<option value={wallet.id}
                  >{wallet.name}{wallet.hardware ? ' · Nockster' : ''}</option
                >{/each}</select
            ></label
          >
          {#if canAddSignature && !signer?.hardware}
            <label for="transaction-password"
              >Wallet password<input
                id="transaction-password"
                type="password"
                autocomplete="current-password"
                bind:value={password}
                disabled={!!busy}
              /></label
            >
          {:else if canAddSignature && signer?.hardware && !deviceReady}
            <p class="field-hint">Connect and unlock your Nockster in Hardware to sign.</p>
          {/if}
        {:else if !fullySigned}<p class="field-hint">
            None of your wallets match this transaction’s signing addresses. You can still inspect
            and download it.
          </p>{/if}
        <div class="actions">
          {#if canAddSignature}<button
              class="primary"
              disabled={!!busy || (signer?.hardware ? !deviceReady : !password)}
              on:click={sign}>{signer?.hardware ? 'Sign on Nockster' : 'Sign Transaction'}</button
            >{/if}
          <button
            disabled={!!busy}
            on:click={() =>
              loaded &&
              download(loaded.bytes, `${loaded.info.txId}.${fullySigned ? 'tx' : 'psnt'}`)}
            >Download {fullySigned ? 'Transaction' : 'Draft'}</button
          >
        </div>
      </section>
      <details class="transaction-section">
        <summary>Inputs & signatures</summary>
        {#each loaded.info.spends as spend, index}<div class="review-output">
            <div class="section-heading">
              <span>Input {index + 1}</span><span class="status-label"
                >{spend.signedBy.length} / {spend.lock?.pkh?.m ?? 0} signatures</span
              >
            </div>
            <p class="mono">{spend.name}</p>
            {#each spend.signedBy as signer}<p class="mono lock-description">
                Signed by {signer}
              </p>{/each}
          </div>{/each}
      </details>
      {#if loaded.tree}<div class="transaction-section">
          <TransactionTree node={loaded.tree} />
        </div>{/if}
      <details class="transaction-section">
        <summary>Combine multisig signatures</summary>
        <p class="field-hint">
          Open another signed copy of this same transaction to combine its signatures.
        </p>
        <label for="signature-file"
          >Signed copy<input
            id="signature-file"
            type="file"
            accept={transactionFileTypes}
            disabled={!!busy}
            on:change={event => (mergeFile = event.currentTarget.files?.[0] || null)}
          /></label
        >
        <button disabled={!!busy || !mergeFile} on:click={combineSignatures}
          >Combine Signatures</button
        >
      </details>
      <p class="mono source-address">Transaction ID · {loaded.info.txId}</p>
    {/if}
  </div>
</div>
