import { writable } from 'svelte/store';

export interface Settings {
  network: 'mainnet' | 'testnet';
  currency: 'USD' | 'EUR' | 'GBP';
  notifications: boolean;
  biometrics: boolean;
  autoLockTime: string;
  theme: 'light' | 'dark';
  language: 'en' | 'es' | 'fr' | 'de';
}

const defaultSettings: Settings = {
  network: 'mainnet',
  currency: 'USD',
  notifications: true,
  biometrics: false,
  autoLockTime: '5',
  theme: 'light',
  language: 'en'
};

function createSettingsStore() {
  const stored = localStorage.getItem('fletch-settings');
  const initial = stored ? JSON.parse(stored) : defaultSettings;
  
  const { subscribe, set, update } = writable<Settings>(initial);
  
  return {
    subscribe,
    
    updateSetting: <K extends keyof Settings>(key: K, value: Settings[K]) => {
      update(settings => {
        const updated = {
          ...settings,
          [key]: value
        };
        localStorage.setItem('fletch-settings', JSON.stringify(updated));
        return updated;
      });
    },
    
    reset: () => {
      set(defaultSettings);
      localStorage.setItem('fletch-settings', JSON.stringify(defaultSettings));
    }
  };
}

export const settingsStore = createSettingsStore();