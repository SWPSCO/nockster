import { CapacitorHttp } from '@capacitor/core';

// Capacitor's fetch patch decodes non-JSON bodies as text. Its explicit file/arraybuffer
// transport preserves protobuf bytes on both Android and iOS.
export function createNativeBinaryFetch(send: typeof CapacitorHttp.request): typeof fetch {
  return async (input, init) => {
    const request = new Request(input, init);
    request.signal.throwIfAborted();
    const bytes = new Uint8Array(await request.arrayBuffer());
    let binary = '';
    for (const byte of bytes) binary += String.fromCharCode(byte);
    const reply = await send({
      url: request.url,
      method: 'POST',
      headers: Object.fromEntries(request.headers.entries()),
      data: btoa(binary),
      dataType: 'file',
      responseType: 'arraybuffer',
      disableRedirects: true,
      connectTimeout: 10_000,
      readTimeout: 30_000
    });
    request.signal.throwIfAborted();
    const body =
      reply.status >= 400
        ? String(reply.data)
        : Uint8Array.from(atob(reply.data), value => value.charCodeAt(0));
    return new Response(body, { status: reply.status, headers: reply.headers });
  };
}

export const nativeBinaryFetch = createNativeBinaryFetch(options => CapacitorHttp.request(options));
