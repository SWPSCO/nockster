import { pendingStatus, pendingWithoutConfirmed } from '../lib/utils/pendingStatus';
import { nextWalletName } from '../lib/utils/walletName';
import {
  BRIDGE_LOCK_ROOT,
  bridgeDetails,
  verifyBridgeTransaction,
  type BridgeRecipient
} from '../lib/utils/bridge';
import { priceStore, nockPrice } from '../lib/stores/price';
import { formatUsdEstimate } from '../lib/utils/usd';
import {
  getAddressBook,
  saveAddressAlias,
  deleteAddressAlias,
  type AddressAlias
} from '../lib/utils/addressBook';
import { clearRpcAuth } from './rpcAuth';
import { get } from 'svelte/store';
import * as vault from '../vaultController';
import { handleVaultMessage } from '../vault/engine';
import { walletStore } from '../lib/stores/wallet';
import { createSignedTransaction, getVaultNickname } from '../lib/utils/vaultBridge';
import { collectReservedNoteIds, noteNameToId } from '../lib/utils/noteIds';
import {
  parseNocksInput,
  parseNicksInput,
  formatNocksWithSeparator,
  formatNicksWithSeparator,
  toNicks
} from '../lib/utils/nicks';
import type { Wallet } from '../lib/types/wallet';
import { submissionStatusLabel } from '../lib/utils/submissionStatus';
import { parseRecipientAddress } from '../lib/utils/recipientAddress';
import { VanitySearch, initialVanityProgress, type VanityOptions } from '../lib/services/vanity';

type Request = {
  action: string;
  vanity?: VanityOptions;
  searchId?: string;
  password?: string;
  name?: string;
  key?: string;
  walletId?: string;
  confirmation?: string;
  backupConfirmed?: boolean;
  address?: string;
  contactId?: string;
  recipients?: Array<{ address: string; amount: string }>;
  bridge?: { destination: string; amount: string };
  amountUnit?: 'nock' | 'nicks';
  privateOutputs?: boolean;
  previewId?: string;
  txId?: string;
};

type PreparedPayment = {
  id: string;
  walletId: string;
  expires: number;
  recipients: BridgeRecipient[];
  privateOutputs: boolean;
  transaction: { signedTx: string; feePaid: number; txId: string; inputNotes: string[] };
};

const vanitySearch = new VanitySearch();
let vanitySearchId: string | null = null;
let contacts: AddressAlias[] = [];
let prepared: PreparedPayment | null = null;
let generation = 0;
let queue: Promise<unknown> = Promise.resolve();
let networkError: string | null = null;
const loaded = walletStore.loadFromStorage();

function amount(nicks: number): string {
  if (!Number.isSafeInteger(nicks) || nicks < 0) throw new Error('Invalid balance');
  const value = BigInt(nicks);
  const whole = value / 65536n;
  const fraction = (((value % 65536n) * 10n ** 16n) / 65536n)
    .toString()
    .padStart(16, '0')
    .replace(/0+$/, '');
  return `${whole}${fraction ? `.${fraction}` : ''}`;
}

async function synchronize() {
  const summaries = await vault.getWallets();
  const stored = get(walletStore);
  const activeAddress = stored.activeWallet?.addresses[0];
  const wallets: Wallet[] = summaries.map(summary => {
    const previous = stored.wallets.find(
      wallet =>
        wallet.id === `vault-${summary.nickname}` || wallet.addresses[0] === summary.publicKey
    );
    return {
      ...previous,
      id: `vault-${summary.nickname}`,
      name: summary.nickname,
      addresses: [summary.publicKey],
      masterPublicKey: summary.extendedPublicKey ?? undefined,
      currentAddressIndex: 0,
      createdAt: previous?.createdAt ?? Date.now(),
      lastUsed: Date.now()
    };
  });
  walletStore.setWallets(wallets);
  const selected = wallets.find(wallet => wallet.addresses[0] === activeAddress);
  if (selected) walletStore.selectWallet(selected.id);
  if (!get(walletStore).activeWallet && wallets.length) walletStore.selectWallet(wallets[0].id);
  walletStore.unlock();
  await walletStore.saveToStorage();
}

