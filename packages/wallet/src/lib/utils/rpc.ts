import { bridgeFromOutputs, type BridgeDetails } from './bridge';
// Nockchain RPC Client for interacting with the blockchain
import { NETWORK, ERROR_MESSAGES } from '../constants';
import type { components as V0Components } from '../types/nockblocks-api-v0';
import type { components as V1Components } from '../types/nockblocks-api-v1';

export interface RPCError {
  code: number;
  message: string;
  data?: any;
}

export interface RPCResponse<T = any> {
  jsonrpc: string;
  id: string | number;
  result?: T;
  error?: RPCError;
}

export type RPCVersion = 'v0' | 'v1';

export interface RPCClientOptions {
  version?: RPCVersion;
  url?: string;
  retries?: number;
  timeout?: number;
}

// Use generated types from OpenAPI spec
export type Note = V0Components['schemas']['NoteObject'];
export type NotesResponse = V0Components['schemas']['NotesResponse'];
export type AddressTransaction = V0Components['schemas']['AddressTransaction'];
export type TransactionWithBlockInfo = V0Components['schemas']['TransactionWithBlockInfo'];
export type AddressTransactionsResponse = V0Components['schemas']['AddressTransactionsResponse'];

export type NoteV1 = V1Components['schemas']['NoteObjectV1'];
export type TransactionObjectV1 = V1Components['schemas']['TransactionObjectV1'];
export type BlockObjectV1 = V1Components['schemas']['BlockObjectV1'];

// Legacy interface for backward compatibility with wallet UI
// TODO: Gradually migrate UI to use AddressTransaction directly
export interface TransactionData {
  bridge?: BridgeDetails;
  txId: string;
  from?: string;
  to?: string;
  amount: number;
  fee: number;
  timestamp: number;
  status: string;
  blockHeight?: number;
  confirmations?: number;
  notes?: string;
  isCoinbase?: boolean;
  /** Transaction type from the API - 'received', 'sent', or 'self' */
  type?: 'received' | 'sent' | 'self';
}

// Legacy interface - not returned by actual API
// Keeping for backward compatibility but should be replaced
export interface BlockchainMetrics {
  blockHeight: number;
  totalTransactions: number;
  totalAddresses: number;
  networkHashrate: number;
  difficulty: number;
  lastBlockTime: number;
}

export class NockchainRPC {
  private url: string;
  private retries: number;
  private timeout: number;
  private version: RPCVersion;
  private connectionStatus: 'connected' | 'disconnected' | 'connecting' = 'disconnected';

  constructor(options: RPCClientOptions = {}) {
    const {
      version = 'v0',
      url,
      retries = NETWORK.RETRY_ATTEMPTS,
      timeout = NETWORK.DEFAULT_TIMEOUT
    } = options;

    this.version = version;
    this.url = url ?? (version === 'v0' ? NETWORK.RPC_URL : NETWORK.RPC_V1_URL);
    this.retries = retries;
    this.timeout = timeout;

    // Test connection on initialization
    this.testConnection();
  }

  private async testConnection() {
    this.connectionStatus = 'connecting';
    try {
      await this.ping();
      this.connectionStatus = 'connected';
      console.log(`Connected to RPC at ${this.url}`);
    } catch (error) {
      this.connectionStatus = 'disconnected';
      console.warn(`Failed to connect to RPC at ${this.url}:`, error);
    }
  }

  getConnectionStatus() {
    return {
      status: this.connectionStatus,
      url: this.url,
      version: this.version
    };
  }

