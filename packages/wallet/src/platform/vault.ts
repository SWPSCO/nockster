import { browserStorage as localStorage } from './browserStorage';
import { Capacitor } from '@capacitor/core';
import { Preferences } from '@capacitor/preferences';
import { desktopStorage } from './desktopStorage';

// Persist ciphertext and public wallet metadata. Passwords and unlocked keys stay in memory.
export const vaultStorage = {
  async get(keys: string[]): Promise<Record<string, unknown>> {
    if (import.meta.env.MODE === 'urbit')
      return Object.fromEntries(
        keys.flatMap(key => {
          const value = localStorage.getItem(key);
          return value === null ? [] : [[key, JSON.parse(value)]];
        })
      );
    if (import.meta.env.MODE === 'desktop') return desktopStorage.get(keys);
    if (import.meta.env.MODE !== 'mobile') return chrome.storage.local.get(keys);
    const items: Record<string, unknown> = {};
    for (const key of keys) {
      const value = Capacitor.isNativePlatform()
        ? (await Preferences.get({ key })).value
        : localStorage.getItem(key);
      if (value !== null) items[key] = JSON.parse(value);
    }
    return items;
  },
  async set(items: Record<string, unknown>): Promise<void> {
    if (import.meta.env.MODE === 'urbit') {
      for (const [key, value] of Object.entries(items))
        localStorage.setItem(key, JSON.stringify(value));
      return;
    }
    if (import.meta.env.MODE === 'desktop') return desktopStorage.set(items);
    if (import.meta.env.MODE !== 'mobile') return chrome.storage.local.set(items);
    for (const [key, item] of Object.entries(items)) {
      const value = JSON.stringify(item);
      if (Capacitor.isNativePlatform()) {
        await Preferences.set({ key, value });
      } else {
        localStorage.setItem(key, value);
      }
    }
  },
  async remove(keys: string[]): Promise<void> {
    if (import.meta.env.MODE === 'urbit') {
      keys.forEach(key => localStorage.removeItem(key));
      return;
    }
    if (import.meta.env.MODE === 'desktop') return desktopStorage.remove(keys);
    if (import.meta.env.MODE !== 'mobile') return chrome.storage.local.remove(keys);
    for (const key of keys) {
      if (Capacitor.isNativePlatform()) {
        await Preferences.remove({ key });
      } else {
        localStorage.removeItem(key);
      }
    }
  }
};

export function vaultWasmUrl(): string {
  return ['mobile', 'desktop', 'urbit'].includes(import.meta.env.MODE)
    ? new URL('../pkg/nockster_core_bg.wasm', import.meta.url).href
    : chrome.runtime.getURL('dist/nockster_core_bg.wasm');
}
