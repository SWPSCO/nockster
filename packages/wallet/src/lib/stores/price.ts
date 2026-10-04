import { writable, derived } from 'svelte/store';
import type { Writable } from 'svelte/store';
import { NETWORK } from '../constants';

interface PriceState {
  nockToUsd: number;
  isLoading: boolean;
  lastUpdated: Date | null;
  error: string | null;
}

const initialState: PriceState = {
  nockToUsd: 0,
  isLoading: false,
  lastUpdated: null,
  error: null
};

function createPriceStore() {
  const { subscribe, update }: Writable<PriceState> = writable(initialState);

  let updateInterval: ReturnType<typeof setTimeout> | null = null;

  async function fetchNockPrice() {
    update(state => ({ ...state, isLoading: true, error: null }));

    try {
      const response = await fetch(NETWORK.RPC_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: 'price', method: 'getTip', params: [] }),
        signal: AbortSignal.timeout(5000)
      });

      if (!response.ok) {
        throw new Error(
          `Price request failed (HTTP ${response.status}${response.statusText ? ` ${response.statusText}` : ''})`
        );
      }

      const data = await response.json();
      const price = data.price;

      if (typeof price !== 'number' || !Number.isFinite(price) || price <= 0) {
        throw new Error('Invalid price data received');
      }

      update(state => ({
        ...state,
        nockToUsd: price,
        isLoading: false,
        lastUpdated: new Date(),
        error: null
      }));

      return price;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to fetch price';
      console.warn(`NOCK price unavailable; USD estimates are hidden: ${errorMessage}`);

      update(state => ({
        ...state,
        isLoading: false,
        error: errorMessage,
        nockToUsd: 0
      }));

      throw error;
    }
  }

  return {
    subscribe,

    // Initialize and start periodic updates
    init: async () => {
      if (updateInterval) return;
      try {
        await fetchNockPrice();
      } catch {
        // USD estimates stay hidden while the price is unavailable.
      }

      // Update price every 60 seconds
      updateInterval = setInterval(() => {
        fetchNockPrice().catch(() => {
          // The store records the failure; the next interval refreshes the price.
        });
      }, 60000);
    },

    // Clean up interval on destroy
    destroy: () => {
      if (updateInterval) {
        clearInterval(updateInterval);
        updateInterval = null;
      }
    },

    // Manual refresh
    refresh: fetchNockPrice,

    // Format USD amount
    formatUSD: (nockAmount: number, price: number): string => {
      const usdAmount = nockAmount * price;
      return new Intl.NumberFormat('en-US', {
        style: 'currency',
        currency: 'USD',
        minimumFractionDigits: 2,
        maximumFractionDigits: 2
      }).format(usdAmount);
    }
  };
}

export const priceStore = createPriceStore();

// Derived store for easy access to just the price
export const nockPrice = derived(priceStore, $priceStore => $priceStore.nockToUsd);