  private async fetchWithRetry(body: any, attempt = 0): Promise<any> {
    try {
      // Check if we're running in a Chrome extension popup (not just on a webpage with Chrome)
      // We need both chrome.runtime AND chrome-extension:// protocol
      const isExtension =
        typeof chrome !== 'undefined' &&
        typeof chrome.runtime !== 'undefined' &&
        typeof chrome.runtime.sendMessage === 'function' &&
        typeof chrome.runtime.id === 'string' &&
        typeof window !== 'undefined' &&
        window.location.protocol === 'chrome-extension:';

      console.log(
        '[RPC] isExtension:',
        isExtension,
        'protocol:',
        typeof window !== 'undefined' ? window.location.protocol : 'N/A'
      );

      if (isExtension) {
        // Use background service worker to avoid CORS issues
        console.log('Sending message to background worker:', body);
        return await new Promise((resolve, reject) => {
          const timeoutId = setTimeout(() => {
            console.error('Background worker timeout');
            reject(new Error(ERROR_MESSAGES.REQUEST_TIMEOUT));
          }, this.timeout);

          try {
            chrome.runtime.sendMessage(
              {
                type: 'rpcRequest',
                payload: body,
                rpcUrl: this.url
              },
              (response: any) => {
                clearTimeout(timeoutId);
                console.log('Background worker response:', response);

                if (chrome.runtime.lastError) {
                  console.error('chrome.runtime.lastError:', chrome.runtime.lastError);
                  reject(new Error(chrome.runtime.lastError.message));
                  return;
                }

                if (!response) {
                  console.error('No response from background worker');
                  reject(new Error('No response from background worker'));
                  return;
                }

                if (!response.success) {
                  console.error('Background worker error:', response.error);
                  reject(new Error(response.error || ERROR_MESSAGES.NETWORK_ERROR));
                  return;
                }

                const data: RPCResponse = response.data;

                if (data.error) {
                  console.error('RPC error:', data.error);
                  reject(new Error(data.error.message || ERROR_MESSAGES.UNKNOWN_ERROR));
                  return;
                }

                console.log('RPC success:', data.result);
                resolve(data.result);
              }
            );
          } catch (error) {
            console.error('sendMessage exception:', error);
            clearTimeout(timeoutId);
            reject(error);
          }
        });
      } else {
        // Development mode: use direct fetch with auth headers
        const controller = new AbortController();
        const timeoutId = setTimeout(() => controller.abort(), this.timeout);

        try {
          const bodyStr = JSON.stringify(body);
          const response = await (
            await import('../../platform/rpcAuth')
          ).fetchAuthenticatedRpc(this.url, bodyStr, controller.signal);
          if (!response.ok) throw new Error(`HTTP ${response.status}: ${await response.text()}`);
          const data: RPCResponse = await response.json();
          if (data.error) throw new Error(data.error.message || ERROR_MESSAGES.UNKNOWN_ERROR);
          return data.result;
        } finally {
          clearTimeout(timeoutId);
        }
      }
    } catch (error) {
      // Handle network errors and retries
      if (body.method !== 'submitTransaction' && attempt < this.retries - 1) {
        // Exponential backoff: wait 1s, 2s, 4s...
        await new Promise(resolve => setTimeout(resolve, 1000 * Math.pow(2, attempt)));
        return this.fetchWithRetry(body, attempt + 1);
      }

      if (error instanceof Error) {
        if (error.name === 'AbortError') {
          throw new Error(ERROR_MESSAGES.REQUEST_TIMEOUT);
        }
        throw error;
      }

      throw new Error(ERROR_MESSAGES.NETWORK_ERROR);
    }
  }

  private ensureV0Support(methodName: string) {
    if (this.version !== 'v0') {
      throw new Error(
        `${methodName} is only supported by RPC v0. Use rpc.request() directly for RPC v1 endpoints.`
      );
    }
  }

  async request<T = any>(method: string, params: any[] = []): Promise<T> {
    const body = {
      jsonrpc: '2.0',
      method,
      params: params.length === 1 && typeof params[0] === 'object' ? [params[0]] : params,
      id: Math.random().toString(36).substr(2, 9)
    };

    return this.fetchWithRetry(body);
  }

  // Core RPC Methods

  async getBalance(address: string): Promise<number> {
    this.ensureV0Support('getBalance');
    const result = await this.request<{ nicks: number }>('getNotes', [{ address }]);
    const nicks = result.nicks;
    if (!Number.isSafeInteger(nicks) || nicks < 0) throw new Error('Invalid balance response');
    return nicks;
  }

