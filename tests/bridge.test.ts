import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  bridgeDetails,
  bridgeFromRecipients,
  bridgeFromOutputs,
  parseBaseAddress,
  BRIDGE_LOCK_ROOT,
  BRIDGE_MINIMUM_NICKS
} from '../packages/wallet/src/lib/utils/bridge.ts';
const destination = '0xB74f4D97d40f49EafF06c3a9E97907362C9063Fa';
test('Base recipients preserve all 160 bits and validate mixed-case checksums', () => {
  assert.equal(parseBaseAddress(destination), destination.toLowerCase());
  assert.equal(
    parseBaseAddress('0x0000000000000000000000000000000000000001'),
    '0x0000000000000000000000000000000000000001'
  );
  for (const address of [
    '',
    '0x' + '0'.repeat(40),
    '0x' + 'f'.repeat(39),
    destination.replace('B7', 'b7'),
    '0x' + 'g'.repeat(40)
  ])
    assert.throws(() => parseBaseAddress(address));
});
test('protocol fee follows the bridge kernel ceiling at whole and fractional NOCK boundaries', () => {
  assert.throws(() => bridgeDetails(destination, BRIDGE_MINIMUM_NICKS - 1n));
  const quote = bridgeDetails(destination, BRIDGE_MINIMUM_NICKS);
  assert.equal(quote.protocolFee, 19_500_000);
  assert.equal(quote.expectedReceived, 6_534_100_000);
  assert.equal(bridgeDetails(destination, BRIDGE_MINIMUM_NICKS + 1n).protocolFee, 19_500_195);
  assert.equal(bridgeDetails(destination, BRIDGE_MINIMUM_NICKS + 65_535n).protocolFee, 19_500_195);
  assert.equal(bridgeDetails(destination, BRIDGE_MINIMUM_NICKS + 65_536n).protocolFee, 19_500_195);
  assert.equal(bridgeDetails(destination, BRIDGE_MINIMUM_NICKS + 65_537n).protocolFee, 19_500_390);
  assert.throws(() => bridgeDetails(destination, BigInt(Number.MAX_SAFE_INTEGER) + 1n));
});
test('history requires the canonical lock, combines split seeds, and rejects inconsistent metadata', () => {
  const seed = {
    lockRoot: BRIDGE_LOCK_ROOT,
    gift: Number(BRIDGE_MINIMUM_NICKS / 2n),
    noteData: { bridge: destination }
  };
  assert.equal(
    bridgeFromOutputs([{ seeds: [seed] }, { seeds: [seed] }])?.amount,
    Number(BRIDGE_MINIMUM_NICKS)
  );
  assert.equal(bridgeFromOutputs([{ seeds: [{ ...seed, lockRoot: 'another lock' }] }]), undefined);
  assert.equal(
    bridgeFromOutputs([
      { seeds: [seed, { ...seed, noteData: { bridge: '0x' + '1'.repeat(40) } }] }
    ]),
    undefined
  );
  assert.equal(
    bridgeFromOutputs([{ seeds: [{ ...seed, gift: Number.MAX_SAFE_INTEGER + 1 }] }]),
    undefined
  );
});

test('bridge recipients cannot disguise ordinary payments or add another fee recipient', () => {
  const deposit = {
    address: BRIDGE_LOCK_ROOT,
    amount: Number(BRIDGE_MINIMUM_NICKS),
    bridgeEvmAddress: destination
  };
  assert.equal(bridgeFromRecipients([deposit])?.destination, destination.toLowerCase());
  assert.throws(() => bridgeFromRecipients([{ ...deposit, address: 'wrong' }]));
  assert.throws(() => bridgeFromRecipients([{ ...deposit, bridgeEvmAddress: undefined }]));
  assert.throws(() => bridgeFromRecipients([deposit, { address: 'fee', amount: 1 }]));
  assert.equal(bridgeFromRecipients([{ address: 'ordinary', amount: 1 }]), undefined);
});
test('history counts authenticated merged output notes once even when seeds are present', () => {
  const value = Number(BRIDGE_MINIMUM_NICKS);
  assert.equal(
    bridgeFromOutputs([
      {
        note: {
          lockScriptHash: BRIDGE_LOCK_ROOT,
          assets: value,
          noteData: { bridge: destination }
        },
        seeds: [{ lockRoot: BRIDGE_LOCK_ROOT, gift: value, noteData: { bridge: destination } }]
      }
    ])?.amount,
    value
  );
});
