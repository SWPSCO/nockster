import { resolve } from 'node:path';
export const source = `/@fs${resolve('packages/wallet/src')}`;
import type { Page } from '@playwright/test';
import { mockAccounts } from '../mobile/rpcAuth.fixture';

// Emulate only native I/O. Wallet cryptography, auth, routing, and UI run unchanged.
export async function desktopIO(
  page: Page,
  hardware?: (command: string, args: any) => Promise<unknown>
) {
  if (hardware) await page.exposeFunction('desktopHardware', hardware);
  await page.route('https://**', route => route.abort());
  await mockAccounts(page);
  await page.route('https://nockblocks.com/rpc{,/v1}', route => {
    const { method, id } = route.request().postDataJSON();
    const results: Record<string, unknown> = {
      getBlockchainMetrics: { blockHeight: 200000 },
      getTip: { height: 200000, timestamp: Date.now() },
      getNotes: { nicks: 501728379, notes: [] },
      getNotesByAddress: [],
      getTransactionsByAddress: { transactions: [], total: 0 },
      getTransactionSubmission: null
    };
    if (!(method in results)) throw new Error(`Unexpected desktop RPC method: ${method}`);
    return route.fulfill({
      json: { jsonrpc: '2.0', id, result: results[method], price: 0.018159 }
    });
  });
  await page.addInitScript(() => {
    const transport = window.fetch.bind(window);
    const requests = new Map<number, any>();
    const bodies = new Map<number, Uint8Array>();
    let id = 100;
    const read = () => JSON.parse(localStorage.getItem('desktop-test-native-store') || '{}');
    const save = (data: unknown) =>
      localStorage.setItem('desktop-test-native-store', JSON.stringify(data));
    (window as any).__TAURI_INTERNALS__ = {
      invoke: async (command: string, args: any) => {
        if (command.startsWith('hardware_')) {
          if ((window as any).desktopHardware)
            return (window as any).desktopHardware(command, args);
          if (command === 'hardware_devices') return [];
          throw new Error('No test device is connected');
        }
        if (command === 'desktop_storage_get')
          return Object.fromEntries(
            args.keys
              .filter((key: string) => Object.hasOwn(read(), key))
              .map((key: string) => [key, read()[key]])
          );
        if (command === 'desktop_storage_set') {
          save({ ...read(), ...args.items });
          return;
        }
        if (command === 'desktop_storage_remove') {
          const data = read();
          for (const key of args.keys) delete data[key];
          save(data);
          sessionStorage.setItem(
            'desktop-test-removed-keys',
            JSON.stringify([
              ...JSON.parse(sessionStorage.getItem('desktop-test-removed-keys') || '[]'),
              ...args.keys
            ])
          );
          return;
        }
        if (command === 'plugin:http|fetch') {
          requests.set(++id, args.clientConfig);
          return id;
        }
        if (command === 'plugin:http|fetch_send') {
          const request = requests.get(args.rid);
          requests.delete(args.rid);
          const response = await transport(request.url, {
            method: request.method,
            headers: request.headers,
            body: request.data ? new Uint8Array(request.data) : undefined
          });
          const rid = ++id;
          bodies.set(rid, new Uint8Array(await response.arrayBuffer()));
          return {
            rid,
            status: response.status,
            statusText: response.statusText,
            url: response.url,
            headers: Array.from(response.headers.entries())
          };
        }
        if (command === 'plugin:http|fetch_read_body') {
          const body = bodies.get(args.rid);
          if (!body) return [1];
          bodies.delete(args.rid);
          return [...body, 0];
        }
        if (command === 'plugin:http|fetch_cancel' || command === 'plugin:http|fetch_cancel_body')
          return;
        if (command === 'plugin:opener|open_url') {
          (window as any).__openedUrl = args.url;
          return;
        }
        throw new Error(`Unexpected native command ${command}`);
      }
    };
  });
}

export const password = 'synthetic desktop wallet password';
export const mnemonic =
  'fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float';

export async function createWallet(page: Page) {
  await page.evaluate(
    async ({ password, mnemonic, source }) => {
      const { newVault, importWallet } = await import(`${source}/vaultController.ts`);
      const { getWalletsFromVault } = await import(`${source}/lib/utils/vaultBridge.ts`);
      const { walletStore } = await import(`${source}/lib/stores/wallet.ts`);
      await newVault(password);
      await importWallet('Savings', mnemonic);
      const result = await getWalletsFromVault();
      if (!result.success) throw new Error(result.error);
      walletStore.setWallets(result.wallets);
      walletStore.selectWallet(result.wallets[0].id);
      walletStore.unlock();
      await walletStore.saveToStorage();
    },
    { password, mnemonic, source }
  );
}
