import type { ComposeInput, ComposeSummary } from './composer';

export type ComposedDraft = { bytes: Uint8Array; summary: ComposeSummary; txId: string };

export function createComposerWorker() {
  let cancelPending: (() => void) | undefined;
  return {
    cancel() {
      cancelPending?.();
    },
    run(input: ComposeInput): Promise<ComposedDraft> {
      cancelPending?.();
      return new Promise((resolve, reject) => {
        const worker = new Worker(new URL('./composer.worker.ts', import.meta.url), {
          type: 'module'
        });
        const finish = () => {
          clearTimeout(timeout);
          worker.terminate();
          cancelPending = undefined;
        };
        const timeout = setTimeout(() => {
          finish();
          reject(
            new Error(
              'Input selection took too long. Try a smaller amount or choose notes manually.'
            )
          );
        }, 20_000);
        cancelPending = () => {
          finish();
          reject(new DOMException('Input selection cancelled', 'AbortError'));
        };
        worker.onmessage = event => {
          finish();
          if (event.data.error) reject(new Error(event.data.error));
          else resolve(event.data);
        };
        worker.onerror = event => {
          event.preventDefault();
          finish();
          reject(new Error(event.message || 'Unable to calculate transaction inputs.'));
        };
        worker.postMessage(input);
      });
    }
  };
}
