import type { MineOptions, MineResult } from '../../../public/vanity/miner.js';

/** Own the worker so cancelling never waits for a CPU batch or GPU reply. */
export function mineAddress(options: MineOptions): Promise<MineResult | null> {
  return new Promise((resolve, reject) => {
    const { signal, onProgress, ...settings } = options;
    if (signal?.aborted) {
      reject(new DOMException('Search cancelled', 'AbortError'));
      return;
    }
    const worker = new Worker(new URL('vanity/worker.js', document.baseURI), { type: 'module' });
    let finished = false;
    const finish = (error?: Error, result: MineResult | null = null) => {
      if (finished) return;
      finished = true;
      signal?.removeEventListener('abort', abort);
      worker.terminate();
      worker.onmessage = null;
      worker.onerror = null;
      worker.onmessageerror = null;
      if (error) reject(error);
      else resolve(result);
    };
    const abort = () => finish(new DOMException('Search cancelled', 'AbortError'));
    signal?.addEventListener('abort', abort, { once: true });
    worker.onerror = event => finish(new Error(event.message || 'Address generation failed.'));
    worker.onmessageerror = () =>
      finish(new Error('Cannot decode the address generator response.'));
    worker.onmessage = ({ data }) => {
      if (finished) return;
      if (data.type === 'error') finish(new Error(data.message));
      else if (data.type === 'stopped') abort();
      else if (data.type === 'limit') finish();
      else if (data.type === 'match')
        finish(undefined, {
          pkh: data.pkh,
          keyJson: new Uint8Array(data.bytes),
          attempts: data.attempts,
          backend: data.adapter
        });
      else {
        try {
          onProgress?.(data);
        } catch (error) {
          finish(error instanceof Error ? error : new Error(String(error)));
        }
      }
    };
    try {
      worker.postMessage({ ...settings, type: 'start' });
    } catch (error) {
      finish(error instanceof Error ? error : new Error(String(error)));
    }
  });
}
