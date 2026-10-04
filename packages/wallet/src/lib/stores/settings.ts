import { writable } from 'svelte/store';
import type { Writable } from 'svelte/store';

export interface Settings {
  autoLockTimeout: number; // in minutes, 0 = disabled
  currency: 'USD' | 'EUR' | 'GBP' | 'JPY';
  language: 'en' | 'es' | 'fr' | 'de' | 'ja';
  developerMode: boolean;
  showTestNetworks: boolean;
  enableNotifications: boolean;
  defaultFeeLevel: 'fast' | 'standard' | 'economy';
  addressBookSyncEnabled: boolean;
}

const defaultSettings: Settings = {
  autoLockTimeout: 5,
  currency: 'USD',
  language: 'en',
  developerMode: false,
  showTestNetworks: false,
  enableNotifications: true,
  defaultFeeLevel: 'standard',
  addressBookSyncEnabled: false
};

interface SettingsStore extends Writable<Settings> {
  loadSettings: () => Promise<void>;
  saveSettings: () => void;
  updateSetting: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  resetToDefaults: () => void;
}

function createSettingsStore(): SettingsStore {
  const { subscribe, set, update } = writable<Settings>(defaultSettings);

  return {
    subscribe,
    set,
    update,

    loadSettings: async (): Promise<void> => {
      return new Promise(resolve => {
        if (typeof chrome !== 'undefined' && chrome.storage) {
          chrome.storage.local.get(['settings'], result => {
            if (result.settings) {
              set({ ...defaultSettings, ...result.settings });
            }
            resolve();
          });
        } else {
          const stored = localStorage.getItem('settings');
          if (stored) {
            try {
              const settings = JSON.parse(stored);
              set({ ...defaultSettings, ...settings });
            } catch (e) {
              console.error('Failed to parse settings:', e);
              set(defaultSettings);
            }
          }
          resolve();
        }
      });
    },

    saveSettings: () => {
      update(settings => {
        if (typeof chrome !== 'undefined' && chrome.storage) {
          chrome.storage.local.set({ settings });
        } else {
          localStorage.setItem('settings', JSON.stringify(settings));
        }
        return settings;
      });
    },

    updateSetting: <K extends keyof Settings>(key: K, value: Settings[K]) => {
      update(settings => {
        const updated = { ...settings, [key]: value };

        // Auto-save settings
        if (typeof chrome !== 'undefined' && chrome.storage) {
          chrome.storage.local.set({ settings: updated });
        } else {
          localStorage.setItem('settings', JSON.stringify(updated));
        }

        return updated;
      });
    },

    resetToDefaults: () => {
      set(defaultSettings);
      if (typeof chrome !== 'undefined' && chrome.storage) {
        chrome.storage.local.set({ settings: defaultSettings });
      } else {
        localStorage.setItem('settings', JSON.stringify(defaultSettings));
      }
    }
  };
}

export const settingsStore = createSettingsStore();
