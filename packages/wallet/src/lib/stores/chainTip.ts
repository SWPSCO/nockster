import { writable } from 'svelte/store';
import { getRPCClientV1 } from '../utils/rpc';

export const chainTip = writable<number | null>(null);
let request: Promise<void> | undefined;
let users = 0;
let timer: ReturnType<typeof setInterval> | undefined;

export function refreshChainTip(): Promise<void> {
  return (request ??= getRPCClientV1()
    .getTipHeight()
    .then(height => {
      chainTip.set(height);
    })
    .catch(() => {
      chainTip.set(null);
    })
    .finally(() => {
      request = undefined;
    }));
}

export function watchChainTip() {
  if (users++ === 0) {
    void refreshChainTip();
    timer = setInterval(() => void refreshChainTip(), 30_000);
  }
  return () => {
    if (--users === 0) clearInterval(timer);
  };
}
