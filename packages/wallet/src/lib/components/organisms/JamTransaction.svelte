<script lang="ts">
  import { onMount } from 'svelte';
  import Header from '../molecules/Header.svelte';
  import { activeWallet, walletStore } from '../../stores/wallet';
  import type { Wallet } from '../../types/wallet';
  import { getVaultNickname } from '../../utils/vaultBridge';
  import {
    formatNocksNumberWithSeparator,
    formatNocksWithSeparator,
    nicksToNocks
  } from '../../utils/nicks';
  import { noteNameToId } from '../../utils/noteIds';
  import { getRPCClient, getRPCClientV1 } from '../../utils/rpc';
  import {
    inspectTxJam,
    lockVault,
    signTxJam,
    signTxJamSelected,
    toRawTxJam,
    toWalletTxJam,
    unlockVault,
    vaultStatus,
    checkPassword
  } from '../../../vaultApi';
  import type {
    JamFormat,
    JamInspectResult,
    JamOutputSummary,
    JamSignResult,
    JamSpendSummary
  } from '../../../vault/types';

  export let jam: string = '';
  export let toSign: boolean = false;
  export let origin: string | undefined = undefined;
  export let onBack: () => void = () => {};

  let originalJam = jam;
  let jamPayload = jam;
  let initialInspectResult: JamInspectResult | null = null;
  let inspectResult: JamInspectResult | null = null;
  let signedResult: JamSignResult | null = null;
  let sendResult: { txId: string; rpcTxId: string } | null = null;
  let isInspecting = false;
  let isSigning = false;
  let isSending = false;
  let isRefreshingBalances = false;
  let error: string | null = null;
  let vaultExists: boolean | null = null;
  let vaultUnlocked: boolean | null = null;
  let hasMissingSignatures = false;
  let password = '';
  let selectedSpendNames = new Set<string>();
  let selectionWalletAddress: string | null = null;
  let signingWalletId = '';
  let signingWallet: Wallet | null = null;
  let signingWalletAddress = '';
  let ownedWalletsByAddress = new Map<string, Wallet>();
  let involvedOwnedWallets: Wallet[] = [];
  let signerWallets: Wallet[] = [];

  type InputNoteStatus = 'unknown' | 'present' | 'missing';
  let inputNoteStatus: Record<string, InputNoteStatus> = {};
  let inputNoteFoundAt: Record<string, string | null> = {};
  let inputNoteAssets: Record<string, bigint | null> = {};
  let isCheckingInputs = false;
  let inputCheckError: string | null = null;
  let inputCheckSeq = 0;
  let missingInputs: string[] = [];
  let allInputsPresent = false;
  let selectedInputsPresent = false;
  let unitMode: 'nicks' | 'nocks' = 'nocks';
  let recipientOutputs: JamOutputSummary[] = [];
  let recipientTotalNicks = 0n;
  let outputTotalNicks = 0n;
  let inputTotalNicks: bigint | null = null;
  let computedFeeNicks: bigint | null = null;
  let displayFeeNicks: bigint | null = null;
  let totalCostNicks: bigint | null = null;

  const NICKS_PER_NOCK = 65536n;

  $: {
    const map = new Map<string, Wallet>();
    for (const wallet of $walletStore.wallets ?? []) {
      for (const address of wallet.addresses ?? []) {
        map.set(address, wallet);
      }
    }
    ownedWalletsByAddress = map;
  }

  $: signerWallets = (() => {
    const spends = inspectResult?.spends ?? [];
    if (!spends.length) return [];

    const addresses = new Set<string>();
    for (const spend of spends) {
      for (const address of spend.lock?.pkh?.pubkeyHashes ?? []) {
        addresses.add(address);
      }
    }

    return ($walletStore.wallets ?? []).filter(wallet => {
      const walletAddresses = wallet.addresses ?? [];
      return !wallet.watchOnly && walletAddresses.some(address => addresses.has(address));
    });
  })();

  $: involvedOwnedWallets = (() => {
    const referenced = new Set<string>();
    for (const spend of inspectResult?.spends ?? []) {
      for (const address of spend.lock?.pkh?.pubkeyHashes ?? []) {
        referenced.add(address);
      }
    }
    for (const output of inspectResult?.outputs ?? []) {
      for (const address of output.lock?.pkh?.pubkeyHashes ?? []) {
        referenced.add(address);
      }
    }

    return ($walletStore.wallets ?? []).filter(wallet => {
      const walletAddresses = wallet.addresses ?? [];
      return walletAddresses.some(address => referenced.has(address));
    });
  })();

  $: if (inspectResult && signerWallets.length > 0) {
    const stillValid =
      signingWalletId && signerWallets.some(wallet => wallet.id === signingWalletId);
    if (!stillValid) {
      const activeId = $activeWallet?.id;
      const canUseActive = Boolean(
        activeId && signerWallets.some(wallet => wallet.id === activeId)
      );
      signingWalletId = canUseActive ? (activeId as string) : signerWallets[0].id;
    }
  }

  $: {
    const selected =
      ($walletStore.wallets ?? []).find(wallet => wallet.id === signingWalletId) ??
      $activeWallet ??
      null;
    signingWallet = selected;
    signingWalletAddress =
      selected?.addresses?.[selected.currentAddressIndex || 0] ?? selected?.addresses?.[0] ?? '';
  }

  const fileLabel = (format: JamFormat | undefined) => {
    if (!format) return 'Transaction file';
    return format === 'walletTransaction' ? 'Nockchain wallet transaction file' : 'raw-transaction';
  };

  const isFullySigned = (result: JamInspectResult | null | undefined) => {
    if (!result?.spends?.length) return false;
    return !result.spends.some(spend => spend.lock?.pkh && !spend.isFullySigned);
  };

  const fileExtension = (result: JamInspectResult | null | undefined) =>
    isFullySigned(result) ? 'tx' : 'psnt';

  const shorten = (value: string, prefix = 4, suffix = 4) => {
    if (!value) return value;
    if (value.length <= prefix + suffix + 3) return value;
    return `${value.slice(0, prefix)}...${value.slice(-suffix)}`;
  };

  const spendIsSignable = (spend: JamSpendSummary) => {
    const pkh = spend.lock?.pkh;
    if (!signingWalletAddress || !pkh?.pubkeyHashes?.length) return false;
    return pkh.pubkeyHashes.includes(signingWalletAddress);
  };

  $: if (inspectResult && signingWalletAddress && selectionWalletAddress !== signingWalletAddress) {
    selectionWalletAddress = signingWalletAddress;
    const next = new Set<string>();
    for (const spend of inspectResult.spends) {
      if (spendIsSignable(spend) && inputNoteStatus[spend.name] === 'present') {
        next.add(spend.name);
      }
    }
    selectedSpendNames = next;
  }

  $: hasMissingSignatures =
    inspectResult?.spends?.some(spend => spend.lock?.pkh && !spend.isFullySigned) ?? false;

  $: {
    const spends = inspectResult?.spends ?? [];
    const missing = spends
      .filter(spend => inputNoteStatus[spend.name] !== 'present')
      .map(spend => spend.name);
    missingInputs = missing;
    allInputsPresent = spends.length > 0 && missing.length === 0;
  }

  $: selectedInputsPresent = Array.from(selectedSpendNames).every(
    name => inputNoteStatus[name] === 'present'
  );

  const describeRecipient = (output: JamOutputSummary) => {
    const pkh = output.lock?.pkh;
    if (pkh?.pubkeyHashes?.length) {
      if (pkh.m === 1 && pkh.pubkeyHashes.length === 1) {
        return shorten(pkh.pubkeyHashes[0]);
      }
      return `${pkh.m}-of-${pkh.pubkeyHashes.length} (${shorten(pkh.pubkeyHashes[0])})`;
    }
    return shorten(output.lockRoot);
  };

  const formatGift = (giftNicks: number, unit: 'nicks' | 'nocks' = unitMode) => {
    if (unit === 'nicks') {
      return Math.trunc(giftNicks || 0).toLocaleString();
    }
    const nocks = nicksToNocks(BigInt(Math.trunc(giftNicks || 0)));
    const decimals = nocks >= 1 ? 2 : 8;
    return formatNocksNumberWithSeparator(nocks, decimals);
  };

  $: unitLabel = unitMode === 'nicks' ? 'n' : 'NOCK';

  // Auto-detect unit mode based on total output amount
  $: if (inspectResult && !sendResult) {
    const totalNicks = inspectResult.outputs.reduce((sum, o) => sum + (o.gift || 0), 0);
    const totalNocks = nicksToNocks(BigInt(Math.trunc(totalNicks)));
    if (totalNocks < 1) {
      unitMode = 'nicks';
    }
  }

  const formatWalletBalance = (wallet: Wallet) =>
    formatNocksWithSeparator(BigInt(Math.trunc(wallet.balance || 0)), 2);

  const walletNameForAddress = (address: string | null) => {
    if (!address) return null;
    return ownedWalletsByAddress.get(address)?.name ?? null;
  };

  const outputIsChange = (output: JamOutputSummary) => {
    const addresses = output.lock?.pkh?.pubkeyHashes ?? [];
    return addresses.some(address => ownedWalletsByAddress.has(address));
  };

  const sumOutputNicks = (outputs: JamOutputSummary[] | null | undefined) => {
    let total = 0n;
    for (const output of outputs ?? []) {
      total += BigInt(Math.trunc(output.gift || 0));
    }
    return total;
  };

  const formatNicksAsNocks = (nicks: bigint) => {
    const nocks = nicksToNocks(nicks);
    const decimals = nocks >= 1 ? 2 : 8;
    return formatNocksNumberWithSeparator(nocks, decimals);
  };

  $: recipientOutputs = (inspectResult?.outputs ?? []).filter(output => !outputIsChange(output));
  $: recipientTotalNicks = sumOutputNicks(recipientOutputs);
  $: outputTotalNicks = sumOutputNicks(inspectResult?.outputs);
  $: inputTotalNicks = (() => {
    const spends = inspectResult?.spends ?? [];
    if (spends.length === 0) return null;
    let total = 0n;
    for (const spend of spends) {
      const assets = inputNoteAssets[spend.name];
      if (assets === null || assets === undefined) return null;
      total += assets;
    }
    return total;
  })();
  $: computedFeeNicks = inputTotalNicks !== null ? inputTotalNicks - outputTotalNicks : null;
  $: displayFeeNicks =
    inspectResult?.feePaid != null
      ? BigInt(inspectResult.feePaid)
      : computedFeeNicks !== null && computedFeeNicks >= 0n
        ? computedFeeNicks
        : null;
  $: totalCostNicks = displayFeeNicks !== null ? recipientTotalNicks + displayFeeNicks : null;

  async function refreshVaultState() {
    const result = await vaultStatus();
    if (!result.success || !result.data) {
      throw new Error(result.error || 'Failed to determine vault status');
    }
    vaultExists = result.data.exists;
    vaultUnlocked = result.data.unlocked;
  }

  function getInvolvedWallets(result: JamInspectResult | null): Wallet[] {
    if (!result) return [];

    const referenced = new Set<string>();
    for (const spend of result.spends ?? []) {
      for (const address of spend.lock?.pkh?.pubkeyHashes ?? []) {
        referenced.add(address);
      }
    }
    for (const output of result.outputs ?? []) {
      for (const address of output.lock?.pkh?.pubkeyHashes ?? []) {
        referenced.add(address);
      }
    }

    return ($walletStore.wallets ?? []).filter(wallet => {
      const walletAddresses = wallet.addresses ?? [];
      return walletAddresses.some(address => referenced.has(address));
    });
  }

  async function refreshBalances(
    forceRefresh = false,
    result: JamInspectResult | null = inspectResult
  ) {
    const wallets = getInvolvedWallets(result);
    if (!wallets.length) return;
    isRefreshingBalances = true;
    try {
      await Promise.all(
        wallets.map(async wallet => {
          try {
            await walletStore.fetchBalance(wallet.id, forceRefresh);
          } catch {
            // Ignore balance failures for unrelated wallets.
          }
        })
      );
    } finally {
      isRefreshingBalances = false;
    }
  }

  async function refreshInputNotes(result: JamInspectResult) {
    const seq = ++inputCheckSeq;
    inputCheckError = null;
    isCheckingInputs = true;

    const initial: Record<string, InputNoteStatus> = {};
    const initialAssets: Record<string, bigint | null> = {};
    for (const spend of result.spends ?? []) {
      initial[spend.name] = 'unknown';
      initialAssets[spend.name] = null;
    }
    inputNoteStatus = initial;
    inputNoteFoundAt = {};
    inputNoteAssets = initialAssets;

    try {
      if (!result.spends.length) {
        inputCheckError = 'Input validation is unavailable for this transaction format.';
        return;
      }

      const addressSet = new Set<string>();
      for (const spend of result.spends) {
        for (const address of spend.lock?.pkh?.pubkeyHashes ?? []) {
          addressSet.add(address);
        }
      }

      const addresses = Array.from(addressSet);
      if (addresses.length === 0) {
        inputCheckError = 'Cannot validate input notes (no lock address information found).';
        return;
      }

      const rpc = getRPCClientV1();
      const noteIdsByAddress = new Map<string, Set<string>>();
      const noteAssetsById = new Map<string, bigint>();

      await Promise.all(
        addresses.map(async address => {
          const notes = await rpc.getNotesByAddress(address);
          const ids = new Set<string>();
          for (const note of notes) {
            const id = noteNameToId(note);
            if (id) {
              ids.add(id);
              const assets = (note as any)?.assets;
              if (typeof assets === 'number' && Number.isFinite(assets)) {
                noteAssetsById.set(id, BigInt(Math.trunc(assets)));
              }
            }
          }
          noteIdsByAddress.set(address, ids);
        })
      );

      if (seq !== inputCheckSeq) return;

      const statuses: Record<string, InputNoteStatus> = {};
      const foundAt: Record<string, string | null> = {};
      const assetsBySpend: Record<string, bigint | null> = {};

      for (const spend of result.spends) {
        const candidates = spend.lock?.pkh?.pubkeyHashes ?? [];
        if (candidates.length === 0) {
          statuses[spend.name] = 'unknown';
          foundAt[spend.name] = null;
          assetsBySpend[spend.name] = null;
          continue;
        }

        let foundAddress: string | null = null;
        for (const address of candidates) {
          const noteIds = noteIdsByAddress.get(address);
          if (noteIds?.has(spend.name)) {
            foundAddress = address;
            break;
          }
        }

        statuses[spend.name] = foundAddress ? 'present' : 'missing';
        foundAt[spend.name] = foundAddress;
        assetsBySpend[spend.name] = foundAddress ? (noteAssetsById.get(spend.name) ?? null) : null;
      }

      inputNoteStatus = statuses;
      inputNoteFoundAt = foundAt;
      inputNoteAssets = assetsBySpend;

      if (selectedSpendNames.size === 0 && signingWalletAddress) {
        const next = new Set<string>();
        for (const spend of result.spends) {
          if (statuses[spend.name] !== 'present') continue;
          const candidates = spend.lock?.pkh?.pubkeyHashes ?? [];
          if (candidates.includes(signingWalletAddress)) {
            next.add(spend.name);
          }
        }
        selectedSpendNames = next;
      }
    } catch (err) {
      if (seq !== inputCheckSeq) return;
      inputCheckError = err instanceof Error ? err.message : String(err);
    } finally {
      if (seq === inputCheckSeq) {
        isCheckingInputs = false;
      }
    }
  }

  async function runInspect(payload: string = jamPayload) {
    error = null;
    sendResult = null;
    inspectResult = null;

    const value = payload.trim();
    if (!value) {
      error = 'Transaction payload is missing.';
      return;
    }

    isInspecting = true;
    try {
      selectionWalletAddress = null;
      selectedSpendNames = new Set<string>();
      inputCheckError = null;
      const result = await inspectTxJam(value);
      if (!result.success || !result.data) {
        throw new Error(result.error || 'Failed to inspect jam');
      }
      inspectResult = result.data;
      if (!initialInspectResult && value === originalJam.trim()) {
        initialInspectResult = result.data;
      }

      void refreshBalances(false, result.data);
      void refreshInputNotes(result.data);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      isInspecting = false;
    }
  }

  async function handleSign() {
    if (!inspectResult) return;
    if (!signingWallet) {
      error = 'Select a wallet to sign with.';
      return;
    }
    if (signingWallet.watchOnly) {
      error = 'This wallet is watch-only and cannot sign transactions.';
      return;
    }
    if (isCheckingInputs) {
      error = 'Validating input notes…';
      return;
    }
    if (inputCheckError) {
      error = inputCheckError;
      return;
    }

    isSigning = true;
    error = null;
    try {
      const selection = Array.from(selectedSpendNames);
      if (inspectResult.spends.length > 0 && selection.length === 0) {
        throw new Error('Select at least one spend to sign.');
      }
      if (!selectedInputsPresent) {
        throw new Error('One or more selected spends reference missing input notes.');
      }

      const nickname = getVaultNickname(signingWallet);
      await refreshVaultState();
      const shouldRelock = vaultExists === true && vaultUnlocked === false;
      if (shouldRelock) {
        if (!password.trim()) {
          throw new Error('Enter your password to sign this transaction.');
        }
        const unlockResult = await unlockVault(password);
        if (!unlockResult.success) {
          throw new Error(unlockResult.error || 'Failed to unlock vault');
        }
      }

      const payload = jamPayload.trim();

      const result =
        selection.length > 0
          ? await signTxJamSelected(nickname, payload, selection)
          : await signTxJam(nickname, payload);
      if (!result.success || !result.data) {
        throw new Error(result.error || 'Signing failed');
      }
      signedResult = result.data;
      jamPayload = result.data.base64Tx;
      await runInspect(result.data.base64Tx);

      if (shouldRelock) {
        await lockVault();
        await refreshVaultState();
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      isSigning = false;
    }
  }

  function copy(value: string) {
    navigator.clipboard.writeText(value).catch(() => undefined);
  }

  function decodeBase64(base64: string): Uint8Array {
    const cleaned = base64
      .trim()
      .replace(/ /g, '+')
      .replace(/\s+/g, '')
      .replace(/-/g, '+')
      .replace(/_/g, '/');
    const remainder = cleaned.length % 4;
    if (remainder === 1) {
      throw new Error('Invalid base64 payload length.');
    }
    const padded = remainder === 0 ? cleaned : remainder === 2 ? `${cleaned}==` : `${cleaned}=`;
    const binary = atob(padded);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  }

  async function downloadPayload(payload: string, info: JamInspectResult | null) {
    error = null;
    try {
      const filenameBase = info?.txId ? info.txId : 'transaction';
      const ext = fileExtension(info);
      const trimmed = payload.trim();
      if (!trimmed) {
        throw new Error('Transaction payload is missing.');
      }

      const walletTxResult = await toWalletTxJam(trimmed);
      if (!walletTxResult.success || !walletTxResult.data) {
        throw new Error(walletTxResult.error || 'Failed to prepare wallet transaction file.');
      }

      const bytes = decodeBase64(walletTxResult.data);
      const blob = new Blob([new Uint8Array(bytes)], { type: 'application/octet-stream' });
      const url = URL.createObjectURL(blob);

      const a = document.createElement('a');
      a.href = url;
      a.download = `${filenameBase}.${ext}`;
      a.style.display = 'none';
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);

      // Give Chromium a moment to start reading the object URL before revoking it.
      window.setTimeout(() => URL.revokeObjectURL(url), 10_000);
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleSend() {
    if (!inspectResult) return;
    if (hasMissingSignatures) {
      error = 'Transaction is missing required signatures.';
      return;
    }
    if (isCheckingInputs) {
      error = 'Validating input notes…';
      return;
    }

    isSending = true;
    error = null;
    sendResult = null;
    try {
      await refreshInputNotes(inspectResult);
      const missing = inspectResult.spends.filter(
        spend => inputNoteStatus[spend.name] !== 'present'
      );
      if (missing.length > 0) {
        throw new Error(`Missing ${missing.length} input note${missing.length === 1 ? '' : 's'}.`);
      }

      // Always require password to send transactions for security
      if (!password.trim()) {
        throw new Error('Enter your password to send this transaction.');
      }

      await refreshVaultState();
      const wasLocked = vaultExists === true && vaultUnlocked === false;

      // Verify password by unlocking (if locked) or checking (if unlocked)
      if (wasLocked) {
        const unlockResult = await unlockVault(password);
        if (!unlockResult.success) {
          throw new Error(unlockResult.error || 'Incorrect password');
        }
      } else {
        // Vault is unlocked - verify password matches
        const checkResult = await checkPassword(password);
        if (!checkResult.success || checkResult.data !== true) {
          throw new Error('Incorrect password');
        }
      }

      const payload = jamPayload.trim();
      const rawResult = await toRawTxJam(payload);
      if (!rawResult.success || !rawResult.data) {
        throw new Error(rawResult.error || 'Failed to convert transaction to raw form');
      }
      const rpcTxId = await getRPCClient().submitTransaction(rawResult.data);
      sendResult = { txId: inspectResult.txId, rpcTxId };

      // Re-lock if it was locked before
      if (wasLocked) {
        await lockVault();
        await refreshVaultState();
      }
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      isSending = false;
    }
  }

  onMount(() => {
    originalJam = jam;
    jamPayload = jam;
    refreshVaultState().catch(() => undefined);
    if (jam.trim()) runInspect(jam);
  });
</script>

<div class="jam-transaction fixed-screen">
  <Header
    title={toSign ? 'Review Transaction' : 'Inspect Transaction'}
    showBack={true}
    on:click={onBack}
  />

  <div class="content">
    {#if origin}
      <div class="origin">Request from: <span class="mono">{origin}</span></div>
    {/if}

    {#if error}
      <div class="error">{error}</div>
    {/if}

    {#if isInspecting && !inspectResult}
      <div class="card">
        <div class="label">Loading</div>
        <div class="muted">Fetching transaction details…</div>
      </div>
    {/if}

    {#if inspectResult}
      {#if toSign}
        <div class="card review-card">
          <div class="review-title">Transaction details</div>

          <div class="review-step">
            <div class="review-step-label">From</div>
            <div class="review-step-body">
              <div class="value">{signingWallet?.name ?? 'Unknown wallet'}</div>
              {#if signingWalletAddress}
                <div class="muted mono">{shorten(signingWalletAddress, 10, 10)}</div>
              {/if}
              {#if signingWallet}
                <div class="muted">Balance: {formatWalletBalance(signingWallet)} NOCK</div>
              {/if}

              {#if signerWallets.length > 1}
                <div class="review-inline-control">
                  <div class="muted">Signing wallet</div>
                  <select class="select" bind:value={signingWalletId}>
                    {#each signerWallets as wallet (wallet.id)}
                      <option value={wallet.id}>
                        {wallet.name} · {formatWalletBalance(wallet)} NOCK
                      </option>
                    {/each}
                  </select>
                </div>
              {/if}
            </div>
          </div>

          <div class="review-step">
            <div class="review-step-label">To</div>
            <div class="review-step-body">
              {#if recipientOutputs.length === 0}
                <div class="muted">No external recipients (change-only transaction).</div>
              {:else}
                <div class="recipients">
                  {#each recipientOutputs as output, index (index)}
                    <div class="recipient-row">
                      <div class="recipient-address mono">{describeRecipient(output)}</div>
                      <div class="recipient-amount">{formatGift(output.gift, 'nocks')} NOCK</div>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          </div>

          <div class="review-divider"></div>

          <div class="review-row">
            <div class="review-row-label">Amount</div>
            <div class="review-row-value">{formatNicksAsNocks(recipientTotalNicks)} NOCK</div>
          </div>
          <div class="review-row">
            <div class="review-row-label">Network fee</div>
            <div class="review-row-value">
              {#if displayFeeNicks !== null}
                {formatNicksAsNocks(displayFeeNicks)} NOCK
              {:else if isCheckingInputs}
                Calculating…
              {:else}
                —
              {/if}
            </div>
          </div>
          <div class="review-row total">
            <div class="review-row-label">Total cost</div>
            <div class="review-row-value total">
              {#if totalCostNicks !== null}
                {formatNicksAsNocks(totalCostNicks)} NOCK
              {:else if isCheckingInputs}
                Calculating…
              {:else}
                —
              {/if}
            </div>
          </div>
        </div>

        <div class="card action-card">
          <div class="label">Sign & Send</div>
          <div class="muted">Sign first, then send once the transaction is fully signed.</div>

          {#if vaultExists === false}
            <div class="muted">No vault found. Create or import a wallet to sign.</div>
          {:else}
            <div class="password">
              <div class="label">
                Password {#if vaultUnlocked}<span class="muted">(required to send)</span>{/if}
              </div>
              <input
                class="input password-input"
                type="password"
                bind:value={password}
                placeholder={vaultUnlocked
                  ? 'Confirm password to send'
                  : 'Enter password to sign/send'}
                autocomplete="current-password"
              />
            </div>
          {/if}

          <div class="actions signing-actions">
            <button
              class="btn-secondary"
              type="button"
              disabled={!inspectResult ||
                isSigning ||
                !signingWallet ||
                (signingWallet?.watchOnly ?? false) ||
                isCheckingInputs ||
                Boolean(inputCheckError) ||
                (inspectResult?.spends?.length ? selectedSpendNames.size === 0 : false) ||
                !selectedInputsPresent ||
                (vaultExists === true && vaultUnlocked === false && !password.trim())}
              on:click={handleSign}
            >
              {isSigning ? 'Signing…' : 'Sign'}
            </button>
            <button
              class="btn-primary"
              type="button"
              disabled={!inspectResult ||
                isSending ||
                hasMissingSignatures ||
                isCheckingInputs ||
                Boolean(inputCheckError) ||
                !allInputsPresent ||
                !password.trim()}
              on:click={handleSend}
            >
              {isSending ? 'Sending…' : 'Send'}
            </button>
          </div>

          {#if inputCheckError}
            <div class="error-inline">{inputCheckError}</div>
          {/if}

          {#if hasMissingSignatures && inspectResult}
            <div class="muted">Needs more signatures before it can be sent.</div>
          {/if}

          {#if inspectResult?.spends?.length && !isCheckingInputs && missingInputs.length > 0}
            <div class="muted">Cannot sign or send while inputs are missing.</div>
          {/if}

          {#if inspectResult?.txId && isFullySigned(inspectResult)}
            <div class="signed">
              <div class="label">Transaction ID</div>
              <div class="tx-actions">
                <div class="value mono">{shorten(inspectResult.txId, 10, 10)}</div>
                <button class="btn-small" type="button" on:click={() => inspectResult && copy(inspectResult.txId)}>
                  Copy
                </button>
              </div>
            </div>
          {/if}

          {#if signedResult}
            <div class="signed">
              <div class="label">Last signature</div>
              <div class="muted">Spends signed: {signedResult.spendsSigned}</div>
            </div>
          {/if}

          {#if sendResult}
            <div class="signed">
              <div class="label">Submitted</div>
              <div class="value mono">{shorten(sendResult.txId || sendResult.rpcTxId, 10, 10)}</div>
              <button
                class="btn-small"
                type="button"
                on:click={() => sendResult && copy(sendResult.txId || sendResult.rpcTxId)}
              >
                Copy Tx ID
              </button>
            </div>
          {/if}
        </div>

        <details class="card advanced-section">
          <summary>Advanced</summary>
          <div class="advanced-content">
            <div class="advanced-block">
              <div class="label">Files</div>
              <div class="row file-row">
                <div class="value">{fileLabel(inspectResult?.format)}</div>
                <div class="file-actions">
                  <button
                    class="btn-secondary"
                    type="button"
                    disabled={!jamPayload.trim()}
                    on:click={() => downloadPayload(jamPayload, inspectResult)}
                  >
                    Download file
                  </button>
                  {#if signedResult && originalJam.trim() && originalJam.trim() !== jamPayload.trim()}
                    <button
                      class="btn-secondary"
                      type="button"
                      on:click={() => downloadPayload(originalJam, initialInspectResult)}
                    >
                      Download original
                    </button>
                  {/if}
                </div>
              </div>
            </div>

            <div class="advanced-block">
              <div class="outputs-header">
                <div class="label">Outputs</div>
                <div class="unit-toggle">
                  <button
                    class="unit-btn"
                    class:active={unitMode === 'nocks'}
                    on:click={() => (unitMode = 'nocks')}
                  >
                    nocks
                  </button>
                  <button
                    class="unit-btn"
                    class:active={unitMode === 'nicks'}
                    on:click={() => (unitMode = 'nicks')}
                  >
                    nicks
                  </button>
                </div>
              </div>
              <div class="outputs">
                {#if inspectResult.outputs.length === 0}
                  <div class="muted">No output details available.</div>
                {:else}
                  {#each inspectResult.outputs as output}
                    {@const outputAddresses = output.lock?.pkh?.pubkeyHashes ?? []}
                    {@const isRefund = outputAddresses.some(addr =>
                      ownedWalletsByAddress.has(addr)
                    )}
                    {@const refundWalletName = outputAddresses
                      .map(addr => ownedWalletsByAddress.get(addr)?.name)
                      .find(Boolean)}
                    <div class="output" class:refund={isRefund}>
                      <div class="output-main">
                        <div class="output-recipient">
                          <div class="value mono">{describeRecipient(output)}</div>
                          {#if isRefund && refundWalletName}
                            <div class="output-label change">↩ Change to {refundWalletName}</div>
                          {:else}
                            <div class="output-label recipient">Recipient</div>
                          {/if}
                        </div>
                        <div class="amount">{formatGift(output.gift, unitMode)} {unitLabel}</div>
                      </div>
                    </div>
                  {/each}
                {/if}
              </div>
            </div>

            {#if involvedOwnedWallets.length > 0}
              <div class="advanced-block">
                <div class="section-header">
                  <div>
                    <div class="label">Wallets</div>
                    <div class="muted">Balances for wallets referenced by this transaction</div>
                  </div>
                  <button
                    class="btn-small"
                    type="button"
                    disabled={isRefreshingBalances}
                    on:click={() => refreshBalances(true)}
                  >
                    {isRefreshingBalances ? 'Refreshing…' : 'Refresh'}
                  </button>
                </div>

                <div class="wallets">
                  {#each involvedOwnedWallets as wallet (wallet.id)}
                    {@const walletAddress =
                      wallet.addresses?.[wallet.currentAddressIndex || 0] ??
                      wallet.addresses?.[0] ??
                      ''}
                    {@const inputCount = (inspectResult.spends ?? []).filter(spend =>
                      spend.lock?.pkh?.pubkeyHashes?.includes(walletAddress)
                    ).length}
                    {@const missingCount = (inspectResult.spends ?? []).filter(
                      spend =>
                        spend.lock?.pkh?.pubkeyHashes?.includes(walletAddress) &&
                        inputNoteStatus[spend.name] !== 'present'
                    ).length}

                    <div class="wallet-row">
                      <div class="wallet-left">
                        <div class="value">{wallet.name}</div>
                        <div class="muted mono">{shorten(walletAddress, 10, 10)}</div>
                        {#if inputCount > 0}
                          <div class="muted">
                            Inputs: {inputCount}
                            {#if missingCount > 0}
                              · missing {missingCount}
                            {/if}
                          </div>
                        {/if}
                      </div>
                      <div class="wallet-right">
                        <div class="amount">{formatWalletBalance(wallet)} NOCK</div>
                      </div>
                    </div>
                  {/each}
                </div>
              </div>
            {/if}

            <div class="advanced-block">
              <div class="label">Signing details</div>

              <div class="validation">
                <div class="validation-header">
                  <div class="label">Inputs</div>
                  <button
                    class="btn-small"
                    type="button"
                    disabled={isCheckingInputs}
                    on:click={() => inspectResult && refreshInputNotes(inspectResult)}
                  >
                    {isCheckingInputs ? 'Checking…' : 'Recheck'}
                  </button>
                </div>

                {#if inputCheckError}
                  <div class="error-inline">{inputCheckError}</div>
                {:else if isCheckingInputs}
                  <div class="muted">Validating input notes…</div>
                {:else if inspectResult.spends.length === 0}
                  <div class="muted">No spend details available to validate inputs.</div>
                {:else if missingInputs.length > 0}
                  <div class="error-inline">
                    Missing {missingInputs.length} input note{missingInputs.length === 1
                      ? ''
                      : 's'}.
                  </div>
                {:else}
                  <div class="success-inline">All input notes found.</div>
                {/if}
              </div>

              {#if inspectResult?.spends?.length}
                {@const signableCount = inspectResult.spends.filter(s => spendIsSignable(s)).length}
                {@const totalSpends = inspectResult.spends.length}
                <div class="spends-summary">
                  {#if signableCount === totalSpends}
                    <span class="success-text">All spends signable</span>
                  {:else if signableCount > 0}
                    <span>{signableCount} of {totalSpends} spends signable</span>
                  {:else}
                    <span class="muted">No spends signable by selected wallet</span>
                  {/if}
                </div>
                <div class="spends">
                  {#each inspectResult.spends as spend (spend.name)}
                    {@const inputStatus = inputNoteStatus[spend.name] ?? 'unknown'}
                    {@const spendAddresses = spend.lock?.pkh?.pubkeyHashes ?? []}
                    <label class="spend">
                      <input
                        type="checkbox"
                        class="spend-checkbox"
                        checked={selectedSpendNames.has(spend.name)}
                        disabled={!spendIsSignable(spend) ||
                          isSigning ||
                          isCheckingInputs ||
                          inputStatus !== 'present'}
                        on:change={event => {
                          const checked = (event.currentTarget as HTMLInputElement).checked;
                          const next = new Set(selectedSpendNames);
                          if (checked) next.add(spend.name);
                          else next.delete(spend.name);
                          selectedSpendNames = next;
                        }}
                      />
                      <div class="spend-body">
                        <div class="spend-name mono">{shorten(spend.name, 4, 4)}</div>
                        {#if spendAddresses.length > 0}
                          <div class="spend-addresses muted">
                            {#each spendAddresses.slice(0, 2) as addr}
                              <span class="mono">{shorten(addr, 4, 4)}</span>
                            {/each}
                            {#if spendAddresses.length > 2}
                              <span>+{spendAddresses.length - 2} more</span>
                            {/if}
                          </div>
                        {/if}
                        <div class="spend-badges">
                          {#if spend.isFullySigned}
                            <span class="badge success">Signed</span>
                          {:else if spend.signedBy.length > 0}
                            <span class="badge warning">{spend.signedBy.length} sig</span>
                          {:else}
                            <span class="badge">Unsigned</span>
                          {/if}

                          {#if isCheckingInputs}
                            <span class="badge">Checking</span>
                          {:else if inputStatus === 'present'}
                            <span class="badge success">Input ok</span>
                          {:else if inputStatus === 'missing'}
                            <span class="badge danger">Missing</span>
                          {:else}
                            <span class="badge">Unknown</span>
                          {/if}
                        </div>
                      </div>
                    </label>
                  {/each}
                </div>
              {:else}
                <div class="muted">No spend details available.</div>
              {/if}
            </div>
          </div>
        </details>
      {:else}
        <div class="card">
          <div class="row file-row">
            <div class="value">{fileLabel(inspectResult?.format)}</div>
            <div class="file-actions">
              <button
                class="btn-secondary"
                type="button"
                disabled={!jamPayload.trim()}
                on:click={() => downloadPayload(jamPayload, inspectResult)}
              >
                Save file
              </button>
              {#if signedResult && originalJam.trim() && originalJam.trim() !== jamPayload.trim()}
                <button
                  class="btn-secondary"
                  type="button"
                  on:click={() => downloadPayload(originalJam, initialInspectResult)}
                >
                  Save original
                </button>
              {/if}
            </div>
          </div>

          {#if inspectResult?.txId && isFullySigned(inspectResult)}
            <div class="row txid-row">
              <div class="muted">Tx ID</div>
              <div class="tx-actions">
                <div class="value mono">{shorten(inspectResult.txId, 10, 10)}</div>
                <button class="btn-small" type="button" on:click={() => inspectResult && copy(inspectResult.txId)}>
                  Copy
                </button>
              </div>
            </div>
          {/if}
        </div>

        <div class="card">
          <div class="outputs-header">
            <div class="label">Outputs</div>
            <div class="unit-toggle">
              <button
                class="unit-btn"
                class:active={unitMode === 'nocks'}
                on:click={() => (unitMode = 'nocks')}
              >
                nocks
              </button>
              <button
                class="unit-btn"
                class:active={unitMode === 'nicks'}
                on:click={() => (unitMode = 'nicks')}
              >
                nicks
              </button>
            </div>
          </div>
          <div class="outputs">
            {#if inspectResult.outputs.length === 0}
              <div class="muted">No output details available.</div>
            {:else}
              {#each inspectResult.outputs as output}
                {@const outputAddresses = output.lock?.pkh?.pubkeyHashes ?? []}
                {@const isRefund = outputAddresses.some(addr => ownedWalletsByAddress.has(addr))}
                {@const refundWalletName = outputAddresses
                  .map(addr => ownedWalletsByAddress.get(addr)?.name)
                  .find(Boolean)}
                <div class="output" class:refund={isRefund}>
                  <div class="output-main">
                    <div class="output-recipient">
                      <div class="value mono">{describeRecipient(output)}</div>
                      {#if isRefund && refundWalletName}
                        <div class="output-label change">↩ Change to {refundWalletName}</div>
                      {:else}
                        <div class="output-label recipient">Recipient</div>
                      {/if}
                    </div>
                    <div class="amount">{formatGift(output.gift, unitMode)} {unitLabel}</div>
                  </div>
                </div>
              {/each}
            {/if}
          </div>
        </div>

        {#if involvedOwnedWallets.length > 0}
          <div class="card">
            <div class="section-header">
              <div>
                <div class="label">Wallets</div>
                <div class="muted">Balances for wallets referenced by this transaction</div>
              </div>
              <button
                class="btn-small"
                type="button"
                disabled={isRefreshingBalances}
                on:click={() => refreshBalances(true)}
              >
                {isRefreshingBalances ? 'Refreshing…' : 'Refresh'}
              </button>
            </div>

            <div class="wallets">
              {#each involvedOwnedWallets as wallet (wallet.id)}
                {@const walletAddress =
                  wallet.addresses?.[wallet.currentAddressIndex || 0] ??
                  wallet.addresses?.[0] ??
                  ''}
                {@const inputCount = (inspectResult.spends ?? []).filter(spend =>
                  spend.lock?.pkh?.pubkeyHashes?.includes(walletAddress)
                ).length}
                {@const missingCount = (inspectResult.spends ?? []).filter(
                  spend =>
                    spend.lock?.pkh?.pubkeyHashes?.includes(walletAddress) &&
                    inputNoteStatus[spend.name] !== 'present'
                ).length}

                <div class="wallet-row">
                  <div class="wallet-left">
                    <div class="value">{wallet.name}</div>
                    <div class="muted mono">{shorten(walletAddress, 10, 10)}</div>
                    {#if inputCount > 0}
                      <div class="muted">
                        Inputs: {inputCount}
                        {#if missingCount > 0}
                          · missing {missingCount}
                        {/if}
                      </div>
                    {/if}
                  </div>
                  <div class="wallet-right">
                    <div class="amount">{formatWalletBalance(wallet)} NOCK</div>
                  </div>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      {/if}
    {/if}
  </div>
</div>

<style>
  .jam-transaction {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-background);
  }

  .content {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .origin {
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  .card {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 14px;
  }

  .label {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text-secondary);
    margin-bottom: 8px;
  }

  .value {
    font-size: 14px;
    color: var(--color-text);
  }

  .mono {
    font-family:
      ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New',
      monospace;
  }

  .input {
    width: 100%;
    min-height: 110px;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 10px;
    background: var(--color-background);
    color: var(--color-text);
    font-size: 12px;
    resize: vertical;
    outline: none;
  }

  .actions {
    display: flex;
    gap: 8px;
    margin-top: 10px;
    flex-wrap: wrap;
  }

  .btn-primary,
  .btn-secondary {
    border-radius: 10px;
    padding: 12px 14px;
    font-weight: 600;
    cursor: pointer;
    border: 1px solid transparent;
  }

  .btn-primary {
    width: 100%;
    background: var(--color-text);
    color: var(--color-background);
    border-color: var(--color-text);
  }

  .btn-primary:disabled,
  .btn-secondary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .btn-secondary {
    background: var(--color-surface);
    border-color: var(--color-border);
    color: var(--color-text);
  }

  .btn-small {
    border-radius: 999px;
    padding: 8px 10px;
    font-weight: 600;
    cursor: pointer;
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-text);
    font-size: 12px;
    white-space: nowrap;
  }

  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    margin-bottom: 12px;
  }

  .file-row {
    grid-template-columns: 1fr auto;
    align-items: center;
    margin-bottom: 14px;
  }

  .txid-row {
    grid-template-columns: 1fr auto;
    align-items: center;
    margin-bottom: 0;
  }

  .file-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .tx-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
  }

  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 10px;
  }

  .wallets {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 6px;
  }

  .wallet-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 12px;
    border-radius: 12px;
    border: 1px solid var(--color-border);
    background: var(--color-background);
  }

  .wallet-left {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .wallet-right {
    display: flex;
    align-items: baseline;
    justify-content: flex-end;
    min-width: 0;
  }

  .select {
    width: 100%;
    border: 1px solid var(--color-border);
    border-radius: 10px;
    padding: 10px 12px;
    background: var(--color-background);
    color: var(--color-text);
    font-size: 14px;
    outline: none;
  }

  .validation {
    margin-top: 12px;
    padding: 12px;
    border-radius: 12px;
    border: 1px solid var(--color-border);
    background: var(--color-background);
  }

  .validation-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 6px;
  }

  .validation-header .label {
    margin-bottom: 0;
  }

  .error-inline {
    font-size: 12px;
    color: var(--color-text);
    background: rgba(255, 0, 0, 0.08);
    border: 1px solid rgba(255, 0, 0, 0.25);
    border-radius: 10px;
    padding: 10px 12px;
  }

  .success-inline {
    font-size: 12px;
    color: var(--color-text);
    background: rgba(34, 197, 94, 0.1);
    border: 1px solid rgba(34, 197, 94, 0.25);
    border-radius: 10px;
    padding: 10px 12px;
  }

  .outputs-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 10px;
  }

  .outputs-header .label {
    margin-bottom: 0;
  }

  .unit-toggle {
    display: flex;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    overflow: hidden;
  }

  .unit-btn {
    padding: 4px 10px;
    font-size: 12px;
    border: none;
    background: var(--color-surface);
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s;
  }

  .unit-btn:hover {
    background: var(--color-background);
  }

  .unit-btn.active {
    background: var(--color-text);
    color: var(--color-background);
  }

  .outputs {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .output {
    padding: 10px 10px;
    border-radius: 10px;
    background: var(--color-background);
    border: 1px solid var(--color-border);
  }

  .output-main {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
  }

  .output-recipient {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .output-label {
    font-size: 11px;
    color: var(--color-text-tertiary);
  }

  .output-label.change {
    color: rgba(34, 197, 94, 0.8);
  }

  .output-label.recipient {
    color: var(--color-text-tertiary);
  }

  .amount {
    font-weight: 700;
    color: var(--color-text);
    white-space: nowrap;
  }

  .muted {
    font-size: 12px;
    color: var(--color-text-tertiary);
  }

  .output.refund {
    background: rgba(34, 197, 94, 0.06);
    border-color: rgba(34, 197, 94, 0.25);
  }

  .review-title {
    font-size: 14px;
    font-weight: 700;
    color: var(--color-text);
    margin-bottom: 12px;
  }

  .review-step {
    display: grid;
    grid-template-columns: 64px 1fr;
    gap: 12px;
    padding: 12px 0;
    border-top: 1px solid var(--color-border);
  }

  .review-step:first-of-type {
    padding-top: 0;
    border-top: none;
  }

  .review-step-label {
    font-size: 12px;
    font-weight: 700;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }

  .review-step-body {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .review-inline-control {
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .recipients {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 6px;
  }

  .recipient-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 10px;
    border-radius: 10px;
    border: 1px solid var(--color-border);
    background: var(--color-background);
  }

  .recipient-address {
    font-size: 13px;
    color: var(--color-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .recipient-amount {
    font-weight: 700;
    color: var(--color-text);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .review-divider {
    height: 1px;
    background: var(--color-border);
    margin: 12px 0;
  }

  .review-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 0;
  }

  .review-row-label {
    font-size: 13px;
    color: var(--color-text-secondary);
  }

  .review-row-value {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text);
    font-variant-numeric: tabular-nums;
  }

  .review-row.total .review-row-value {
    font-size: 16px;
    font-weight: 800;
  }

  .action-card .error-inline {
    margin-top: 10px;
  }

  .advanced-section summary {
    font-size: 13px;
    font-weight: 700;
    color: var(--color-text);
    cursor: pointer;
  }

  .advanced-content {
    margin-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .advanced-block {
    padding: 12px;
    border-radius: 12px;
    border: 1px solid var(--color-border);
    background: var(--color-background);
  }

  .advanced-block .label {
    margin-bottom: 8px;
  }

  .advanced-block .row {
    margin-bottom: 0;
  }

  .spends-summary {
    margin-top: 12px;
    font-size: 13px;
    font-weight: 500;
  }

  .success-text {
    color: rgba(34, 197, 94, 1);
  }

  .spends {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 8px;
  }

  .spend {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 10px;
    padding: 10px;
    border-radius: 10px;
    border: 1px solid var(--color-border);
    background: var(--color-background);
    align-items: start;
  }

  .spend-checkbox {
    margin-top: 2px;
  }

  .spend-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .spend-name {
    font-size: 13px;
  }

  .spend-addresses {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .spend-badges {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    align-items: center;
  }

  .badge {
    font-size: 12px;
    padding: 2px 8px;
    border-radius: 999px;
    border: 1px solid var(--color-border);
    color: var(--color-text-secondary);
    background: var(--color-surface);
    white-space: nowrap;
  }

  .badge.success {
    border-color: rgba(34, 197, 94, 0.5);
    color: rgba(34, 197, 94, 1);
  }

  .badge.warning {
    border-color: rgba(245, 158, 11, 0.6);
    color: rgba(245, 158, 11, 1);
  }

  .badge.danger {
    border-color: rgba(239, 68, 68, 0.6);
    color: rgba(239, 68, 68, 1);
  }

  .password {
    margin-top: 12px;
  }

  .password-input {
    min-height: unset;
  }

  .signing-actions {
    flex-direction: column;
  }

  .error {
    padding: 10px 12px;
    background: rgba(255, 0, 0, 0.08);
    border: 1px solid rgba(255, 0, 0, 0.25);
    border-radius: 12px;
    color: var(--color-text);
    font-size: 13px;
  }

  .signed {
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--color-border);
  }
</style>