  async getTransactions(
    address: string,
    options: {
      minHeight?: number;
      maxHeight?: number;
      limit?: number;
      offset?: number;
    } = {}
  ): Promise<TransactionData[]> {
    this.ensureV0Support('getTransactions');
    const params = {
      address,
      ...options
    };

    const result = await this.request<AddressTransactionsResponse>('getTransactionsByAddress', [
      params
    ]);

    const ownedFirstNamesRaw = (result as any)?.ownedFirstNames;
    const ownedFirstNameSet = new Set<string>(
      Array.isArray(ownedFirstNamesRaw)
        ? ownedFirstNamesRaw.filter((value): value is string => typeof value === 'string')
        : []
    );

    const getNoteFirstName = (note: any): string | undefined => {
      const name = note?.name;
      if (!name || typeof name !== 'object') return undefined;
      const firstName = (name as any).firstName ?? (name as any).first_name;
      return typeof firstName === 'string' ? firstName : undefined;
    };

    const getNoteAddressHint = (note: any): string | undefined => {
      const firstName = getNoteFirstName(note);
      if (firstName) return firstName;

      const pubkeys = note?.lock?.pubkeys;
      if (Array.isArray(pubkeys) && typeof pubkeys[0] === 'string') return pubkeys[0];

      const noteDataLock = note?.noteData?.lock;
      if (noteDataLock && typeof noteDataLock === 'object') {
        const noteDataPubkeys = (noteDataLock as any).pubkeys;
        if (Array.isArray(noteDataPubkeys) && typeof noteDataPubkeys[0] === 'string') {
          return noteDataPubkeys[0];
        }
      }

      return undefined;
    };

    const isOwnedNote = (note: any): boolean => {
      const firstName = getNoteFirstName(note);
      if (firstName && ownedFirstNameSet.has(firstName)) return true;

      const pubkeys = note?.lock?.pubkeys;
      if (Array.isArray(pubkeys) && pubkeys.some((pk: unknown) => pk === address)) return true;

      const noteDataLockPubkeys = note?.noteData?.lock?.pubkeys;
      if (
        Array.isArray(noteDataLockPubkeys) &&
        noteDataLockPubkeys.some((pk: unknown) => pk === address)
      ) {
        return true;
      }

      return false;
    };

    // Map the API response (AddressTransaction) to legacy TransactionData format
    // for backward compatibility with existing wallet UI
    return (result.transactions || []).map((tx: AddressTransaction) => {
      // Check if this is a coinbase transaction by looking at outputs
      const isCoinbase =
        tx.outputs?.some(
          output =>
            (output as any)?.note?.isCoinbase ||
            (output as any)?.seeds?.some((seed: any) => seed?.outputSource?.isCoinbase)
        ) || false;

      // Determine 'from' and 'to' based on transaction type
      const counterparty = tx.counterparties?.[0];

      let correctedType = tx.type;

      let hasOwnedInput = false;
      let hasOwnedOutput = false;
      let ownedOutputSum = 0;
      let foreignOutputSum = 0;
      const foreignInputHints = new Set<string>();
      const foreignOutputHints = new Set<string>();

      for (const input of tx.inputs ?? []) {
        const note = (input as any)?.note;
        if (isOwnedNote(note)) {
          hasOwnedInput = true;
        } else {
          const hint = getNoteAddressHint(note);
          if (hint) foreignInputHints.add(hint);
        }
      }

      for (const output of tx.outputs ?? []) {
        const note = (output as any)?.note;
        const assets = typeof note?.assets === 'number' ? note.assets : 0;
        if (isOwnedNote(note)) {
          hasOwnedOutput = true;
          ownedOutputSum += assets;
        } else {
          foreignOutputSum += assets;
          const hint = getNoteAddressHint(note);
          if (hint) foreignOutputHints.add(hint);
        }
      }

      if (hasOwnedInput) {
        correctedType = foreignOutputSum > 0 ? 'sent' : 'self';
      } else if (hasOwnedOutput) {
        correctedType = 'received';
      } else if (tx.type === 'self') {
        if (counterparty && counterparty !== address) {
          // Correct API type if it returns 'self' but counterparty differs from user's address
          // This works around an API bug where transactions are incorrectly classified as 'self'
          const counterpartyInInputs = tx.inputs?.some(input =>
            (input as any)?.note?.lock?.pubkeys?.some((pk: unknown) => pk === counterparty)
          );
          correctedType = counterpartyInInputs ? 'received' : 'sent';
        } else if (!counterparty || (tx.counterparties?.length ?? 0) === 0) {
          // Counterparties list is empty; compare input vs output names to detect different recipients.
          const inputNames = new Set<string>();
          for (const input of tx.inputs ?? []) {
            const name = getNoteFirstName((input as any)?.note);
            if (name) inputNames.add(name);
          }

          const hasDifferentRecipient = (tx.outputs ?? []).some(output => {
            const name = getNoteFirstName((output as any)?.note);
            return Boolean(name && !inputNames.has(name));
          });

          if (hasDifferentRecipient) {
            correctedType = 'sent';
          }
        }
      }

      const isSelfTransfer = correctedType === 'self';
      const isReceived = correctedType === 'received';
      const isSent = correctedType === 'sent';

      // For self-transfers, both from and to are the user's address
      // For received, from is counterparty (or Coinbase), to is user's address
      // For sent, from is user's address, to is counterparty
      let from: string | undefined;
      let to: string | undefined;

      // Try to find recipient from outputs when counterparty is missing
      let recipientFromOutputs: string | undefined;
      if (isSent && !counterparty && tx.outputs) {
        const inputNames = new Set<string>();
        for (const input of tx.inputs ?? []) {
          const name = getNoteFirstName((input as any)?.note);
          if (name) inputNames.add(name);
        }

        const recipientOutput = (tx.outputs ?? []).find(output => {
          const name = getNoteFirstName((output as any)?.note);
          return Boolean(name && !inputNames.has(name));
        });

        if (recipientOutput) {
          recipientFromOutputs = getNoteAddressHint((recipientOutput as any)?.note);
        }
      }

      if (isSelfTransfer) {
        from = address;
        to = address;
      } else if (isReceived) {
        const derivedFrom =
          foreignInputHints.size === 1
            ? Array.from(foreignInputHints)[0]
            : foreignInputHints.size > 1
              ? 'Multiple'
              : undefined;
        from = isCoinbase ? 'Coinbase' : counterparty || derivedFrom;
        to = address;
      } else if (isSent) {
        const derivedTo =
          foreignOutputHints.size === 1
            ? Array.from(foreignOutputHints)[0]
            : foreignOutputHints.size > 1
              ? 'Multiple'
              : undefined;
        from = address;
        to = counterparty || derivedTo || recipientFromOutputs;
      } else {
        // Fallback: if type is unknown, default to received if we have counterparty
        from = counterparty || address;
        to = address;
      }

      // Calculate amount from inputs/outputs if API returns 0
      let amount = tx.amount;
      if (amount === 0) {
        const canInferAmount = ownedFirstNameSet.size > 0 || hasOwnedInput || hasOwnedOutput;
        if (canInferAmount) {
          if (isSent) amount = foreignOutputSum;
          else if (isReceived) amount = ownedOutputSum;
        } else if (tx.outputs && tx.outputs.length > 0) {
          const inputNames = new Set<string>();
          for (const input of tx.inputs ?? []) {
            const name = getNoteFirstName((input as any)?.note);
            if (name) inputNames.add(name);
          }

          if (isSent) {
            amount = tx.outputs.reduce((sum, output) => {
              const note = (output as any)?.note;
              const name = getNoteFirstName(note);
              const assets = typeof note?.assets === 'number' ? note.assets : 0;
              if (name && !inputNames.has(name)) {
                return sum + assets;
              }
              return sum;
            }, 0);
          } else if (isReceived) {
            amount = tx.outputs.reduce((sum, output) => {
              const note = (output as any)?.note;
              const assets = typeof note?.assets === 'number' ? note.assets : 0;
              return sum + assets;
            }, 0);
            if (tx.fee) {
              amount = Math.max(0, amount - tx.fee);
            }
          } else {
            amount = tx.outputs.reduce((sum, output) => {
              const note = (output as any)?.note;
              const assets = typeof note?.assets === 'number' ? note.assets : 0;
              return sum + assets;
            }, 0);
          }
        }
      }

      const bridge = isSent ? bridgeFromOutputs(tx.outputs) : undefined;
      return {
        bridge,
        txId: tx.txId,
        from,
        to: bridge?.destination ?? to,
        amount,
        fee: tx.fee,
        timestamp: tx.timestamp * 1000, // Convert from seconds to milliseconds
        status: 'confirmed', // All transactions in blocks are confirmed
        blockHeight: tx.blockHeight,
        confirmations: undefined, // Not provided by API
        notes: tx.note || undefined,
        isCoinbase,
        type: correctedType // Use corrected type (fixes API bug with incorrect 'self' classification)
      };
    });
  }

