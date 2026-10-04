import { writable } from 'svelte/store';

export type Route =
  | 'welcome'
  | 'seed-phrase'
  | 'confirm-seed'
  | 'password-creation'
  | 'dashboard'
  | 'send'
  | 'receive'
  | 'history'
  | 'settings'
  | 'import-wallet'
  | 'address-book'
  | 'lock-screen'
  | 'confirm-transaction'
  | 'transaction-failed'
  | 'transaction-details'
  | 'wallet-management'
  | 'wallet-created'
  | 'wallet-imported';

export interface RouterState {
  currentRoute: Route;
  history: Route[];
  routeData?: any;
}

const initialState: RouterState = {
  currentRoute: 'welcome',
  history: []
};

function createRouter() {
  const { subscribe, set, update } = writable<RouterState>(initialState);

  return {
    subscribe,

    navigate: (route: Route, data?: any) => {
      update(state => ({
        currentRoute: route,
        history: [...state.history, state.currentRoute],
        routeData: data
      }));
    },

    back: () => {
      update(state => {
        if (state.history.length > 0) {
          const newHistory = [...state.history];
          const previousRoute = newHistory.pop()!;
          return {
            currentRoute: previousRoute,
            history: newHistory,
            routeData: undefined
          };
        }
        return state;
      });
    },

    reset: () => {
      set(initialState);
    }
  };
}

export const router = createRouter();