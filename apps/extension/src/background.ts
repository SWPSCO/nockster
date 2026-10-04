import { submitIrisV1Transaction } from '../../../packages/wallet/src/platform/submission';
import { handleWebsiteRequest } from './websiteRequests';
// Background service worker for Nockster wallet
import type { ExtensionMessage, ExtensionResponse } from '../../../packages/wallet/src/lib/types';
import { ERROR_MESSAGES, NETWORK } from '../../../packages/wallet/src/lib/constants';
import { fetchAuthenticatedRpc, clearRpcAuth, ensureRpcAuth } from '../../../packages/wallet/src/platform/rpcAuth';
import { handleVaultMessage, checkAutoLock, type VaultMessage } from '../../../packages/wallet/src/vault/engine';

const RPC_URL = NETWORK.RPC_URL;

chrome.storage.onChanged.addListener((changes, area) => {
  if (area !== 'local' || !changes.walletState) return;
  type WalletState = {
    activeWallet?: { addresses?: string[]; watchOnly?: boolean; hardware?: unknown };
  };
  const { oldValue, newValue } = changes.walletState as {
    oldValue?: WalletState;
    newValue?: WalletState;
  };
  const wallet = newValue?.activeWallet;
  if (oldValue?.activeWallet?.addresses?.[0] !== wallet?.addresses?.[0]) {
    clearRpcAuth();
    if (wallet?.addresses?.[0] && !wallet.watchOnly && !wallet.hardware) {
      // Network failures do not prevent saving a wallet. RPC requests retry sign-in.
      void ensureRpcAuth().catch(error => console.warn('[Background] Nockblocks sign-in:', error));
    }
  }
});

console.log('[Background] Using RPC URL:', RPC_URL);

function getPopupPath(): string {
  const manifest = chrome.runtime.getManifest() as { action?: { default_popup?: string } };
  const popup = manifest?.action?.default_popup;
  return typeof popup === 'string' && popup.length > 0 ? popup : 'index.html';
}

