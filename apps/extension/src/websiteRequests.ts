import { handleVaultMessage } from '../../../packages/wallet/src/vault/engine';
import type { WalletSummaryPayload } from '../../../packages/wallet/src/vaultApi';

type Reply = { result?: unknown; error?: { code: number; message: string } };
type Pending = {
  id: string;
  clientId: string;
  tabId: number;
  documentId: string;
  origin: string;
  method: 'nock_connect' | 'nock_signMessage';
  message?: string;
  address?: string;
  windowId?: number;
  approving: boolean;
  respond: (reply: Reply) => void;
  timer: ReturnType<typeof setTimeout>;
};
const requests = new Map<string, Pending>();
const approvalUrl = chrome.runtime.getURL('dist/approval.html');
const failure = (message: string, code = 4001): Reply => ({ error: { code, message } });

function finish(request: Pending, reply: Reply) {
  if (!requests.delete(request.id)) return;
  clearTimeout(request.timer);
  request.respond(reply);
  if (request.windowId !== undefined) void chrome.windows.remove(request.windowId).catch(() => {});
}

async function wallets(): Promise<WalletSummaryPayload[]> {
  const response = await handleVaultMessage({ type: 'vault:getWallets' });
  if (!response.success) throw new Error(response.error || 'Unlock Nockster to continue.');
  return (response.data as { wallets: WalletSummaryPayload[] }).wallets;
}

function websiteSender(sender: chrome.runtime.MessageSender) {
  if (
    sender.id !== chrome.runtime.id ||
    sender.frameId !== 0 ||
    sender.tab?.id === undefined ||
    !sender.documentId
  )
    throw new Error('Wallet requests require a top-level website.');
  const url = new URL(sender.url || '');
  if (
    url.protocol !== 'https:' &&
    !(url.protocol === 'http:' && ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname))
  )
    throw new Error('Wallet requests require HTTPS.');
  return { tabId: sender.tab.id, documentId: sender.documentId, origin: url.origin };
}

function approvalRequest(id: unknown, sender: chrome.runtime.MessageSender): Pending {
  const request = typeof id === 'string' ? requests.get(id) : undefined;
  if (
    !request ||
    sender.id !== chrome.runtime.id ||
    sender.url !== `${approvalUrl}?request=${request.id}` ||
    sender.tab?.windowId !== request.windowId
  )
    throw new Error('This wallet request has expired. Return to the site and try again.');
  return request;
}

async function validateDocument(source: { tabId: number; documentId: string; origin: string }) {
  const reply = await chrome.tabs.sendMessage(
    source.tabId,
    { type: 'website:validateDocument' },
    { documentId: source.documentId }
  );
  if (reply?.origin !== source.origin) throw new Error('The requesting page changed. Try again.');
}

export async function handleWebsiteRequest(
  input: { type: string; id?: unknown; payload?: any; address?: unknown; approved?: unknown },
  sender: chrome.runtime.MessageSender,
  respond: (reply: any) => void
) {
  try {
    if (input.type === 'website:request' || input.type === 'website:cancel') {
      const source = websiteSender(sender);
      if (typeof input.id !== 'string' || input.id.length > 100)
        throw new Error('Invalid request ID.');
      if (input.type === 'website:cancel') {
        for (const request of requests.values())
          if (
            request.tabId === source.tabId &&
            request.documentId === source.documentId &&
            request.clientId === input.id
          )
            finish(request, failure('Nockster request cancelled.'));
        respond({});
        return;
      }
      const method = input.payload?.method;
      if (!['nock_connect', 'nock_signMessage'].includes(method)) {
        respond(failure('Nockster does not support this method.', 4200));
        return;
      }
      let address: string | undefined;
      let message: string | undefined;
      if (method === 'nock_signMessage') {
        message = input.payload?.params?.message;
        if (typeof message !== 'string' || !message.trim() || message.length > 16_384)
          throw new Error('Provide a message between 1 and 16384 characters.');
        const key = `websiteConnection:${source.origin}`;
        const connected = (await chrome.storage.session.get(key))[key];
        address = typeof connected === 'string' ? connected : undefined;
        if (!address) {
          respond(failure('Connect this site to Nockster first.', 4100));
          return;
        }
      }
      await validateDocument(source);
      if (
        requests.size >= 8 ||
        [...requests.values()].some(request => request.tabId === source.tabId)
      ) {
        respond(failure('Complete the open Nockster request first.', -32002));
        return;
      }
      const id = crypto.randomUUID();
      const request: Pending = {
        id,
        clientId: input.id,
        ...source,
        method,
        message,
        address,
        approving: false,
        respond,
        timer: setTimeout(() => finish(request, failure('Nockster request timed out.')), 180_000)
      };
      requests.set(id, request);
      try {
        const window = await chrome.windows.create({
          url: `${approvalUrl}?request=${id}`,
          type: 'popup',
          width: 420,
          height: 660,
          focused: true
        });
        if (!window) throw new Error('Unable to open Nockster approval.');
        request.windowId = window.id;
        if (!requests.has(id) && window.id !== undefined) await chrome.windows.remove(window.id);
      } catch (error) {
        finish(request, failure(error instanceof Error ? error.message : String(error), 4900));
      }
      return;
    }

    const request = approvalRequest(input.id, sender);
    if (input.type === 'website:details') {
      respond({
        success: true,
        data: {
          origin: request.origin,
          method: request.method,
          message: request.message,
          address: request.address
        }
      });
      return;
    }
    if (input.type !== 'website:approve') throw new Error('Unknown website request.');
    if (input.approved !== true) {
      respond({ success: true });
      finish(request, failure('Nockster request rejected.'));
      return;
    }
    if (request.approving) throw new Error('This request is already being approved.');
    request.approving = true;
    try {
      const wallet = (await wallets()).find(
        wallet =>
          wallet.publicKey === (request.method === 'nock_connect' ? input.address : request.address)
      );
      if (!wallet)
        throw new Error('The selected wallet is unavailable. Unlock Nockster and try again.');
      await validateDocument(request);
      if (!requests.has(request.id)) throw new Error('This request has expired.');
      let result: unknown;
      if (request.method === 'nock_connect') {
        await chrome.storage.session.set({
          [`websiteConnection:${request.origin}`]: wallet.publicKey
        });
        result = { account: { address: wallet.publicKey, type: 'v1' } };
      } else {
        const signed = await handleVaultMessage({
          type: 'vault:signIrisV1',
          nickname: wallet.nickname,
          address: wallet.publicKey,
          message: request.message!
        });
        if (!signed.success) throw new Error(signed.error || 'Unable to sign this message.');
        const proof = signed.data as { signature: unknown; public_key: string };
        result = {
          signature: proof.signature,
          publicKey: proof.public_key,
          address: wallet.publicKey
        };
      }
      respond({ success: true });
      finish(request, { result });
    } catch (error) {
      request.approving = false;
      throw error;
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    respond(
      input.type === 'website:request' ? failure(message, 4100) : { success: false, error: message }
    );
  }
}

chrome.windows.onRemoved.addListener(windowId => {
  for (const request of requests.values())
    if (request.windowId === windowId) finish(request, failure('Nockster request rejected.'));
});
chrome.tabs.onRemoved.addListener(tabId => {
  for (const request of requests.values())
    if (request.tabId === tabId) finish(request, failure('The requesting tab was closed.'));
});
chrome.tabs.onUpdated.addListener((tabId, change) => {
  if (change.status !== 'loading' && !change.url) return;
  for (const request of requests.values())
    if (request.tabId === tabId)
      finish(request, failure('The requesting page changed. Try again.'));
});
