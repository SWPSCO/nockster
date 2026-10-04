// Runs in the website's MAIN world; privileged work stays in the extension.
(() => {
  const target = 'NOCKSTER_PROVIDER';
  const provider = Object.freeze({
    provider: 'nockster',
    request(args: { method: string; params?: unknown; api?: string; timeout?: number }) {
      return new Promise((resolve, reject) => {
        const id = crypto.randomUUID();
        const timeout = Math.min(Math.max(args?.timeout || 120_000, 1000), 180_000);
        const finish = () => {
          clearTimeout(timer);
          window.removeEventListener('message', receive);
        };
        const receive = (event: MessageEvent) => {
          if (event.source !== window || event.origin !== location.origin) return;
          const data = event.data;
          if (data?.target !== target || data.id !== id || data.kind !== 'response') return;
          finish();
          if (data.error)
            reject(Object.assign(new Error(data.error.message), { code: data.error.code }));
          else resolve(data.result);
        };
        const timer = setTimeout(() => {
          finish();
          window.postMessage({ target, kind: 'cancel', id }, location.origin);
          reject(
            Object.assign(new Error('Nockster request timed out. Try again.'), { code: 4001 })
          );
        }, timeout);
        window.addEventListener('message', receive);
        window.postMessage({ target, kind: 'request', id, payload: args }, location.origin);
      });
    }
  });
  Object.defineProperty(window, 'nockster', {
    value: provider,
    writable: false,
    configurable: false
  });
  window.dispatchEvent(new Event('nockster#initialized'));
})();
