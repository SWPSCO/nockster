// Capacitor's patched fetch does not honor AbortSignal for native POSTs. Bound
// the caller's wait as well, so an unreachable service cannot hold the wallet's
// serialized command queue open for the native transport's ten-minute timeout.
export async function fetchWithAbort(
  input: RequestInfo | URL,
  init?: RequestInit,
  transport: typeof fetch = fetch
): Promise<Response> {
  const signal = init?.signal ?? (input instanceof Request ? input.signal : undefined);
  if (!signal) return transport(input, init);
  signal.throwIfAborted();
  let onAbort: () => void;
  try {
    return await new Promise<Response>((resolve, reject) => {
      onAbort = () => reject(signal.reason);
      signal.addEventListener('abort', onAbort, { once: true });
      // Attach both handlers even when abort wins, to consume late rejections.
      transport(input, init).then(resolve, reject);
    });
  } finally {
    signal.removeEventListener('abort', onAbort!);
  }
}
