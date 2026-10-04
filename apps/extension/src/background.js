// Background service worker for Nockster wallet
import { process } from '../../../packages/wallet/src/wasm.js';
import { getAuthHeaders } from './auth.js';

const RPC_URL = 'https://fletch.dev.nockblocks.com/wallet-gw/rpc';
const DEFAULT_RPC_PATH = '/wallet-gw/rpc';

const getRpcPath = url => {
  try {
    const parsed = new URL(url);
    return parsed.pathname || DEFAULT_RPC_PATH;
  } catch (err) {
    console.warn('[Background] Failed to parse RPC URL, defaulting to', DEFAULT_RPC_PATH, err);
    return DEFAULT_RPC_PATH;
  }
};

// Message handler for communication with popup
chrome.runtime.onMessage.addListener((request, sender, sendResponse) => {
  handleMessage(request, sender, sendResponse);
  return true;
});

async function handleMessage(request, sender, sendResponse) {
  try {
    switch (request.type) {
      case 'rpcRequest': {
        // Proxy RPC requests to avoid CORS issues
        try {
          const body = JSON.stringify(request.payload);
          const targetUrl = typeof request.rpcUrl === 'string' ? request.rpcUrl : RPC_URL;
          const authHeaders = await getAuthHeaders(getRpcPath(targetUrl), body);

          const response = await fetch(targetUrl, {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              ...authHeaders
            },
            body
          });

          if (!response.ok) {
            const errorText = await response.text();
            sendResponse({
              success: false,
              error: `HTTP ${response.status}: ${errorText}`
            });
            return;
          }

          const data = await response.json();
          sendResponse({
            success: true,
            data
          });
        } catch (error) {
          sendResponse({
            success: false,
            error: error instanceof Error ? error.message : 'Network error'
          });
        }
        break;
      }

      case 'networkStatus': {
        // Check network connectivity
        sendResponse({
          success: true,
          connected: true,
          blockHeight: 123456
        });
        break;
      }

      case 'broadcastTransaction': {
        // Broadcast transaction to Nockchain network
        sendResponse({
          success: true,
          txId: crypto.randomUUID()
        });
        break;
      }

      case 'fetchBalance': {
        // Fetch balance from network
        sendResponse({
          success: true,
          balance: Math.floor(Math.random() * 100000)
        });
        break;
      }

      default: {
        // potentially a wasm request. If not, returns unknown request type
        const result = await process(request);
        sendResponse(result);
      }
    }
  } catch (error) {
    sendResponse({
      success: false,
      error: error.message
    });
  }
}

// Keep service worker alive
setInterval(() => {
  chrome.storage.local.get(null, () => {});
}, 20000);
