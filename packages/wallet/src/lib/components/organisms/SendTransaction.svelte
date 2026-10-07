<script lang="ts">
  import { formatUsdEstimate } from '../../utils/usd';
  import PaymentPassword from '../molecules/PaymentPassword.svelte';
  let confirmingPayment = false;
  import { onMount, onDestroy } from 'svelte';
  import Header from '../molecules/Header.svelte';
  import { router } from '../../stores/router';
  import { walletStore, activeWallet } from '../../stores/wallet';
  import { nockPrice } from '../../stores/price';
  import {
    nocksToNicks,
    nicksToNocks,
    formatNocksNumberWithSeparator,
    formatNocksWithSeparator,
    parseNocksInput,
    formatFeeInNicks
  } from '../../utils/nicks';
  import { collectReservedNoteIds, noteNameToId } from '../../utils/noteIds';
  import { STANDARD_NETWORK_FEE, type Nicks } from '../../types/nicks';
  import {
    getMinNetworkFee,
    createSignedTransaction,
    getVaultNickname
  } from '../../utils/vaultBridge';
  import { previewHardwareTransaction } from '../../utils/hardwareTx';
  import type { NoteV1 } from '../../utils/rpc';

  export let balance: number = 1234.56;
  export let desktopLayout = false;
  export const onSend: (data: any) => void = () => {};
  export let onBack: () => void = () => {};
  export const onAddressBook: () => void = () => {};

  import { BRIDGE_LOCK_ROOT, BRIDGE_CONFIRMATION_BLOCKS, bridgeDetails, parseBaseAddress, verifyBridgeTransaction, type BridgeDetails } from '../../utils/bridge';
  const BRIDGE_MINIMUM_NOCK = 100_000; // 100k NOCK minimum for bridge

  type Recipient = {
    address: string;
    amount: string;
  };

  let recipients: Recipient[] = [{ address: '', amount: '' }];
  let isSending = false;
  let error: string | null = null;
  let privateOutputs = false;

  // Bridge mode state
  let bridgeMode = false;
  let bridgeEvmAddress = '';
  let bridgeAmountNocks = '';

  $: isHardwareWallet = Boolean($activeWallet?.hardware);
  $: if (isHardwareWallet && privateOutputs) {
    privateOutputs = false;
  }

  // Bridge validation
  $: bridgeAmountNicks = parseNocksInput(bridgeAmountNocks) || 0n;
  $: bridgeAmountInNocks = nicksToNocks(bridgeAmountNicks);
  $: bridgeAmountValid = bridgeAmountInNocks >= BRIDGE_MINIMUM_NOCK;
  function validBaseAddress(address: string): boolean {
    try { parseBaseAddress(address); return true; } catch { return false; }
  }
  function quoteBridge(address: string, value: bigint): BridgeDetails | null {
    try { return bridgeDetails(address, value); } catch { return null; }
  }
  $: bridgeEvmAddressValid = validBaseAddress(bridgeEvmAddress);
  $: bridgeQuote = quoteBridge(bridgeEvmAddress, bridgeAmountNicks);
  $: bridgeValid = bridgeAmountValid && bridgeEvmAddressValid;

  // Dynamic fee from transaction preview
  let calculatedFee: bigint | null = null;
  let isCalculatingFee = false;
  let feeError: string | null = null;
  let feeDebounceTimer: ReturnType<typeof setTimeout> | null = null;

  // Cached transaction from preview (reused on send to avoid building twice)
  let cachedTransaction: {
    signedTx: string;
    feePaid: number;
    txId: string;
    inputNotes?: string[];
  } | null = null;
  let previewExpires = 0;
  let cachedRecipientsKey: string | null = null; // To detect if recipients changed

  // Minimum network fee as fallback
  let minNetworkFee: bigint = STANDARD_NETWORK_FEE;

  // Check if we're returning from address book with a selected address
  onMount(() => {
    let mounted = true;
    void getMinNetworkFee()
      .then(fee => {
        if (mounted) minNetworkFee = fee;
      })
      .catch(() => {});

    const unsubscribe = router.subscribe(state => {
      // Restore recipients state if available
      if (state.routeData?.recipients) {
        console.log('[SendTransaction] Restoring recipients state:', state.routeData.recipients);
        recipients = state.routeData.recipients;
      }

      // Restore scroll position if available
      if (typeof state.routeData?.scrollPosition === 'number') {
        console.log('[SendTransaction] Restoring scroll position:', state.routeData.scrollPosition);
        requestAnimationFrame(() => {
          const content = scrollContainer();
          if (content) {
            content.scrollTop = state.routeData.scrollPosition;
          }
        });
      }

      // Handle address book selection
      if (
        state.routeData?.fromAddressBook &&
        state.routeData?.address &&
        typeof state.routeData?.recipientIndex === 'number'
      ) {
        const recipientIndex = state.routeData.recipientIndex;

        console.log(
          `[SendTransaction] Populating recipient #${recipientIndex} with address:`,
          state.routeData.address
        );

        // Populate the specific recipient field that was clicked
        recipients[recipientIndex].address = state.routeData.address;
        recipients = [...recipients]; // Trigger reactivity
      }
    });
    return () => {
      mounted = false;
      unsubscribe();
    };
  });

  function scrollContainer() {
    const content = document.querySelector('.send-transaction .content');
    return content?.closest('.desktop-workspace') ?? content;
  }

  function openAddressBook(index: number) {
    console.log(`[SendTransaction] Opening address book for recipient #${index}`);

    // Save current scroll position
    const content = scrollContainer();
    const scrollPosition = content ? content.scrollTop : 0;

    // Navigate to address book with return context, recipient index, scroll position, AND current recipients state
    router.navigate('address-book', {
      returnTo: 'send',
      recipientIndex: index,
      recipients: recipients, // Preserve current recipients
      scrollPosition: scrollPosition // Preserve scroll position
    });
  }

  function addRecipient() {
    recipients = [...recipients, { address: '', amount: '' }];
  }

  function removeRecipient(index: number) {
    if (recipients.length > 1) {
      recipients = recipients.filter((_, i) => i !== index);
    }
  }

  function formatUSD(nockAmount: number): string {
    return formatUsdEstimate(nockAmount, $nockPrice) ?? '';
  }

  async function setSendAll() {
    if (recipients.length !== 1 || !$activeWallet || !recipients[0].address.trim()) {
      error = 'Enter a recipient address before choosing Max';
      return;
    }
    const walletId = $activeWallet.id;
    const address = recipients[0].address.trim();
    const source = $activeWallet.addresses[0];
    try {
      const { ensureVaultReady } = await import('../../../vault/wasm');
      const { maxSendAmount } = await import('../../../pkg/nockster_core.js');
      const { getRPCClientV1 } = await import('../../utils/rpc');
      await ensureVaultReady();
      const height = await getRPCClientV1().getTipHeight();
      const amount = maxSendAmount(source, availableNotes, address, privateOutputs, height);
      if (
        $activeWallet?.id !== walletId ||
        recipients.length !== 1 ||
        recipients[0].address.trim() !== address
      )
        return;
      const whole = amount / 65536n;
      const fractional = (((amount % 65536n) * 10n ** 16n) / 65536n)
        .toString()
        .padStart(16, '0')
        .replace(/0+$/, '');
      recipients[0].amount = `${whole}${fractional ? '.' + fractional : ''}`;
      recipients = [...recipients];
      error = null;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    }
  }

  function formatNumber(value: string): string {
    const num = parseFloat(value) || 0;
    return new Intl.NumberFormat('en-US', {
      minimumFractionDigits: 2,
      maximumFractionDigits: 8
    }).format(num);
  }

  // Reactive computation of totals - updates when recipients or fee changes
  $: totalAmountNicks = recipients.reduce((sum, r) => {
    const amountNicks = parseNocksInput(r.amount) || 0n;
    return sum + amountNicks;
  }, 0n);

  // Use calculated fee if available, otherwise fall back to minimum
  $: displayFee = calculatedFee !== null ? calculatedFee : minNetworkFee;

  $: reservedNoteIds = collectReservedNoteIds($activeWallet?.pendingTransactions);
  $: availableNotes =
    reservedNoteIds.size > 0
      ? ($activeWallet?.notes || []).filter(note => {
          const noteId = noteNameToId(note);
          return !noteId || !reservedNoteIds.has(noteId);
        })
      : $activeWallet?.notes || [];

  $: availableBalanceNicks = availableNotes.reduce((sum, note) => {
    const assets = (note as any)?.assets;
    const nicks = typeof assets === 'number' ? BigInt(Math.trunc(assets)) : 0n;
    return sum + nicks;
  }, 0n);

  $: hasLoadedNotes = Array.isArray($activeWallet?.notes);
  $: availableBalance = hasLoadedNotes ? nicksToNocks(availableBalanceNicks) : balance;

  $: totals = {
    amountNicks: totalAmountNicks,
    feeNicks: displayFee,
    totalNicks: totalAmountNicks + displayFee,
    // Also provide NOCK values for display
    amount: nicksToNocks(totalAmountNicks),
    fee: nicksToNocks(displayFee),
    total: nicksToNocks(totalAmountNicks + displayFee)
  };

  function handleAmountInput(e: Event, index: number) {
    const input = e.target as HTMLInputElement;
    const value = input.value.replace(/,/g, '');
    if (!isNaN(parseFloat(value)) || value === '') {
      recipients[index].amount = value;
      recipients = [...recipients]; // Trigger reactivity
    }
  }

  let previewGeneration = 0;
  $: canPreviewFee = bridgeMode
    ? bridgeValid
    : recipients.length > 0 &&
      recipients.every(r => r.address.trim() && (parseNocksInput(r.amount) ?? 0n) > 0n);

  // Preview transaction to calculate dynamic fee
  async function previewTransactionFee() {
    const generation = previewGeneration;
    const previewPrivateOutputs = bridgeMode ? false : privateOutputs;
    const previewBridge = bridgeMode ? bridgeQuote : null;
    previewExpires = 0;
    cachedTransaction = null;
    cachedRecipientsKey = null;
    const wallet = $activeWallet;
    if (!wallet) {
      feeError = 'No wallet selected';
      return;
    }

    const notes = availableNotes;
    if (notes.length === 0) {
      feeError = 'No notes available';
      return;
    }

    // Get valid recipients
    const validRecipients = bridgeMode
      ? (previewBridge ? [{ address: BRIDGE_LOCK_ROOT, amount: bridgeAmountNocks, bridgeEvmAddress: previewBridge.destination }] : [])
      : recipients.filter(r => r.address.trim() && (parseNocksInput(r.amount) ?? 0n) > 0n);

    if (validRecipients.length === 0 || (!bridgeMode && validRecipients.length !== recipients.length)) {
      calculatedFee = null;
      feeError = null;
      return;
    }

    isCalculatingFee = true;
    feeError = null;

    try {
      if (wallet.hardware) {
        if (privateOutputs) {
          feeError = 'Hardware wallets do not support private outputs yet.';
          calculatedFee = null;
          return;
        }

        const hardwarePreview = await previewHardwareTransaction(
          wallet.addresses[0],
          notes as unknown as NoteV1[],
          validRecipients.map(r => ({
            address: r.address.trim(),
            amount: Number(parseNocksInput(r.amount) || 0n),
            bridgeEvmAddress: previewBridge?.destination
          }))
        );
        if (generation !== previewGeneration) return;
        calculatedFee = BigInt(hardwarePreview.feePaid);
        previewExpires = Date.now() + 120_000;
        feeError = null;
        cachedTransaction = null;
        cachedRecipientsKey = null;
        return;
      }

      const vaultRecipients = validRecipients.map(r => ({
        address: r.address.trim(),
        gift: Number(parseNocksInput(r.amount) || 0n),
        ...(previewBridge ? { bridgeEvmAddress: previewBridge.destination } : {})
      }));

      const walletAddress = wallet.addresses[0];
      const nickname = getVaultNickname(wallet);
      const vaultNotes = notes as unknown as Array<Record<string, unknown>>;

      const reservedNotesKey = reservedNoteIds.size > 0 ? Array.from(reservedNoteIds).sort() : [];

      const result = await createSignedTransaction(
        nickname,
        vaultNotes,
        vaultRecipients,
        walletAddress,
        { privateOutputs: previewPrivateOutputs }
      );

      if (generation !== previewGeneration) return;
      if (result.success && result.feePaid !== undefined) {
        if (previewBridge && result.signedTx) {
          await verifyBridgeTransaction(result.signedTx, walletAddress, [{ address: BRIDGE_LOCK_ROOT, amount: previewBridge.amount, bridgeEvmAddress: previewBridge.destination }], result.feePaid);
          if (generation !== previewGeneration) return;
        }
        calculatedFee = BigInt(result.feePaid);
        previewExpires = Date.now() + 120_000;
        feeError = null;

        // Cache the transaction for reuse on send (avoids building twice)
        if (result.signedTx && result.txId) {
          cachedTransaction = {
            signedTx: result.signedTx,
            feePaid: result.feePaid,
            txId: result.txId,
            inputNotes: result.inputNotes
          };
          // Create a key to detect if recipients changed
          cachedRecipientsKey = JSON.stringify({
            recipients: vaultRecipients,
            privateOutputs: previewPrivateOutputs,
            walletId: wallet.id,
            notes: vaultNotes,
            reservedNotesKey
          });
        }
      } else {
        feeError = result.error || 'Failed to calculate fee';
        calculatedFee = null;
        cachedTransaction = null;
        cachedRecipientsKey = null;
      }
    } catch (err) {
      if (generation !== previewGeneration) return;
      console.error('Fee preview error:', err);
      feeError = err instanceof Error ? err.message : 'Failed to calculate fee';
      calculatedFee = null;
      cachedTransaction = null;
      cachedRecipientsKey = null;
    } finally {
      if (generation === previewGeneration) isCalculatingFee = false;
    }
  }

  // Debounced fee calculation - triggers when recipients change
  function debouncedFeePreview() {
    previewGeneration++;
    cachedTransaction = null;
    cachedRecipientsKey = null;
    calculatedFee = null;
    isCalculatingFee = false;
    if (desktopLayout) feeError = null;
    if (feeDebounceTimer) {
      clearTimeout(feeDebounceTimer);
    }
    if (desktopLayout && !canPreviewFee) return;
    feeDebounceTimer = setTimeout(() => {
      previewTransactionFee();
    }, 500); // 500ms debounce
  }

  // Trigger fee preview when recipients change
  $: if (recipients && privateOutputs !== undefined && availableNotes && $activeWallet && bridgeMode !== undefined && bridgeEvmAddress !== undefined && bridgeAmountNocks !== undefined && canPreviewFee !== undefined) {
    debouncedFeePreview();
  }

  // Cleanup on destroy
  onDestroy(() => {
    previewGeneration++;
    if (feeDebounceTimer) {
      clearTimeout(feeDebounceTimer);
    }
  });

  async function handleSendTransaction() {
    // Handle bridge mode
    if (bridgeMode) {
      return handleBridgeTransaction();
    }

    // Validate all recipients
    const validRecipients = recipients.filter(
      r => r.address.trim() && (parseNocksInput(r.amount) ?? 0n) > 0n
    );

    if (validRecipients.length === 0 || validRecipients.length !== recipients.length) {
      error = 'Please add at least one recipient with a valid address and amount';
      return;
    }

    if (isHardwareWallet && privateOutputs) {
      error = 'Hardware wallets do not support private outputs yet.';
      return;
    }

    isSending = true;
    error = null;

    try {
      // Convert all recipients to the correct format (amounts in nicks)
      const recipientsData = validRecipients.map(r => {
        const amountNicks = parseNocksInput(r.amount);
        if (!amountNicks || amountNicks <= 0n) {
          throw new Error(`Invalid amount for address ${r.address}`);
        }
        return {
          address: r.address.trim(), // Trim whitespace from address
          amount: Number(amountNicks) // sendTransaction expects nicks as number
        };
      });

      // Check if we can reuse the cached transaction from fee preview
      const currentRecipientsKey = JSON.stringify({
        recipients: recipientsData.map(r => ({ address: r.address, gift: r.amount })),
        privateOutputs,
        walletId: $activeWallet?.id,
        notes: availableNotes,
        reservedNotesKey: reservedNoteIds.size > 0 ? Array.from(reservedNoteIds).sort() : []
      });
      const canUseCachedTx = cachedTransaction && cachedRecipientsKey === currentRecipientsKey;

      if (canUseCachedTx) {
        console.log('Reusing cached transaction from fee preview');
      }

      await walletStore.sendTransaction(
        recipientsData,
        'standard',
        canUseCachedTx ? (cachedTransaction ?? undefined) : undefined,
        { privateOutputs }
      );

      // Clear cache after successful send
      cachedTransaction = null;
      cachedRecipientsKey = null;

      // Navigate to transaction success screen
      router.navigate('dashboard');
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to send transaction';
      console.error('Transaction error:', err);
    } finally {
      isSending = false;
    }
  }

  async function handleBridgeTransaction() {
    isSending = true;
    error = null;
    try {
      const quote = bridgeDetails(bridgeEvmAddress, bridgeAmountNicks);
      const bridgeRecipient = { address: BRIDGE_LOCK_ROOT, amount: quote.amount, bridgeEvmAddress: quote.destination };
      const currentKey = JSON.stringify({
        recipients: [{ address: BRIDGE_LOCK_ROOT, gift: quote.amount, bridgeEvmAddress: quote.destination }],
        privateOutputs: false, walletId: $activeWallet?.id, notes: availableNotes,
        reservedNotesKey: reservedNoteIds.size > 0 ? Array.from(reservedNoteIds).sort() : []
      });
      if (calculatedFee === null || previewExpires < Date.now() || (!isHardwareWallet && (!cachedTransaction || currentKey !== cachedRecipientsKey))) {
        throw new Error('Bridge preview expired or changed. Review the refreshed fee before confirming.');
      }
      await walletStore.sendTransaction([bridgeRecipient], 'standard', cachedTransaction ?? undefined,
        { privateOutputs: false, expectedNetworkFee: Number(calculatedFee) });
      router.navigate('dashboard');
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to create bridge transaction';
      debouncedFeePreview();
    } finally {
      isSending = false;
    }
  }

  function toggleBridgeMode() {
    bridgeMode = !bridgeMode;
    if (bridgeMode) {
      // Reset regular recipients when entering bridge mode
      recipients = [{ address: '', amount: '' }];
      privateOutputs = false;
    } else {
      // Reset bridge state when exiting bridge mode
      bridgeEvmAddress = '';
      bridgeAmountNocks = '';
    }
  }
