import * as irisV1 from 'iris-v1/wasm';
import { initializeIrisV1, transactionProtobuf } from './irisV1';
import { fetchAuthenticatedRpc, GRPC_URL, GRPC_SERVICE } from './rpcAuth';
import { handleVaultMessage } from '../vault/engine';
import { Capacitor } from '@capacitor/core';
import { nativeBinaryFetch } from './nativeBinaryFetch';

let installed = false;
function installTransport() {
  if (installed) return;
  installed = true;
  const transport = globalThis.fetch.bind(globalThis);
  globalThis.fetch = async (input, init) => {
    const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
    if (
      ![
        `${GRPC_URL}${GRPC_SERVICE}WalletSendTransaction`,
        `${GRPC_URL}${GRPC_SERVICE}TransactionAccepted`
      ].includes(url)
    )
      return transport(input, init);
    const request = new Request(input, init);
    if (request.method !== 'POST') throw new Error('Invalid wallet gRPC method');
    const response = await fetchAuthenticatedRpc(
      url,
      await request.arrayBuffer(),
      request.signal,
      { 'Content-Type': 'application/grpc-web+proto', 'X-Grpc-Web': '1' },
      Capacitor.isNativePlatform() ? nativeBinaryFetch : transport
    );
    return grpcResponse(response);
  };
}

export async function submitIrisV1Transaction(signedTx: string): Promise<string> {
  await initializeIrisV1();
  installTransport();
  const converted = await handleVaultMessage({ type: 'vault:toRawTxJam', base64Jam: signedTx });
  if (!converted.success) throw new Error(converted.error);
  const jam = converted.data as string;
  const bytes = Uint8Array.from(atob(jam), value => value.charCodeAt(0));
  const raw = irisV1.rawTxV1FromNoun(irisV1.cue(bytes));
  const client = new irisV1.GrpcClient(GRPC_URL);
  try {
    // The gRPC response acknowledges submission; its SDK string is not a transaction ID.
    await client.sendTransaction(transactionProtobuf(raw));
    return raw.id;
  } finally {
    client.free();
  }
}

export function grpcResponse(response: Response): Response {
  if (response.ok && response.headers.get('content-type')?.startsWith('application/grpc-web'))
    return response;
  const message = `Transaction service unavailable (HTTP ${response.status}). Check Activity before retrying.`;
  const trailer = new TextEncoder().encode(
    `grpc-status: 14\r\ngrpc-message: ${encodeURIComponent(message)}\r\n`
  );
  const body = new Uint8Array(5 + trailer.length);
  body[0] = 128;
  new DataView(body.buffer).setUint32(1, trailer.length);
  body.set(trailer, 5);
  return new Response(body, { headers: { 'Content-Type': 'application/grpc-web+proto' } });
}