  async getNotes(address: string): Promise<Note[]> {
    this.ensureV0Support('getNotes');
    const result = await this.request<NotesResponse>('getNotes', [{ address }]);
    // Return notes as-is from API - they have correct structure
    // Note: The API returns NoteObject[] which has correct fields:
    // - name: { firstName, lastName }
    // - assets: number
    // - lock: { m, pubkeys }
    // - originPage: number (block height)
    // - sourceHash: string
    // - isCoinbase: boolean
    // - version: number
    return result.notes || [];
  }

  // V1 API: Get notes by address with proper noteData.lock format
  async getTipHeight(): Promise<number> {
    const tip = await this.request<BlockObjectV1>('getTip', [{ includeTransactions: false }]);
    if (!Number.isSafeInteger(tip?.height) || tip.height < 0) throw new Error('Invalid chain tip');
    return tip.height;
  }

  async getNotesByAddress(address: string): Promise<NoteV1[]> {
    if (this.version !== 'v1') {
      throw new Error('getNotesByAddress requires RPC v1. Use getRPCClientV1().');
    }
    console.log('[RPC V1] Fetching notes for address:', address);
    const result = await this.request<NoteV1[]>('getNotesByAddress', [{ address }]);
    console.log('[RPC V1] getNotesByAddress response:', JSON.stringify(result));
    return result || [];
  }

