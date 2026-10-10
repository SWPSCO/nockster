import { browserStorage as localStorage } from '../../platform/browserStorage';
import { writable } from 'svelte/store';
import type { Writable } from 'svelte/store';

export type Theme = 'light' | 'dark' | 'dark-blue' | 'purple';

export const themes: Theme[] = ['light', 'dark', 'dark-blue', 'purple'];

interface ThemeStore extends Writable<Theme> {
  setTheme: (theme: Theme) => void;
  toggleTheme: () => void;
  loadSavedTheme: () => Promise<Theme>;
}

function createThemeStore(): ThemeStore {
  const { subscribe, set, update } = writable<Theme>('light');

  return {
    subscribe,
    set,
    update,

    setTheme: (theme: Theme) => {
      if (themes.includes(theme)) {
        set(theme);
        // Apply theme to document
        document.documentElement.setAttribute('data-theme', theme);
        // Persist theme choice
        if (typeof chrome !== 'undefined' && chrome.storage) {
          chrome.storage.local.set({ theme });
        } else {
          localStorage.setItem('theme', theme);
        }
      }
    },

    toggleTheme: () => {
      update(currentTheme => {
        const currentIndex = themes.indexOf(currentTheme);
        const nextIndex = (currentIndex + 1) % themes.length;
        const newTheme = themes[nextIndex];

        // Apply theme to document
        document.documentElement.setAttribute('data-theme', newTheme);

        // Persist theme choice
        if (typeof chrome !== 'undefined' && chrome.storage) {
          chrome.storage.local.set({ theme: newTheme });
        } else {
          localStorage.setItem('theme', newTheme);
        }

        return newTheme;
      });
    },

    loadSavedTheme: async (): Promise<Theme> => {
      return new Promise(resolve => {
        if (typeof chrome !== 'undefined' && chrome.storage) {
          chrome.storage.local.get(['theme'], result => {
            const savedTheme = (result.theme as Theme) || 'light';
            set(savedTheme);
            document.documentElement.setAttribute('data-theme', savedTheme);
            resolve(savedTheme);
          });
        } else {
          const savedTheme = (localStorage.getItem('theme') as Theme) || 'light';
          set(savedTheme);
          document.documentElement.setAttribute('data-theme', savedTheme);
          resolve(savedTheme);
        }
      });
    }
  };
}

export const currentTheme = createThemeStore();
