import { writable } from 'svelte/store';
import type { Writable } from 'svelte/store';
import { ROUTES } from '../constants';

export type Route =
  | 'welcome'
  | 'create-wallet'
  | 'seed-phrase'
  | 'confirm-seed'
  | 'password-creation'
  | 'password-creation-import'
  | 'wallet-created'
  | 'wallet-imported'
  | 'import-wallet'
  | 'lock-screen'
  | 'dashboard'
  | 'wallet-management'
  | 'send'
  | 'jam'
  | 'receive'
  | 'confirm-transaction'
  | 'transaction-failed'
  | 'transaction-details'
  | 'history'
  | 'settings'
  | 'address-book'
  | 'hardware-wallet'
  | 'import-watch-only';

export interface RouterState {
  currentRoute: Route;
  previousRoute: Route | null;
  routeData: any;
  history: Route[];
}

const initialState: RouterState = {
  currentRoute: ROUTES.WELCOME as Route,
  previousRoute: null,
  routeData: null,
  history: []
};

function createRouter() {
  const { subscribe, set, update }: Writable<RouterState> = writable(initialState);

  return {
    subscribe,

    navigate: (route: Route, data: any = null) => {
      console.log('🚀 Router navigating to:', route, 'with data:', data);
      update(state => {
        console.log('📍 Previous route:', state.currentRoute, '→ New route:', route);
        const newState = {
          ...state,
          currentRoute: route,
          previousRoute: state.currentRoute,
          routeData: data,
          history: [...state.history, state.currentRoute].slice(-10) // Keep last 10 routes
        };
        // Save current route to storage for persistence
        router.saveToStorage(newState);
        return newState;
      });
    },

    back: () => {
      update(state => {
        if (state.history.length > 0) {
          const newHistory = [...state.history];
          const previousRoute = newHistory.pop() || 'welcome';
          const newState = {
            ...state,
            currentRoute: previousRoute,
            previousRoute: state.currentRoute,
            history: newHistory
          };
          router.saveToStorage(newState);
          return newState;
        }
        return state;
      });
    },

    reset: () => {
      set(initialState);
      router.saveToStorage(initialState);
    },

    // Load router state from Chrome storage
    loadFromStorage: async () => {
      if (import.meta.env.MODE === 'desktop') return;
      return new Promise<void>(resolve => {
        if (typeof chrome !== 'undefined' && chrome.storage) {
          chrome.storage.local.get(['routerState'], result => {
            if (result.routerState) {
              set(result.routerState as RouterState);
            }
            resolve();
          });
        } else {
          const stored = localStorage.getItem('routerState');
          if (stored) {
            try {
              set(JSON.parse(stored));
            } catch (e) {
              console.error('Failed to parse router state:', e);
            }
          }
          resolve();
        }
      });
    },

    // Save router state to Chrome storage
    saveToStorage: (state: RouterState) => {
      if (import.meta.env.MODE === 'desktop') return;
      if (typeof chrome !== 'undefined' && chrome.storage) {
        chrome.storage.local.set({ routerState: state });
      } else {
        localStorage.setItem('routerState', JSON.stringify(state));
      }
    }
  };
}

export const router = createRouter();
