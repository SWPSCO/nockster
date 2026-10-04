import { keccak_256 } from '@noble/hashes/sha3.js';
import { bytesToHex } from '@noble/hashes/utils.js';
import type { Digest, Noun } from 'iris-v1/wasm';

export const BRIDGE_LOCK_ROOT = 'AcsPkuhXQoGeEsF91yynpm1kcW17PQ2Z1MEozgx7YnDPkZwrtzLuuqd';
export const BRIDGE_MINIMUM_NICKS = 100_000n * 65_536n;
export const BRIDGE_CONFIRMATION_BLOCKS = 400;

export type BridgeDetails = {
  destination: string;
  amount: number;
  protocolFee: number;
  expectedReceived: number;
};

export function parseBaseAddress(value: string): string {
  const address = value.trim();
  if (!/^0x[0-9a-fA-F]{40}$/.test(address))
    throw new Error('Enter a Base address: 0x followed by 40 hexadecimal characters');
  const hex = address.slice(2);
  if (/^0+$/.test(hex)) throw new Error('The Base destination cannot be the zero address');
  if (hex !== hex.toLowerCase() && hex !== hex.toUpperCase()) {
    const hash = bytesToHex(keccak_256(new TextEncoder().encode(hex.toLowerCase())));
    for (let index = 0; index < hex.length; index++) {
      const expected =
        parseInt(hash[index], 16) >= 8 ? hex[index].toUpperCase() : hex[index].toLowerCase();
      if (hex[index] !== expected)
        throw new Error('Base address checksum does not match. Check the receiving address');
    }
  }
  return address.toLowerCase();
}

export function bridgeDetails(destination: string, amount: bigint): BridgeDetails {
  if (amount < BRIDGE_MINIMUM_NICKS)
    throw new Error('Bridge to Base requires at least 100,000 NOCK');
  if (amount > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error('Bridge amount is too large');
  // bridge-fee:calculate in the bridge kernel charges each started NOCK.
  // The bridge deducts this fee from the Base payout.
  const protocolFee = ((amount + 65_535n) / 65_536n) * 195n;
  return {
    destination: parseBaseAddress(destination),
    amount: Number(amount),
    protocolFee: Number(protocolFee),
    expectedReceived: Number(amount - protocolFee)
  };
}

/** Recognize actual bridge outputs; metadata on another lock is not a bridge deposit. */
export function bridgeFromOutputs(outputs: unknown): BridgeDetails | undefined {
  if (!Array.isArray(outputs)) return undefined;
  let destination: string | undefined;
  let total = 0n;
  for (const output of outputs) {
    const note = output?.note;
    const candidates =
      note?.lockScriptHash === BRIDGE_LOCK_ROOT
        ? [{ lockRoot: note.lockScriptHash, noteData: note.noteData, gift: note.assets }]
        : (output?.seeds ?? []);
    for (const seed of candidates) {
      if (seed?.lockRoot !== BRIDGE_LOCK_ROOT) continue;
      try {
        const address = parseBaseAddress(seed.noteData?.bridge ?? '');
        if (destination && destination !== address) return undefined;
        if (!Number.isSafeInteger(seed.gift) || seed.gift <= 0) return undefined;
        destination = address;
        total += BigInt(seed.gift);
      } catch {
        return undefined;
      }
    }
  }
  if (!destination) return undefined;
  try {
    return bridgeDetails(destination, total);
  } catch {
    return undefined;
  }
}

export type BridgeRecipient = { address: string; amount: number; bridgeEvmAddress?: string };

export function bridgeFromRecipients(recipients: BridgeRecipient[]): BridgeDetails | undefined {
  const deposits = recipients.filter(
    recipient => recipient.address === BRIDGE_LOCK_ROOT || recipient.bridgeEvmAddress !== undefined
  );
  if (!deposits.length) return undefined;
  if (deposits.length !== 1 || recipients.length !== 1)
    throw new Error('Bridge to Base requires one deposit and no additional recipients');
  const deposit = deposits[0];
  if (deposit.address !== BRIDGE_LOCK_ROOT || !deposit.bridgeEvmAddress)
    throw new Error('Use Bridge to Base to send to the bridge lock');
  if (!Number.isSafeInteger(deposit.amount)) throw new Error('Invalid bridge amount');
  return bridgeDetails(deposit.bridgeEvmAddress, BigInt(deposit.amount));
}

export async function verifyBridgeTransaction(
  transaction: string,
  source: string,
  recipients: BridgeRecipient[],
  fee: number
) {
  const deposit = bridgeFromRecipients(recipients);
  if (!deposit) return;
  if (!Number.isSafeInteger(fee) || fee < 0) throw new Error('Invalid network fee');
  const { ensureVaultReady } = await import('../../vault/wasm');
  const { verifyBridgeIntent, toRawTxJam } = await import('../../pkg/nockster_core.js');
  await ensureVaultReady();
  verifyBridgeIntent(
    transaction,
    source,
    recipients.map(({ address, amount, bridgeEvmAddress }) => ({
      address,
      gift: amount,
      bridgeEvmAddress
    })),
    BigInt(fee)
  );

  // Check the merged output with an independent decoder as well as the seeds.
  // The bridge processes one output note, not the sum of arbitrary deposits.
  const iris = await import('iris-v1/wasm');
  const { initializeIrisV1 } = await import('../../platform/irisV1');
  const { ZORP_BRIDGE_ADDRESSES, ZORP_BRIDGE_THRESHOLD, BYTHOS_TX_ENGINE_SETTINGS } =
    await import('iris-v1');
  await initializeIrisV1();
  const raw = iris.rawTxV1FromNoun(
    iris.cue(Uint8Array.from(atob(toRawTxJam(transaction)), c => c.charCodeAt(0)))
  );
  const lock = iris.spendConditionNewPkh(
    iris.pkhNew(BigInt(ZORP_BRIDGE_THRESHOLD), ZORP_BRIDGE_ADDRESSES as Digest[])
  );
  if (iris.lockHash(lock) !== BRIDGE_LOCK_ROOT)
    throw new Error('Bridge signer configuration does not match the deposit lock');
  const outputs = iris.rawTxV1Outputs(raw, 0, BYTHOS_TX_ENGINE_SETTINGS);
  const deposits = outputs.filter(note => note.note_data.some(([key]) => key === 'bridge'));
  if (
    deposits.length !== 1 ||
    deposits[0].name.first !== iris.spendConditionFirstName(lock) ||
    BigInt(deposits[0].assets) !== BigInt(deposit.amount) ||
    deposits[0].note_data.length !== 1
  )
    throw new Error('Bridge output does not match the reviewed deposit');
  // Hoon evm-address-to-based: three successive divisions by the field prime.
  // BigInt arithmetic checks the Rust limb implementation independently.
  const prime = 18_446_744_069_414_584_321n;
  const address = BigInt(deposit.destination);
  const cell = (head: Noun, tail: Noun): Noun => {
    // Iris types noun arrays as [Noun]; its serializer accepts ordinary cells.
    const pair: [Noun] = [head];
    pair.push(tail);
    return pair;
  };
  const expectedPayload = iris.jam(
    cell(
      '0',
      cell(
        '65736162',
        cell(
          (address % prime).toString(16),
          cell(((address / prime) % prime).toString(16), (address / (prime * prime)).toString(16))
        )
      )
    )
  );
  const actualPayload = iris.jam(deposits[0].note_data[0][1]);
  if (
    actualPayload.length !== expectedPayload.length ||
    actualPayload.some((byte, index) => byte !== expectedPayload[index])
  )
    throw new Error('Bridge output does not match the reviewed Base destination');
}