  async submitTransaction(rawTransaction: string): Promise<string> {
    if (['mobile', 'desktop', 'urbit'].includes(import.meta.env.MODE))
      return (await import('../../platform/submission')).submitIrisV1Transaction(rawTransaction);
    return this.request<string>('submitTransaction', [{ rawTransaction }]);
  }

  async getTransactionSubmission(
    txId: string
  ): Promise<{ txId: string; status: string; tracked: boolean; checkFailed?: boolean }> {
    return getRPCClientV1().request('getTransactionSubmission', [{ txId }]);
  }

  async isTransactionInMempool(txId: string): Promise<boolean> {
    try {
      const result = await getRPCClientV1().request<{ transaction: { id: string } }>(
        'getMempoolTransactionByTxid', [{ transactionId: txId }]
      );
      if (result?.transaction?.id !== txId) throw new Error('Invalid mempool response');
      return true;
    } catch (error) {
      if (error instanceof Error && error.message === 'Transaction not found in mempool') return false;
      throw error;
    }
  }

  async getBlockchainMetrics(): Promise<BlockchainMetrics> {
    this.ensureV0Support('getBlockchainMetrics');
    const result = await this.request<BlockchainMetrics>('getBlockchainMetrics');

    return {
      blockHeight: result.blockHeight || 0,
      totalTransactions: result.totalTransactions || 0,
      totalAddresses: result.totalAddresses || 0,
      networkHashrate: result.networkHashrate || 0,
      difficulty: result.difficulty || 0,
      lastBlockTime: result.lastBlockTime || Date.now()
    };
  }

  async getBlockByHeight(height: number): Promise<any> {
    this.ensureV0Support('getBlockByHeight');
    return this.request('getBlockByHeight', [{ height }]);
  }

  async getTransaction(txId: string): Promise<TransactionData> {
    this.ensureV0Support('getTransaction');
    const result = await this.request<any>('getTransactionById', [{ id: txId }]);

    return {
      txId: result.txId || txId,
      from: result.from || result.sender,
      to: result.to || result.recipient,
      amount: result.amount || 0,
      fee: result.fee || 0,
      timestamp: result.timestamp ? result.timestamp * 1000 : Date.now(), // Convert from seconds to milliseconds
      status: result.status || 'pending',
      blockHeight: result.blockHeight,
      confirmations: result.confirmations || 0,
      notes: result.notes || result.memo
    };
  }

  // Utility method to check if the RPC is accessible
  async ping(): Promise<boolean> {
    try {
      if (this.version === 'v1') {
        const tip = await this.request<BlockObjectV1>('getTip', [{ includeTransactions: false }]);
        return Boolean(tip);
      }

      const metrics = await this.getBlockchainMetrics();
      return metrics.blockHeight > 0;
    } catch {
      return false;
    }
  }
}

// Singleton instances for the app
let rpcInstance: NockchainRPC | null = null;
let rpcV1Instance: NockchainRPC | null = null;

export function getRPCClient(): NockchainRPC {
  if (!rpcInstance) {
    rpcInstance = new NockchainRPC();
  }
  return rpcInstance;
}

export function getRPCClientV1(): NockchainRPC {
  if (!rpcV1Instance) {
    rpcV1Instance = new NockchainRPC({ version: 'v1' });
  }
  return rpcV1Instance;
}

// Export default instance
export default getRPCClient();
