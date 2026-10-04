// chrome.storage helpers for managing the serialized vault blob.
// Falls back to localStorage when chrome.storage is unavailable (dev mode).

import type { StoredVault } from './types';
import { isStoredVault } from './types';

type StorageArea = 'local' | 'session';

const VAULT_STORAGE_KEY = 'vault';

// Check if chrome.storage is available (extension context)
function hasChromeStorage(): boolean {
  return typeof chrome !== 'undefined' && chrome.storage?.local !== undefined;
}

function storageGet(area: StorageArea, keys: string[]) {
  return new Promise<Record<string, unknown>>(resolve => {
    // Fallback to localStorage in dev mode
    if (!hasChromeStorage()) {
      const result: Record<string, unknown> = {};
      for (const key of keys) {
        const stored = localStorage.getItem(`fletch_${key}`);
        if (stored) {
          try {
            result[key] = JSON.parse(stored);
          } catch {
            console.warn(`[VaultStorage] Failed to parse localStorage key: ${key}`);
          }
        }
      }
      resolve(result);
      return;
    }

    const api = chrome.storage?.[area];
    if (!api) {
      resolve({});
      return;
    }

    api.get(keys, items => {
      const error = chrome.runtime.lastError;
      if (error) {
        console.warn(`[VaultStorage] storage get failed (${area})`, error);
        resolve({});
        return;
      }
      resolve(items);
    });
  });
}

function storageSet(area: StorageArea, items: Record<string, unknown>) {
  return new Promise<void>((resolve, reject) => {
    // Fallback to localStorage in dev mode
    if (!hasChromeStorage()) {
      try {
        for (const [key, value] of Object.entries(items)) {
          localStorage.setItem(`fletch_${key}`, JSON.stringify(value));
        }
        resolve();
      } catch (error) {
        reject(error);
      }
      return;
    }

    const api = chrome.storage?.[area];
    if (!api) {
      reject(new Error(`chrome.storage.${area} unavailable`));
      return;
    }

    api.set(items, () => {
      const error = chrome.runtime.lastError;
      if (error) {
        reject(error);
        return;
      }
      resolve();
    });
  });
}

function storageRemove(area: StorageArea, keys: string[]) {
  return new Promise<void>((resolve, reject) => {
    // Fallback to localStorage in dev mode
    if (!hasChromeStorage()) {
      for (const key of keys) {
        localStorage.removeItem(`fletch_${key}`);
      }
      resolve();
      return;
    }

    const api = chrome.storage?.[area];
    if (!api) {
      reject(new Error(`chrome.storage.${area} unavailable`));
      return;
    }

    api.remove(keys, () => {
      const error = chrome.runtime.lastError;
      if (error) {
        reject(error);
        return;
      }
      resolve();
    });
  });
}

export async function fetchStoredVault(): Promise<StoredVault | null> {
  const result = await storageGet('local', [VAULT_STORAGE_KEY]);
  const candidate = result?.[VAULT_STORAGE_KEY];
  return isStoredVault(candidate) ? candidate : null;
}

export async function persistStoredVault(vault: StoredVault) {
  await storageSet('local', { [VAULT_STORAGE_KEY]: vault });
}

export async function deleteStoredVault() {
  await storageRemove('local', [VAULT_STORAGE_KEY]);
}