async function snapshot() {
  const status = await vault.vaultStatus();
  const state = get(walletStore);
  const unlocked = status.unlocked && !state.isLocked;
  const active = unlocked ? state.activeWallet : null;
  const pending = pendingWithoutConfirmed(active?.pendingTransactions, active?.transactions);
  const pendingIds = new Set(pending.map(tx => tx.txId));
  const parties = (addresses: Array<string | undefined>) =>
    [...new Set(addresses.filter((address): address is string => Boolean(address)))].map(
      address => ({
        address,
        label: contacts.find(contact => contact.address === address)?.alias ?? null
      })
    );
  return {
    ...status,
    unlocked,
    suggestedWalletName: nextWalletName(unlocked ? state.wallets.map(wallet => wallet.name) : []),
    activeId: active?.id ?? null,
    usdPerNock: get(nockPrice) || null,
    wallets: unlocked
      ? state.wallets.map(wallet => ({
          id: wallet.id,
          name: wallet.name,
          address: wallet.addresses[0],
          balance:
            wallet.balance === undefined
              ? null
              : formatNocksWithSeparator(toNicks(wallet.balance), 2),
          balanceUsd:
            wallet.balance === undefined
              ? null
              : formatUsdEstimate(wallet.balance / 65536, get(nockPrice)),
          balanceNicks:
            wallet.balance === undefined ? null : formatNicksWithSeparator(toNicks(wallet.balance))
        }))
      : [],
    history: [
      ...pending.map(tx => ({
        bridge: tx.bridge
          ? {
              destination: tx.bridge.destination,
              amount: amount(tx.bridge.amount),
              protocolFee: amount(tx.bridge.protocolFee),
              expectedReceived: amount(tx.bridge.expectedReceived)
            }
          : null,
        id: tx.txId,
        amount: amount(tx.totalAmount),
        amountUsd: formatUsdEstimate(tx.totalAmount / 65536, get(nockPrice)),
        feeUsd: formatUsdEstimate(tx.fee / 65536, get(nockPrice)),
        fee: amount(tx.fee),
        timestamp: tx.timestamp,
        status: tx.submissionStatus ?? 'pending',
        statusLabel: pendingStatus(tx).label,
        pendingDetail: pendingStatus(tx).detail,
        canClear: true,
        direction: 'sent',
        parties: parties(
          tx.bridge ? [tx.bridge.destination] : tx.recipients.map(recipient => recipient.address)
        ),
        from: tx.fromAddress,
        blockHeight: null,
        error: tx.lastSubmitError ?? null
      })),
      ...(active?.transactions ?? [])
        .filter(tx => !pendingIds.has(tx.txId))
        .map(tx => ({
          bridge: tx.bridge
            ? {
                destination: tx.bridge.destination,
                amount: amount(tx.bridge.amount),
                protocolFee: amount(tx.bridge.protocolFee),
                expectedReceived: amount(tx.bridge.expectedReceived)
              }
            : null,
          id: tx.txId,
          amount: amount(tx.amount),
          amountUsd: formatUsdEstimate(tx.amount / 65536, get(nockPrice)),
          feeUsd: formatUsdEstimate((tx.fee ?? 0) / 65536, get(nockPrice)),
          fee: amount(tx.fee ?? 0),
          timestamp: tx.timestamp,
          status: tx.status,
          statusLabel: submissionStatusLabel(tx.status),
          pendingDetail: null,
          canClear: false,
          direction: tx.type ?? 'received',
          parties: parties([tx.type === 'sent' ? tx.to : tx.from]),
          from: tx.from,
          blockHeight: tx.blockHeight ?? null,
          error: null
        }))
    ].sort((a, b) => b.timestamp - a.timestamp),
    contacts: unlocked ? contacts : [],
    networkError: unlocked ? networkError : null
  };
}

async function refresh() {
  const refreshGeneration = generation;
  const { activeWallet: active, wallets } = get(walletStore);
  if (!active) return;
  const tasks = [
    { label: 'Price', run: () => priceStore.refresh().catch(() => undefined) },
    ...wallets.map(wallet => ({
      label: wallets.length > 1 ? `${wallet.name} balance` : 'Balance',
      run: () => walletStore.fetchBalance(wallet.id, true)
    })),
    { label: 'Transaction history', run: () => walletStore.fetchTransactions(active.id, true) },
    { label: 'Submission status', run: () => walletStore.refreshSubmissionStatus(active.id) },
    {
      label: 'Address book',
      run: async () => {
        const entries = await getAddressBook();
        if (refreshGeneration === generation) contacts = entries;
      }
    }
  ];
  // Spendable notes are fetched when preparing a payment, not to display a balance.
  const results = await Promise.allSettled(tasks.map(task => task.run()));
  if (refreshGeneration !== generation) return;
  networkError =
    results
      .flatMap((result, index) => {
        if (result.status !== 'rejected') return [];
        const message =
          result.reason instanceof Error ? result.reason.message : String(result.reason);
        return [`${tasks[index].label} could not update: ${message}`];
      })
      .join('\n') || null;
}