function buildHandoffPopupUrls(options: {
  action: string;
  params: Array<[string, string]>;
  origin?: string;
}): { fullUrl: string; popupRelative: string } {
  const popupBase = new URL(chrome.runtime.getURL(getPopupPath()));
  const outgoing = new URLSearchParams(popupBase.search);

  outgoing.set('handoff', '1');
  outgoing.set('action', options.action);
  if (options.origin) {
    outgoing.set('origin', options.origin);
  }

  for (const [key, value] of options.params) {
    outgoing.append(key, value);
  }

  popupBase.search = outgoing.toString();

  const fullUrl = popupBase.toString();
  const popupRelative = popupBase.pathname.replace(/^\//, '') + popupBase.search;
  return { fullUrl, popupRelative };
}

async function openHandoffUi(options: {
  tabId?: number;
  popupRelative: string;
  fullUrl: string;
}): Promise<void> {
  const defaultPopup = getPopupPath();

  const setPopup = async (popup: string): Promise<void> => {
    if (typeof chrome.action?.setPopup !== 'function' || typeof options.tabId !== 'number') {
      throw new Error('chrome.action.setPopup unavailable');
    }

    return new Promise((resolve, reject) => {
      chrome.action.setPopup({ tabId: options.tabId!, popup }, () => {
        if (chrome.runtime.lastError) {
          reject(new Error(chrome.runtime.lastError.message));
          return;
        }
        resolve();
      });
    });
  };

  const openPopup = async (): Promise<void> => {
    if (typeof chrome.action?.openPopup !== 'function') {
      throw new Error('chrome.action.openPopup unavailable');
    }

    return new Promise((resolve, reject) => {
      chrome.action.openPopup(() => {
        if (chrome.runtime.lastError) {
          reject(new Error(chrome.runtime.lastError.message));
          return;
        }
        resolve();
      });
    });
  };

  const openWindow = async (): Promise<void> => {
    if (typeof chrome.windows?.create !== 'function') {
      throw new Error('chrome.windows.create unavailable');
    }

    return new Promise((resolve, reject) => {
      chrome.windows.create(
        {
          url: options.fullUrl,
          type: 'popup',
          focused: true,
          width: 420,
          height: 640
        },
        () => {
          if (chrome.runtime.lastError) {
            reject(new Error(chrome.runtime.lastError.message));
            return;
          }
          resolve();
        }
      );
    });
  };

  const openTab = async (): Promise<void> => {
    if (typeof chrome.tabs?.create !== 'function') {
      throw new Error('chrome.tabs.create unavailable');
    }

    return new Promise((resolve, reject) => {
      chrome.tabs.create({ url: options.fullUrl, active: true }, () => {
        if (chrome.runtime.lastError) {
          reject(new Error(chrome.runtime.lastError.message));
          return;
        }
        resolve();
      });
    });
  };

  // Prefer the action popup (the same UX as clicking the extension icon).
  if (
    typeof options.tabId === 'number' &&
    typeof chrome.action?.setPopup === 'function' &&
    typeof chrome.action?.openPopup === 'function'
  ) {
    try {
      await setPopup(options.popupRelative);
      await openPopup();
      await setPopup(defaultPopup);
      return;
    } catch (err) {
      try {
        await setPopup(defaultPopup);
      } catch {
        // ignore
      }
      console.warn('[Background] Failed to open action popup; falling back:', err);
    }
  }

  // Fallback: open a dedicated popup window.
  try {
    await openWindow();
    return;
  } catch (err) {
    console.warn('[Background] Failed to open popup window; falling back to tab:', err);
  }

  await openTab();
}

// Message handler for communication with popup
chrome.runtime.onMessage.addListener((request, sender, sendResponse) => {
  (async () => {
    if (!request || typeof request.type !== 'string') {
      sendResponse({ success: false, error: 'Invalid request' });
      return;
    }
    if (request.type.startsWith('website:')) {
      await handleWebsiteRequest(request, sender, sendResponse);
      return;
    }
    const trustedView =
      sender.id === chrome.runtime.id &&
      typeof sender.url === 'string' &&
      sender.url.startsWith(chrome.runtime.getURL(''));
    if (request.type !== 'openHandoff' && !trustedView) {
      sendResponse({ success: false, error: 'Request requires the wallet UI' });
      return;
    }
    // Handle vault messages
    if (typeof request.type === 'string' && request.type.startsWith('vault:')) {
      if (['vault:lock', 'vault:suspend', 'vault:wipe', 'vault:unlock'].includes(request.type))
        clearRpcAuth();
      const result = await handleVaultMessage(request as VaultMessage);
      sendResponse(result);
      return;
    }

    if (request.type === 'openHandoff') {
      const action = typeof request.action === 'string' ? request.action : '';
      const params: Array<[string, string]> = [];
      if (Array.isArray(request.params)) {
        for (const entry of request.params) {
          if (
            Array.isArray(entry) &&
            entry.length === 2 &&
            typeof entry[0] === 'string' &&
            typeof entry[1] === 'string'
          ) {
            params.push([entry[0], entry[1]]);
          }
        }
      }
      const origin = sender.url ? new URL(sender.url).origin : undefined;

      if (!action) {
        sendResponse({
          success: false,
          error: 'Missing handoff action'
        } satisfies ExtensionResponse);
        return;
      }

      const { fullUrl, popupRelative } = buildHandoffPopupUrls({ action, params, origin });

      try {
        await openHandoffUi({ tabId: sender.tab?.id, popupRelative, fullUrl });
        sendResponse({ success: true, data: { url: fullUrl } } satisfies ExtensionResponse);
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        sendResponse({ success: false, error: message } satisfies ExtensionResponse);
      }
    } else if (request.type === 'rpcRequest') {
      const body = JSON.stringify(request.payload);
      const targetUrl = typeof request.rpcUrl === 'string' ? request.rpcUrl : RPC_URL;
      if (![NETWORK.RPC_URL, NETWORK.RPC_V1_URL].includes(targetUrl)) {
        throw new Error('RPC endpoint is not configured');
      }
      if (request.payload?.method === 'submitTransaction') {
        const raw = request.payload.params?.[0]?.rawTransaction;
        if (typeof raw !== 'string') throw new Error('Missing signed transaction');
        const result = await submitIrisV1Transaction(raw);
        sendResponse({ success: true, data: { jsonrpc: '2.0', id: request.payload.id, result } });
        return;
      }
      const resp = await fetchAuthenticatedRpc(targetUrl, body, AbortSignal.timeout(30_000));
      const text = await resp.text();
      sendResponse(
        resp.ok
          ? { success: true, data: text ? JSON.parse(text) : {} }
          : { success: false, error: `HTTP ${resp.status}: ${text}` }
      );
    } else {
      sendResponse({ success: false, error: 'Unknown request type' });
    }
  })().catch(error => {
    sendResponse({ success: false, error: error instanceof Error ? error.message : String(error) });
  });
  return true;
});

// Keep service worker alive and check auto-lock periodically
setInterval(() => {
  // This keeps the service worker alive
  chrome.storage.local.get(null, () => {});
  // Check if we should auto-lock due to inactivity
  if (checkAutoLock()) clearRpcAuth();
}, 20000);
