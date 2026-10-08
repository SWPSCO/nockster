import { get, writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';
import { firmwareUpdate } from '../hardware/firmware';
import {
  lockVaultSession,
  clearPendingWallet
} from '../../../../packages/wallet/src/lib/utils/vaultBridge';
import { walletStore } from '../../../../packages/wallet/src/lib/stores/wallet';

interface Status {
  phase: string;
  version?: string;
  downloaded: number;
  total?: number;
  error?: string;
}
const state = writable({
  status: { phase: '', downloaded: 0 } as Status,
  installing: false,
  installError: '',
  checkPending: false
});
const status = (value: Status) => state.update(current => ({ ...current, status: value }));

async function check() {
  const current = get(state);
  if (current.checkPending || current.installing) return;
  state.update(current => ({ ...current, checkPending: true }));
  try {
    status(await invoke<Status>('desktop_update_check'));
  } catch {
    status({
      phase: 'error',
      downloaded: 0,
      error: 'Unable to check for updates. Try again later.'
    });
  } finally {
    state.update(current => ({ ...current, checkPending: false }));
  }
}
async function install() {
  if (get(state).installing || get(firmwareUpdate).installing) return;
  state.update(current => ({ ...current, installing: true, installError: '' }));
  try {
    if (!(await lockVaultSession())) throw new Error('Unable to lock your wallet. Try again.');
    clearPendingWallet();
    walletStore.lock();
    await invoke('desktop_update_install');
  } catch (error) {
    state.update(current => ({
      ...current,
      installing: false,
      installError: error instanceof Error ? error.message : String(error)
    }));
    try {
      status(await invoke<Status>('desktop_update_status'));
    } catch {
      /* Keep the failure visible. */
    }
  }
}
async function downloadPage() {
  try {
    await openUrl('https://nockster.com/');
  } catch {
    state.update(current => ({
      ...current,
      installError: 'Open nockster.com in your browser to download the installer.'
    }));
  }
}
function start() {
  let alive = true;
  let polling = false;
  void check();
  const poll = setInterval(async () => {
    if (polling || !get(state).checkPending) return;
    polling = true;
    try {
      const latest = await invoke<Status>('desktop_update_status');
      if (alive) status(latest);
    } catch {
      /* A closed native window has no status to display. */
    } finally {
      polling = false;
    }
  }, 1000);
  const periodic = setInterval(() => void check(), 6 * 60 * 60 * 1000);
  const online = () => void check();
  window.addEventListener('online', online);
  return () => {
    alive = false;
    clearInterval(poll);
    clearInterval(periodic);
    window.removeEventListener('online', online);
  };
}
export const desktopUpdates = { subscribe: state.subscribe, check, install, downloadPage, start };