</script>

<div class="send-transaction fixed-screen" class:desktop-layout={desktopLayout}>
  <Header title={bridgeMode ? 'Bridge to Base' : 'Send'} showBack={true} on:click={onBack} />

  <div class="content">
    <!-- Mode Toggle -->
    <div class="mode-toggle-section">
      <button
        class="mode-toggle-btn"
        class:active={!bridgeMode}
        onclick={() => bridgeMode && toggleBridgeMode()}
      >
        Send
      </button>
      <button
        class="mode-toggle-btn"
        class:active={bridgeMode}
        onclick={() => !bridgeMode && toggleBridgeMode()}
      >
        Bridge to Base
      </button>
      <a
        href="https://dexscreener.com/base/0x85f1aa3a70fedd1c52705c15baed143e675cd626"
        target="_blank"
        rel="noopener noreferrer"
        class="bridge-info-link"
        title="View wNOCK on Base"
      >
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
          <circle cx="8" cy="8" r="7" stroke="currentColor" stroke-width="1.5" />
          <path d="M8 7V11" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
          <circle cx="8" cy="5" r="0.75" fill="currentColor" />
        </svg>
      </a>
    </div>

    {#if bridgeMode}
      <!-- Bridge Mode Form -->
      <div class="bridge-section">
        <div class="bridge-warning">
          <svg width="20" height="20" viewBox="0 0 20 20" fill="none">
            <path
              d="M10 6V10M10 14H10.01M19 10C19 14.9706 14.9706 19 10 19C5.02944 19 1 14.9706 1 10C1 5.02944 5.02944 1 10 1C14.9706 1 19 5.02944 19 10Z"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
          <div class="bridge-warning-text">
            <div class="bridge-warning-title">Important</div>
            <div class="bridge-warning-desc">
              Bridge transactions require at least <strong
                >{BRIDGE_MINIMUM_NOCK.toLocaleString()} NOCK</strong
              >
              and wait at least <strong>{BRIDGE_CONFIRMATION_BLOCKS} blocks</strong> after L1 confirmation
              before bridge processing.
            </div>
          </div>
        </div>

        <div class="recipient-card">
          <div class="input-group">
            <label for="bridge-evm-address" class="input-label">Base Chain Address</label>
            <div class="input-wrapper">
              <input
                id="bridge-evm-address"
                type="text"
                class="input-field"
                class:input-error={bridgeEvmAddress && !bridgeEvmAddressValid}
                placeholder="0x..."
                bind:value={bridgeEvmAddress}
              />
            </div>
            {#if bridgeEvmAddress && !bridgeEvmAddressValid}
              <div class="input-hint error">Enter a valid EVM address (0x + 40 hex characters)</div>
            {/if}
          </div>

          <div class="input-group">
            <div class="label-row">
              <label for="bridge-amount" class="input-label">Amount</label>
              <span class="amount-hint">Min: {BRIDGE_MINIMUM_NOCK.toLocaleString()} NOCK</span>
            </div>
            <div class="input-wrapper">
              <input
                id="bridge-amount"
                type="text"
                class="input-field"
                class:input-error={bridgeAmountNocks && !bridgeAmountValid}
                placeholder="100,000"
                bind:value={bridgeAmountNocks}
              />
              <span class="input-suffix">NOCK</span>
            </div>
            {#if desktopLayout || (bridgeAmountNocks.trim() && parseNocksInput(bridgeAmountNocks) !== null)}
              <div class="usd-subtitle">{bridgeAmountNocks.trim() && parseNocksInput(bridgeAmountNocks) !== null ? formatUSD(bridgeAmountInNocks) : ''}</div>
            {/if}
            {#if bridgeAmountNocks && !bridgeAmountValid}
              <div class="input-hint error">
                Minimum {BRIDGE_MINIMUM_NOCK.toLocaleString()} NOCK required
              </div>
            {/if}
          </div>
        </div>

        <div class="balance-hint">
          Available: {formatNocksNumberWithSeparator(availableBalance, 8)} NOCK
        </div>
      </div>
    {:else}
      <!-- Normal Send Form -->
      <div class="recipients-section">
        <div class="section-header">
          <div class="input-label">Recipients</div>
          <button class="add-recipient-btn" onclick={addRecipient} aria-label="Add recipient">
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
              <path
                d="M8 3V13M3 8H13"
                stroke="currentColor"
                stroke-width="1.5"
                stroke-linecap="round"
              />
            </svg>
            Add Recipient
          </button>
        </div>

        {#each recipients as recipient, index}
          <div class="recipient-card">
            <div class="recipient-header">
              <span class="recipient-number">#{index + 1}</span>
              {#if recipients.length > 1}
                <button
                  class="remove-recipient-btn"
                  onclick={() => removeRecipient(index)}
                  aria-label="Remove recipient"
                >
                  <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                    <path
                      d="M4 4L12 12M4 12L12 4"
                      stroke="currentColor"
                      stroke-width="1.5"
                      stroke-linecap="round"
                    />
                  </svg>
                </button>
              {/if}
            </div>

            <div class="input-group">
              <label for="recipient-{index}" class="input-label">Address</label>
              <div class="input-wrapper">
                <input
                  id="recipient-{index}"
                  type="text"
                  class="input-field"
                  placeholder="Enter address or select contact"
                  bind:value={recipient.address}
                />
                <button
                  class="input-action"
                  title="Address Book"
                  aria-label="Address Book"
                  onclick={() => openAddressBook(index)}
                >
                  <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
                    <path
                      d="M9 9C10.6569 9 12 7.65685 12 6C12 4.34315 10.6569 3 9 3C7.34315 3 6 4.34315 6 6C6 7.65685 7.34315 9 9 9Z"
                      stroke="currentColor"
                      stroke-width="1.5"
                    />
                    <path
                      d="M3 15C3 12.7909 5.23858 11 9 11C12.7614 11 15 12.7909 15 15"
                      stroke="currentColor"
                      stroke-width="1.5"
                      stroke-linecap="round"
                    />
                  </svg>
                </button>
              </div>
            </div>

            <div class="input-group">
              <div class="label-row">
                <label for="amount-{index}" class="input-label">Amount</label>
                {#if recipients.length === 1}
                  <button class="send-all-btn" onclick={setSendAll}> Send All </button>
                {/if}
              </div>
              <div class="input-wrapper">
                <input
                  id="amount-{index}"
                  type="text"
                  class="input-field"
                  placeholder="0.00"
                  bind:value={recipient.amount}
                  oninput={e => handleAmountInput(e, index)}
                />
                <span class="input-suffix">NOCK</span>
              </div>
              {#if desktopLayout || (recipient.amount.trim() && parseNocksInput(recipient.amount) !== null)}
                <div class="usd-subtitle">{recipient.amount.trim() && parseNocksInput(recipient.amount) !== null ? formatUSD(Number(parseNocksInput(recipient.amount)) / 65536) : ''}</div>
              {/if}
            </div>
          </div>
        {/each}

        <div class="balance-hint">
          Available: {formatNocksNumberWithSeparator(availableBalance, 8)} NOCK
        </div>
      </div>
    {/if}

    {#if !bridgeMode}
      <div class="fee-section">
        <div class="fee-display">
          <span class="fee-label">Network Fee</span>
          <div class="fee-amount-group" aria-busy={isCalculatingFee}>
            {#if !desktopLayout && (isCalculatingFee || calculatedFee === null)}
              <span class="fee-skeleton"></span>
            {:else}
            <span class="fee-amount-nicks">{isCalculatingFee ? 'Calculating…' : calculatedFee !== null ? formatFeeInNicks(calculatedFee) : '—'}</span>
            <span class="usd-subtitle">{calculatedFee !== null ? formatUSD(Number(calculatedFee) / 65536) : ''}</span>
            {/if}
          </div>
        </div>
      </div>

      <div class="privacy-card">
        <div class="privacy-row">
          <div class="privacy-text">
            <div class="privacy-title">Private outputs</div>
            <div class="privacy-subtitle">Omit lock noteData from output notes</div>
          </div>
          <button
            class="toggle"
            class:active={privateOutputs}
            aria-label="Toggle private outputs"
            disabled={isHardwareWallet}
            onclick={() => (privateOutputs = !privateOutputs)}
          >
            <span class="toggle-slider"></span>
          </button>
        </div>
        {#if isHardwareWallet}
          <div class="privacy-note">Hardware wallets always use public outputs.</div>
        {/if}
      </div>

      <div class="summary-card">
        <div class="summary-section-label">Transaction Summary</div>

        {#each desktopLayout ? recipients : recipients.filter(r => r.amount && parseFloat(r.amount) > 0) as recipient, index}
          <div class="summary-row recipient-summary">
            <span class="summary-label">Recipient #{index + 1}</span>
            <span class="summary-value">{recipient.amount.trim() ? `${formatNumber(recipient.amount)} NOCK` : '—'}<span class="usd-subtitle">{recipient.amount.trim() ? formatUSD(Number(parseNocksInput(recipient.amount) ?? 0n) / 65536) : ''}</span></span>
          </div>
        {/each}

        <div class="summary-row">
          <span class="summary-label">Net sent</span>
          <span class="summary-value">{formatNumber(totals.amount.toString())} NOCK<span class="usd-subtitle">{formatUSD(totals.amount)}</span></span>
        </div>
        <div class="summary-row">
          <span class="summary-label">Total fees</span>
          <span class="summary-value" aria-busy={isCalculatingFee}>
            {#if !desktopLayout && calculatedFee === null}
              <span class="fee-skeleton-small"></span>
            {:else}
            {isCalculatingFee ? 'Calculating…' : calculatedFee !== null ? formatFeeInNicks(totals.feeNicks) : '—'}
            <span class="usd-subtitle">{calculatedFee !== null ? formatUSD(Number(totals.feeNicks) / 65536) : ''}</span>
            {/if}
          </span>
        </div>
        <div class="summary-row total">
          <span class="summary-label-total">Total</span>
          <span class="summary-value-total" aria-busy={isCalculatingFee}>
            {#if !desktopLayout && calculatedFee === null}
              <span class="fee-skeleton-small"></span>
            {:else}
            {isCalculatingFee ? 'Calculating…' : calculatedFee !== null ? `${formatNumber(totals.total.toString())} NOCK` : '—'}
            <span class="usd-subtitle">{calculatedFee !== null ? formatUSD(totals.total) : ''}</span>
            {/if}
          </span>
        </div>
      </div>
    {:else}
      <!-- Bridge Summary -->
      <div class="summary-card">
        <div class="summary-section-label">Bridge Summary</div>

        <div class="summary-row">
          <span class="summary-label">Destination</span>
          <span class="summary-value bridge-address" title={bridgeEvmAddress}>
            {bridgeQuote?.destination ?? (bridgeEvmAddress || '-')}
          </span>
        </div>
        <div class="summary-row">
          <span class="summary-label">Amount</span>
          <span class="summary-value">
            {bridgeAmountNocks ? formatNumber(bridgeAmountInNocks.toString()) : '0'} NOCK
            <span class="usd-subtitle">{bridgeAmountNocks ? formatUSD(bridgeAmountInNocks) : ''}</span>
          </span>
        </div>
        <div class="summary-row">
          <span class="summary-label">Protocol fee (≈0.3%, from payout)</span>
          <span class="summary-value">{bridgeQuote ? formatNocksWithSeparator(BigInt(bridgeQuote.protocolFee), 8) : '—'} NOCK</span>
        </div>
        <div class="summary-row">
          <span class="summary-label">Expected on Base</span>
          <span class="summary-value">{bridgeQuote ? formatNocksWithSeparator(BigInt(bridgeQuote.expectedReceived), 8) : '—'} NOCK</span>
        </div>
        <div class="summary-row">
          <span class="summary-label">Network fee</span>
          <span class="summary-value">{isCalculatingFee ? 'Calculating…' : calculatedFee === null ? '—' : formatNocksWithSeparator(calculatedFee, 8) + ' NOCK'}</span>
        </div>
        <div class="summary-row">
          <span class="summary-label">Total NOCK debited</span>
          <span class="summary-value">{bridgeQuote && calculatedFee !== null ? formatNocksWithSeparator(BigInt(bridgeQuote.amount) + calculatedFee, 8) : '—'} NOCK</span>
        </div>
        {#if feeError}<div class="error-message">{feeError}</div>{/if}
        <div class="summary-row">
          <span class="summary-label">Estimated wait</span>
          <span class="summary-value">At least {BRIDGE_CONFIRMATION_BLOCKS} confirmations + bridge processing</span>
        </div>
      </div>
    {/if}
  </div>

  <div class="button-footer">
    {#if error}
      <div class="error-message">
        {error}
      </div>
    {/if}
    {#if bridgeMode}
      <button
        class="btn-primary bridge-btn"
        onclick={() => { confirmingPayment = true; }}
        disabled={!bridgeValid || isSending || isCalculatingFee || calculatedFee === null || Boolean(feeError)}
      >
        {isSending ? 'Bridging...' : 'Bridge to Base'}
      </button>
    {:else}
      <button
        class="btn-primary"
        onclick={() => { confirmingPayment = true; }}
        disabled={recipients.filter(r => r.address.trim() && (parseNocksInput(r.amount) ?? 0n) > 0n)
          .length === 0 || isSending}
      >
        {isSending ? 'Sending...' : 'Send Transaction'}
      </button>
    {/if}
  </div>
</div>

{#if confirmingPayment}
  <PaymentPassword onCancel={() => { confirmingPayment = false; }} onConfirm={async () => { confirmingPayment = false; await handleSendTransaction(); }} />
{/if}

<style>
  .usd-subtitle { display: block; font-size: 12px; font-weight: 400; color: var(--color-text-secondary, #6b7280); margin-top: 4px; }
  .desktop-layout .usd-subtitle { min-height: 18px; line-height: 18px; }
  .desktop-layout .recipient-header { min-height: 32px; }
  .desktop-layout .fee-amount-group { line-height: 21px; gap: 0; }
  .desktop-layout .summary-value { text-align: right; line-height: 21px; }
  .send-transaction {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-background);
  }

  .content {
    flex: 1;
    display: flex;
    flex-direction: column;
    padding: 14px;
    overflow-y: auto;
    gap: 12px;
  }

  .recipients-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .add-recipient-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    background: var(--color-surface);
    border: 1.5px solid var(--color-border);
    border-radius: 8px;
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .add-recipient-btn:hover {
    border-color: var(--color-text-tertiary);
    background: var(--color-border);
  }

  .recipient-card {
    background: var(--color-surface);
    border: 1.5px solid var(--color-border);
    border-radius: 12px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .recipient-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .recipient-number {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text-secondary);
  }

  .remove-recipient-btn {
    padding: 4px;
    background: transparent;
    border: none;
    color: var(--color-error);
    cursor: pointer;
    transition: all 0.15s ease;
    border-radius: 4px;
  }

  .remove-recipient-btn:hover {
    background: rgba(220, 38, 38, 0.1);
  }

  .input-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .input-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--color-text-secondary);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .label-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .send-all-btn {
    font-size: 12px;
    font-weight: 500;
    color: #3b82f6;
    background: none;
    border: none;
    cursor: pointer;
    padding: 0;
    transition: color 0.15s ease;
  }

  .send-all-btn:hover {
    color: #2563eb;
    text-decoration: underline;
  }

  .input-wrapper {
    position: relative;
    display: flex;
    align-items: center;
  }

  .input-field {
    width: 100%;
    padding: 10px 12px;
    padding-right: 50px;
    border: 1.5px solid var(--color-border);
    border-radius: 8px;
    background: var(--color-surface);
    font-size: 15px;
    color: var(--color-text);
    outline: none;
    transition: all 0.15s ease;
  }

  .input-field:focus {
    border-color: var(--color-text);
  }

  .input-field::placeholder {
    color: var(--color-text-tertiary);
  }

  .input-action {
    position: absolute;
    right: 12px;
    padding: 6px;
    background: none;
    border: none;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
    border-radius: 6px;
  }

  .input-action:hover {
    background: var(--color-surface);
    color: var(--color-text);
  }

  .input-suffix {
    position: absolute;
    right: 16px;
    font-size: 14px;
    font-weight: 500;
    color: var(--color-text-secondary);
    pointer-events: none;
  }

  .balance-hint {
    font-size: 12px;
    color: var(--color-text-tertiary);
  }

  .fee-section {
    background: var(--color-surface);
    border: 1.5px solid var(--color-border);
    border-radius: 12px;
    padding: 12px;
  }

  .fee-display {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .fee-label {
    font-size: 13px;
    font-weight: 500;
    color: var(--color-text-secondary);
  }

  .fee-amount-group {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
  }

  .fee-amount-nicks {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text);
  }

  .fee-skeleton {
    display: inline-block;
    width: 60px;
    height: 14px;
    background: linear-gradient(
      90deg,
      var(--color-border) 25%,
      var(--color-surface) 50%,
      var(--color-border) 75%
    );
    background-size: 200% 100%;
    animation: skeleton-shimmer 1.5s infinite;
    border-radius: 4px;
  }

  .fee-skeleton-small {
    display: inline-block;
    width: 50px;
    height: 12px;
    background: linear-gradient(
      90deg,
      var(--color-border) 25%,
      var(--color-surface) 50%,
      var(--color-border) 75%
    );
    background-size: 200% 100%;
    animation: skeleton-shimmer 1.5s infinite;
    border-radius: 3px;
    vertical-align: middle;
  }

  @keyframes skeleton-shimmer {
    0% {
      background-position: 200% 0;
    }
    100% {
      background-position: -200% 0;
    }
  }

  .fee-amount-nock {
    font-size: 11px;
    font-weight: 400;
    color: var(--color-text-tertiary);
  }

  .privacy-card {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 10px;
  }

  .privacy-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .privacy-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .privacy-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-text);
  }

  .privacy-subtitle {
    font-size: 12px;
    color: var(--color-text-secondary);
  }

  .privacy-note {
    margin-top: 6px;
    font-size: 11px;
    color: var(--color-text-tertiary);
  }

  .toggle {
    position: relative;
    width: 44px;
    height: 24px;
    background: var(--color-border);
    border: none;
    border-radius: 12px;
    cursor: pointer;
    transition: background 150ms ease;
    flex-shrink: 0;
  }

  .toggle:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .toggle.active {
    background: var(--color-success);
  }

  .toggle-slider {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 20px;
    height: 20px;
    background: var(--color-background);
    border-radius: 10px;
    transition: transform 150ms ease;
  }

  .toggle.active .toggle-slider {
    transform: translateX(20px);
  }

  .summary-card {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 10px;
    margin-top: 8px;
  }

  .summary-section-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--color-text);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 8px;
  }

  .summary-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 4px 0;
  }

  .summary-row.recipient-summary {
    padding: 2px 0;
    margin-bottom: 4px;
  }

  .summary-row.recipient-summary .summary-label {
    font-size: 12px;
    color: var(--color-text-tertiary);
  }

  .summary-row.recipient-summary .summary-value {
    font-size: 13px;
  }

  .summary-row.total {
    border-top: 1px solid var(--color-border);
    margin-top: 4px;
    padding-top: 8px;
  }

  .summary-label {
    font-size: 13px;
    color: var(--color-text-secondary);
  }

  .summary-value {
    font-size: 14px;
    color: var(--color-text);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }

  .summary-label-total,
  .summary-value-total {
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text);
  }

  .btn-primary {
    background: var(--color-text);
    color: var(--color-background);
    border: none;
    border-radius: 10px;
    padding: 12px 20px;
    font-size: 15px;
    font-weight: 500;
    cursor: pointer;
    width: 100%;
    transition: all 0.15s ease;
  }

  .btn-primary:hover:not(:disabled) {
    background: #171717;
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .button-footer {
    padding: 10px 14px 14px;
  }

  .error-message {
    background: rgba(239, 68, 68, 0.1);
    color: #ef4444;
    padding: 10px 14px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 500;
    margin-bottom: 10px;
    border: 1px solid rgba(239, 68, 68, 0.2);
  }

  /* Mode Toggle Styles */
  .mode-toggle-section {
    display: flex;
    background: var(--color-surface);
    border: 1.5px solid var(--color-border);
    border-radius: 10px;
    padding: 4px;
    gap: 4px;
  }

  .mode-toggle-btn {
    flex: 1;
    padding: 8px 16px;
    border: none;
    border-radius: 8px;
    background: transparent;
    font-size: 14px;
    font-weight: 500;
    color: var(--color-text-secondary);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .mode-toggle-btn:hover:not(:disabled) {
    color: var(--color-text);
  }

  .mode-toggle-btn.active {
    background: var(--color-background);
    color: var(--color-text);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  }

  .mode-toggle-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* Bridge Styles */
  .bridge-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .bridge-warning {
    display: flex;
    gap: 12px;
    padding: 12px;
    background: rgba(251, 191, 36, 0.1);
    border: 1px solid rgba(251, 191, 36, 0.3);
    border-radius: 10px;
    color: #b45309;
  }

  .bridge-warning svg {
    flex-shrink: 0;
    margin-top: 2px;
    color: #f59e0b;
  }

  .bridge-warning-text {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .bridge-warning-title {
    font-size: 13px;
    font-weight: 600;
    color: #b45309;
  }

  .bridge-warning-desc {
    font-size: 12px;
    line-height: 1.4;
    color: #92400e;
  }

  .bridge-warning-desc strong {
    font-weight: 600;
  }

  .input-error {
    border-color: #ef4444 !important;
  }

  .input-hint {
    font-size: 11px;
    margin-top: 4px;
  }

  .input-hint.error {
    color: #ef4444;
  }

  .amount-hint {
    font-size: 11px;
    color: var(--color-text-tertiary);
  }

  .bridge-address {
    overflow-wrap: anywhere;
    max-width: 240px;
    font-family: monospace;
    font-size: 12px;
  }

  .bridge-btn {
    background: linear-gradient(135deg, #3b82f6, #8b5cf6);
  }

  .bridge-btn:hover:not(:disabled) {
    background: linear-gradient(135deg, #2563eb, #7c3aed);
  }

  .bridge-info-link {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 4px;
    color: var(--color-text-tertiary);
    transition: color 0.15s ease;
    margin-left: 4px;
  }

  .bridge-info-link:hover {
    color: var(--color-text);
  }
</style>
