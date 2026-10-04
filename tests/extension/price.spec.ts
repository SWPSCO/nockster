import { test, expect } from '@playwright/test';

test('price polling hides failed estimates, reports HTTP status, and recovers', async ({
  page
}) => {
  await page.goto('/tests/extension/wallet-ui.html');
  const result = await page.evaluate(async () => {
    const path = '/packages/wallet/src/lib/stores/price.ts';
    const { priceStore } = await import(/* @vite-ignore */ path);
    const warnings: string[] = [];
    const errors: string[] = [];
    const originalFetch = window.fetch;
    const originalWarn = console.warn;
    const originalError = console.error;
    const originalInterval = window.setInterval;
    let poll: (() => void) | undefined;
    let calls = 0;
    let state: {
      nockToUsd: number;
      error: string | null;
      isLoading: boolean;
      lastUpdated: Date | null;
    };
    const unsubscribe = priceStore.subscribe(value => {
      state = value;
    });
    window.fetch = async () => {
      calls++;
      if (calls === 1) return Response.json({ price: 0.03 });
      if (calls === 2) return new Response('', { status: 503, statusText: '' });
      if (calls === 3) return Response.json({ price: 0 });
      return Response.json({ price: 0.04 });
    };
    console.warn = message => {
      warnings.push(String(message));
    };
    console.error = message => {
      errors.push(String(message));
    };
    window.setInterval = ((callback: () => void) => {
      poll = callback;
      return 1;
    }) as typeof window.setInterval;
    const tick = async () => {
      poll!();
      while (state!.isLoading) await new Promise(resolve => setTimeout(resolve, 0));
      return { ...state! };
    };
    try {
      await priceStore.init();
      const initial = { ...state! };
      const failed = await tick();
      const invalid = await tick();
      const recovered = await tick();
      return { initial, failed, invalid, recovered, warnings, errors, calls };
    } finally {
      priceStore.destroy();
      unsubscribe();
      window.fetch = originalFetch;
      window.setInterval = originalInterval;
      console.warn = originalWarn;
      console.error = originalError;
    }
  });
  expect(result.initial).toMatchObject({ nockToUsd: 0.03, error: null });
  expect(result.failed).toMatchObject({
    nockToUsd: 0,
    error: 'Price request failed (HTTP 503)',
    isLoading: false
  });
  expect(result.invalid).toMatchObject({ nockToUsd: 0, error: 'Invalid price data received' });
  expect(result.recovered).toMatchObject({ nockToUsd: 0.04, error: null, isLoading: false });
  expect(result.warnings).toHaveLength(2);
  expect(result.warnings[0]).toContain('HTTP 503');
  expect(result.errors).toEqual([]);
  expect(result.calls).toBe(4);
});
