import { browserStorage as localStorage } from '../../platform/browserStorage';
/**
 * Chrome Storage Utility
 * Provides a unified interface for Chrome storage with fallback to localStorage
 */

export interface StorageData {
  walletState?: any;
  transactions?: any[];
  settings?: any;
  theme?: string;
  addressBook?: any[];
  lastActivity?: number;
  encryptedVault?: string;
}

class StorageManager {
  private isExtension: boolean;

  constructor() {
    this.isExtension = typeof chrome !== 'undefined' && chrome.storage !== undefined;
  }

  /**
   * Get data from storage
   */
  async get<K extends keyof StorageData>(keys: K[]): Promise<Partial<StorageData>> {
    if (this.isExtension) {
      return new Promise(resolve => {
        chrome.storage.local.get(keys as string[], result => {
          resolve(result as Partial<StorageData>);
        });
      });
    } else {
      const result: Partial<StorageData> = {};
      for (const key of keys) {
        const value = localStorage.getItem(key);
        if (value) {
          try {
            result[key] = JSON.parse(value);
          } catch {
            result[key] = value as any;
          }
        }
      }
      return result;
    }
  }

  /**
   * Set data in storage
   */
  async set(data: Partial<StorageData>): Promise<void> {
    if (this.isExtension) {
      return new Promise((resolve, reject) => {
        chrome.storage.local.set(data, () => {
          if (chrome.runtime.lastError) {
            reject(chrome.runtime.lastError);
          } else {
            resolve();
          }
        });
      });
    } else {
      for (const [key, value] of Object.entries(data)) {
        localStorage.setItem(key, JSON.stringify(value));
      }
    }
  }

  /**
   * Remove data from storage
   */
  async remove(keys: (keyof StorageData)[]): Promise<void> {
    if (this.isExtension) {
      return new Promise((resolve, reject) => {
        chrome.storage.local.remove(keys as string[], () => {
          if (chrome.runtime.lastError) {
            reject(chrome.runtime.lastError);
          } else {
            resolve();
          }
        });
      });
    } else {
      for (const key of keys) {
        localStorage.removeItem(key);
      }
    }
  }

  /**
   * Clear all storage
   */
  async clear(): Promise<void> {
    if (this.isExtension) {
      return new Promise((resolve, reject) => {
        chrome.storage.local.clear(() => {
          if (chrome.runtime.lastError) {
            reject(chrome.runtime.lastError);
          } else {
            resolve();
          }
        });
      });
    } else {
      localStorage.clear();
    }
  }

  /**
   * Get storage size info (Chrome only)
   */
  async getBytesInUse(): Promise<number> {
    if (this.isExtension) {
      return new Promise(resolve => {
        chrome.storage.local.getBytesInUse(bytes => {
          resolve(bytes);
        });
      });
    } else {
      // Estimate localStorage size
      let size = 0;
      for (let index = 0; index < localStorage.length; index++) {
        const key = localStorage.key(index)!;
        size += (localStorage.getItem(key)?.length ?? 0) + key.length;
      }
      return size * 2; // Rough estimate (UTF-16)
    }
  }
}

export const storage = new StorageManager();

/**
 * Persist data to storage with encryption
 */
export async function persistToStorage(key: keyof StorageData, data: any): Promise<void> {
  try {
    await storage.set({ [key]: data });
  } catch (error) {
    console.error(`Failed to persist ${key}:`, error);
    throw error;
  }
}

/**
 * Load data from storage
 */
export async function loadFromStorage<K extends keyof StorageData>(
  key: K
): Promise<StorageData[K] | null> {
  try {
    const result = await storage.get([key]);
    return result[key] || null;
  } catch (error) {
    console.error(`Failed to load ${key}:`, error);
    return null;
  }
}

/**
 * Clear specific keys from storage
 */
export async function clearFromStorage(keys: (keyof StorageData)[]): Promise<void> {
  try {
    await storage.remove(keys);
  } catch (error) {
    console.error('Failed to clear storage keys:', error);
    throw error;
  }
}

/**
 * Monitor storage changes (Chrome only)
 */
export function onStorageChanged(
  callback: (changes: { [key: string]: chrome.storage.StorageChange }) => void
): () => void {
  if (typeof chrome !== 'undefined' && chrome.storage) {
    const listener = (changes: { [key: string]: chrome.storage.StorageChange }) => {
      callback(changes);
    };
    chrome.storage.onChanged.addListener(listener);
    return () => chrome.storage.onChanged.removeListener(listener);
  }
  // Return no-op for non-extension environments
  return () => {};
}