async function requireUnlocked() {
  const status = await vault.vaultStatus();
  if (!status.unlocked || get(walletStore).isLocked) throw new Error('Unlock your wallet first');
}

async function suspend() {
  generation++;
  vanitySearch.clear();
  vanitySearchId = null;
  clearRpcAuth();
  contacts = [];
  prepared = null;
  networkError = null;
  walletStore.lock();
  const result = await handleVaultMessage({ type: 'vault:suspend' });
  if (!result.success) throw new Error(result.error);
}

async function perform(
  request: Request,
  requestedGeneration: number
): Promise<Record<string, unknown>> {
  await loaded;
  if (requestedGeneration !== generation) throw new Error('Wallet session ended');
  const epoch = generation;
  const result: Record<string, unknown> = {};
  switch (request.action) {
    case 'status':
      break;
    case 'scanAddress':
      await requireUnlocked();
      result.address = parseRecipientAddress(request.key ?? '');
      break;
    case 'deviceUnlockKey':
      await requireUnlocked();
      result.unlockKey = await vault.exportDeviceUnlockKey(request.password ?? '');
      break;
    case 'unlockWithDeviceKey':
      await vault.unlockWithDeviceKey(request.key ?? '');
      await synchronize();
      break;
    case 'vanityStart': {
      const status = await vault.vaultStatus();
      if (status.exists) await requireUnlocked();
      if (!request.vanity) throw new Error('Choose your address prefix.');
      if (!request.searchId || request.searchId.length > 100)
        throw new Error('Start a new search.');
      vanitySearch.start(request.vanity);
      vanitySearchId = request.searchId;
      result.vanity = vanitySearch.snapshot();
      break;
    }
    case 'vanityStatus':
      result.vanity =
        request.searchId === vanitySearchId ? vanitySearch.snapshot() : initialVanityProgress();
      break;
    case 'vanityStop':
      if (request.searchId === vanitySearchId) {
        vanitySearch.stop();
        vanitySearchId = null;
      }
      result.vanity = vanitySearch.snapshot();
      break;
    case 'vanityTake': {
      if (request.searchId !== vanitySearchId) throw new Error('This search has ended.');
      vanitySearchId = null;
      const candidate = vanitySearch.take();
      try {
        const address = await vault.validateWalletKey(candidate.key);
        if (address !== candidate.address)
          throw new Error('The recovered wallet does not match the mined address.');
        result.candidate = { ...candidate };
      } finally {
        candidate.key = '';
      }
      break;
    }
    case 'generate':
      vanitySearch.clear();
      vanitySearchId = null;
      result.mnemonic = await vault.generateKey();
      break;
    case 'unlock':
      await vault.unlockVault(request.password ?? '');
      await synchronize();
      break;
    case 'import': {
      await vault.validateWalletKey(request.key?.trim() ?? '');
      vanitySearch.clear();
      vanitySearchId = null;
      const status = await vault.vaultStatus();
      if (!status.exists) {
        if ((request.password?.length ?? 0) < 12) throw new Error('Use at least 12 characters');
        await vault.newVault(request.password!);
      } else await requireUnlocked();
      const wallets = await vault.getWallets();
      const name = request.name?.trim() || nextWalletName(wallets.map(wallet => wallet.nickname));
      if (wallets.some(wallet => wallet.nickname === name)) {
        throw new Error(
          `A wallet named “${name}” already exists on this device. Choose a different wallet name.`
        );
      }
      const imported = await vault.importWallet(name, request.key?.trim() ?? '');
      await synchronize();
      prepared = null;
      clearRpcAuth();
      contacts = [];
      walletStore.selectWallet(`vault-${imported.nickname}`);
      await walletStore.saveToStorage();
      await refresh();
      break;
    }
    case 'select':
      await requireUnlocked();
      if (!get(walletStore).wallets.some(wallet => wallet.id === request.walletId))
        throw new Error('Wallet not found');
      prepared = null;
      clearRpcAuth();
      contacts = [];
      networkError = null;
      walletStore.selectWallet(request.walletId!);
      break;
    case 'clearPending': {
      await requireUnlocked();
      const active = get(walletStore).activeWallet;
      if (
        !active ||
        request.walletId !== active.id ||
        !request.txId ||
        request.confirmation !== request.txId
      )
        throw new Error('Confirm clearing this transaction in the selected wallet');
      if (
        !pendingWithoutConfirmed(active.pendingTransactions, active.transactions).some(
          tx => tx.txId === request.txId
        )
      )
        throw new Error('Transaction is no longer pending');
      prepared = null;
      walletStore.deletePendingTransaction(active.id, request.txId);
      await walletStore.saveToStorage();
      break;
    }
    case 'refresh':
      await requireUnlocked();
      await refresh();
      break;
    case 'contacts':
      await requireUnlocked();
      contacts = await getAddressBook();
      break;
    case 'saveContact':
      await requireUnlocked();
      await saveAddressAlias(request.address ?? '', request.name ?? '');
      contacts = await getAddressBook();
      break;
    case 'deleteContact':
      await requireUnlocked();
      await deleteAddressAlias(request.contactId ?? '');
      contacts = await getAddressBook();
      break;
    case 'rename': {
      await requireUnlocked();
      const state = get(walletStore);
      const wallet = state.wallets.find(
        wallet => wallet.id === (request.walletId ?? state.activeWallet?.id)
      );
      if (!wallet) throw new Error('Wallet not found');
      const name = request.name?.trim() ?? '';
      if (!name) throw new Error('Enter a wallet name');
      if (state.wallets.some(other => other.id !== wallet.id && other.name === name))
        throw new Error('A wallet with that name already exists. Choose a different wallet name.');
      if (wallet.name !== name) {
        await vault.renameWallet(wallet.name, name);
        walletStore.renameWallet(wallet.id, name);
        prepared = null;
        await synchronize();
      }
      break;
    }
    case 'deleteWallet': {
      await requireUnlocked();
      const state = get(walletStore);
      const wallet = state.wallets.find(wallet => wallet.id === request.walletId);
      if (!wallet) throw new Error('Wallet not found');
      if (state.wallets.length <= 1)
        throw new Error('Add another wallet before deleting your only wallet.');
      if (request.confirmation !== wallet.name || request.backupConfirmed !== true)
        throw new Error('Confirm your recovery backup and type the wallet name to delete it.');
      await vault.deleteWallet(wallet.name);
      prepared = null;
      if (wallet.id === state.activeWallet?.id) {
        clearRpcAuth();
        contacts = [];
        networkError = null;
      }
      await synchronize();
      await refresh();
      break;
    }
    case 'convertAmounts': {
      await requireUnlocked();
      if (request.amountUnit !== 'nock' && request.amountUnit !== 'nicks')
        throw new Error('Choose NOCK or nicks');
      result.amounts = (request.recipients ?? []).map(recipient => {
        if (!recipient.amount.trim()) return '';
        const value =
          request.amountUnit === 'nicks'
            ? parseNicksInput(recipient.amount)
            : parseNocksInput(recipient.amount);
        if (value === null) throw new Error('Enter a valid amount before switching units');
        return request.amountUnit === 'nicks' ? amount(Number(value)) : value.toString();
      });
      break;
    }
    case 'prepare': {
      await requireUnlocked();
      prepared = null;
      const wallet = get(walletStore).activeWallet;
      if (!wallet) throw new Error('Select a wallet');
      const bridge = request.bridge
        ? bridgeDetails(request.bridge.destination, parseNocksInput(request.bridge.amount) ?? 0n)
        : undefined;
      if (bridge && request.privateOutputs)
        throw new Error('Bridge deposits require public outputs');
      const recipients: BridgeRecipient[] = bridge
        ? [
            {
              address: BRIDGE_LOCK_ROOT,
              amount: bridge.amount,
              bridgeEvmAddress: bridge.destination
            }
          ]
        : (request.recipients ?? []).map(recipient => {
            if (request.amountUnit !== undefined && !['nock', 'nicks'].includes(request.amountUnit))
              throw new Error('Choose NOCK or nicks');
            const value =
              request.amountUnit === 'nicks'
                ? parseNicksInput(recipient.amount)
                : parseNocksInput(recipient.amount);
            if (!value || !recipient.address.trim())
              throw new Error('Enter a valid address and amount');
            return { address: recipient.address.trim(), amount: Number(value) };
          });
      if (!recipients.length || recipients.length > 16)
        throw new Error('Use between 1 and 16 recipients');
      await walletStore.fetchNotes(wallet.id, true);
      await requireUnlocked();
      if (epoch !== generation) throw new Error('Wallet session ended');
      const current = get(walletStore).activeWallet!;
      const reserved = collectReservedNoteIds(current.pendingTransactions);
      const notes = (current.notes ?? []).filter(note => !reserved.has(noteNameToId(note) ?? ''));
      const signed = await createSignedTransaction(
        getVaultNickname(current),
        notes as unknown as Array<Record<string, unknown>>,
        recipients.map(recipient => ({
          address: recipient.address,
          gift: recipient.amount,
          bridgeEvmAddress: recipient.bridgeEvmAddress
        })),
        current.addresses[0],
        { privateOutputs: Boolean(request.privateOutputs) }
      );
      if (
        !signed.success ||
        !signed.signedTx ||
        !signed.txId ||
        !signed.inputNotes ||
        signed.feePaid === undefined
      ) {
        throw new Error(signed.error ?? 'Unable to prepare payment');
      }
      await verifyBridgeTransaction(
        signed.signedTx,
        current.addresses[0],
        recipients,
        signed.feePaid
      );
      prepared = {
        id: crypto.randomUUID(),
        walletId: current.id,
        expires: Date.now() + 120_000,
        recipients,
        privateOutputs: Boolean(request.privateOutputs),
        transaction: {
          signedTx: signed.signedTx,
          feePaid: signed.feePaid,
          txId: signed.txId,
          inputNotes: signed.inputNotes
        }
      };
      const netSent = recipients.reduce((sum, recipient) => sum + recipient.amount, 0);
      const price = get(nockPrice);
      result.preview = {
        bridge: bridge
          ? {
              destination: bridge.destination,
              amount: amount(bridge.amount),
              protocolFee: amount(bridge.protocolFee),
              expectedReceived: amount(bridge.expectedReceived)
            }
          : null,
        netSent: amount(netSent),
        netSentNicks: formatNicksWithSeparator(toNicks(netSent)),
        netSentUsd: formatUsdEstimate(netSent / 65536, price),
        feeUsd: formatUsdEstimate(signed.feePaid / 65536, price),
        totalUsd: formatUsdEstimate((netSent + signed.feePaid) / 65536, price),
        id: prepared.id,
        recipients: recipients.map(recipient => ({
          address: recipient.address,
          amount: amount(recipient.amount),
          amountUsd: formatUsdEstimate(recipient.amount / 65536, price),
          amountNicks: formatNicksWithSeparator(toNicks(recipient.amount))
        })),
        fee: amount(signed.feePaid),
        feeNicks: formatNicksWithSeparator(toNicks(signed.feePaid)),
        totalNicks: formatNicksWithSeparator(
          toNicks(recipients.reduce((sum, recipient) => sum + recipient.amount, signed.feePaid))
        ),
        total: amount(
          recipients.reduce((sum, recipient) => sum + recipient.amount, signed.feePaid)
        ),
        privateOutputs: prepared.privateOutputs
      };
      break;
    }
    case 'send': {
      await requireUnlocked();
      const payment = prepared;
      if (
        !payment ||
        payment.id !== request.previewId ||
        payment.expires < Date.now() ||
        payment.walletId !== get(walletStore).activeWallet?.id
      )
        throw new Error('Review the payment again');
      await confirmSubmission(request);
      if (epoch !== generation || prepared !== payment || payment.expires < Date.now())
        throw new Error('Review the payment again');
      prepared = null;
      const sent = await walletStore.sendTransaction(
        payment.recipients,
        'standard',
        payment.transaction,
        { privateOutputs: payment.privateOutputs }
      );
      result.txId = sent.txId;
      break;
    }
    case 'retry': {
      await requireUnlocked();
      const active = get(walletStore).activeWallet;
      if (!active || !request.txId) throw new Error('Select a pending transaction');
      await confirmSubmission(request);
      if (epoch !== generation) throw new Error('Wallet session ended');
      await walletStore.resubmitPendingTransaction(active.id, request.txId);
      break;
    }
    default:
      throw new Error('Unknown wallet action');
  }
  if (epoch !== generation) {
    await suspend();
    throw new Error('Wallet session ended');
  }
  return { ...result, state: await snapshot() };
}

async function confirmSubmission(request: Request) {
  if (request.key) {
    const checked = await handleVaultMessage({ type: 'vault:checkDeviceKey', key: request.key });
    if (!checked.success) throw new Error('Device authentication failed');
  } else {
    if (!request.password)
      throw new Error('Confirm this payment with your wallet password or device unlock');
    if (!(await vault.checkPassword(request.password)))
      throw new Error('Incorrect wallet password');
  }
}

async function dispatch(request: Request): Promise<string> {
  try {
    if (request.action === 'lock') {
      await suspend();
      return JSON.stringify({ state: await snapshot() });
    }
    const requestedGeneration = generation;
    const operation = queue.then(() => perform(request, requestedGeneration));
    queue = operation.catch(() => undefined);
    return JSON.stringify(await operation);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return JSON.stringify({
      error: message === 'failed to unwrap VMK' ? 'Incorrect wallet password' : message,
      state: await snapshot()
    });
  }
}

declare global {
  interface Window {
    nocksterNative: { dispatch: typeof dispatch };
  }
}
window.nocksterNative = { dispatch };
