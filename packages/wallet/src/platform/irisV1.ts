import * as irisV1 from 'iris-v1/wasm';
import bs58 from 'bs58';
import irisWasmUrl from '@nockbox/iris-wasm/iris_wasm_bg.wasm?url';

let ready: Promise<unknown> | undefined;
export async function initializeIrisV1() {
  await (ready ??= irisV1.initWasm({ module_or_path: irisWasmUrl }).catch(error => {
    ready = undefined;
    throw error;
  }));
}

// Use the saved Nockster key directly; deriving another account changes the login identity.
export function signIrisV1(privateKey: string, address: string, message: string) {
  const bytes = bs58.decode(privateKey);
  let key: irisV1.PrivateKey | undefined;
  try {
    if (bytes.length !== 32) throw new Error('Invalid signing key');
    key = irisV1.PrivateKey.fromBytes(bytes);
    const publicKey = key.publicKey;
    if (irisV1.hashPublicKey(publicKey) !== address) throw new Error('Signing address mismatch');
    const signature = irisV1.signMessage(bytes, message);
    if (!irisV1.verifySignature(publicKey, signature, message))
      throw new Error('Invalid login signature');
    return { signature, public_key: bs58.encode(publicKey) };
  } finally {
    bytes.fill(0);
    key?.free();
  }
}

// Preserve the proof variant when translating the signed noun to protobuf.
export function transactionProtobuf(raw: irisV1.RawTxV1): irisV1.PbCom2RawTransaction {
  const protobuf = irisV1.rawTxToProtobuf(raw);
  for (const entry of protobuf.spends) {
    const original = raw.spends.find(
      ([name]) => name.first === entry.name?.first && name.last === entry.name?.last
    )?.[1];
    if (!original) throw new Error('Transaction input is missing from the signed noun');
    if (original.tag !== 1) continue;
    const kind = entry.spend?.spend_kind;
    const proof = kind && 'Witness' in kind ? kind.Witness.witness?.lock_merkle_proof : undefined;
    if (!proof) throw new Error('Transaction witness is missing');
    const lock = original.witness.lock_merkle_proof;
    proof.lmp_version = 'version' in lock && lock.version === 'full' ? 0x6c6c7566 : undefined;
  }
  return protobuf;
}
