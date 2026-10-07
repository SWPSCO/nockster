import assert from 'node:assert/strict';
import test from 'node:test';
import { NocksterDevice, FEATURE_UPDATE_BOOT_STATUS, type SecurityStatus } from '@swps/nockster-js';
import { HardwareTransport, type NativeInvoke } from '../apps/desktop/src/hardware/transport.ts';
import { canReadBootStatus } from '../apps/desktop/src/hardware/firmwarePolicy.ts';
import { parseHex, asciiLabel, seedFromPhrase } from '../apps/desktop/src/hardware/inputs.ts';
import {
  setSigningDeviceProvider,
  withSigningDevice
} from '../packages/wallet/src/lib/utils/hardwareDevice.ts';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
const tick = () => new Promise(resolve => setImmediate(resolve));

test('cancelling an opening native session closes the returned handle', async () => {
  const opening = deferred<number>();
  const closes: unknown[] = [];
  const call = (async (command, args) => {
    if (command === 'hardware_open') return opening.promise;
    if (command === 'hardware_close') {
      closes.push(args?.session);
      return;
    }
    assert.fail(command);
  }) as NativeInvoke;
  const transport = new HardwareTransport('device', () => assert.fail('no read failure'), call);
  const connected = transport.connect();
  await transport.disconnect();
  opening.resolve(17);
  await assert.rejects(connected, /cancelled/);
  assert.deepEqual(closes, [17]);
  assert.equal(transport.isConnected(), false);
});

test('a completed read from a disconnected session never reaches the SDK', async () => {
  const reading = deferred<number[]>();
  const call = (async command => {
    if (command === 'hardware_open') return 5;
    if (command === 'hardware_read') return reading.promise;
    if (command === 'hardware_close') return;
    assert.fail(command);
  }) as NativeInvoke;
  const transport = new HardwareTransport('device', () => assert.fail('no failure'), call);
  await transport.connect();
  transport.startReading(() => assert.fail('stale bytes must be discarded'));
  await transport.disconnect();
  reading.resolve([1, 2, 3]);
  await tick();
});

test(
  'native unplug rejects pending protocol requests without waiting for an approval timeout',
  { timeout: 2000 },
  async () => {
    const reading = deferred<number[]>();
    let failures = 0;
    let closes = 0;
    const call = (async command => {
      if (command === 'hardware_open') return 7;
      if (command === 'hardware_read') return reading.promise;
      if (command === 'hardware_write') return;
      if (command === 'hardware_close') {
        closes++;
        return;
      }
      assert.fail(command);
    }) as NativeInvoke;
    const transport = new HardwareTransport(
      'device',
      () => {
        failures++;
        void device.disconnect();
      },
      call
    );
    const device = new NocksterDevice(transport);
    await device.connect();
    const pending = device.showAddress(0, [], 180000);
    reading.reject(new Error('USB disconnected'));
    await assert.rejects(pending, /disconnected/);
    await tick();
    assert.equal(failures, 1);
    assert.equal(closes, 1);
  }
);

test('native writes carry the session and transport failure is terminal', async () => {
  const writes: unknown[] = [];
  let failure = '';
  const call = (async (command, args) => {
    if (command === 'hardware_open') return 19;
    if (command === 'hardware_write') {
      writes.push(args);
      throw new Error('Write failed');
    }
    if (command === 'hardware_close') return;
    assert.fail(command);
  }) as NativeInvoke;
  const transport = new HardwareTransport('device', error => (failure = error.message), call);
  await transport.connect();
  await assert.rejects(transport.write(new Uint8Array([1, 2, 0])), /Write failed/);
  await assert.rejects(transport.write(new Uint8Array([3])), /Connect/);
  assert.deepEqual(writes, [{ session: 19, data: [1, 2, 0] }]);
  assert.equal(failure, 'Write failed');
  assert.equal(transport.isConnected(), false);
  await transport.disconnect();
});

test('desktop signing uses the registered session and preserves a rejected approval', async () => {
  let calls = 0;
  const device = new NocksterDevice();
  const clear = setSigningDeviceProvider(async (address, toAddress, action) => {
    assert.equal(address, 'my-address');
    assert.equal(toAddress([], []), 'my-address');
    calls++;
    return action(device);
  });
  try {
    await assert.rejects(
      withSigningDevice(
        'my-address',
        () => 'my-address',
        async received => {
          assert.equal(received, device);
          throw new Error('Rejected on device');
        }
      ),
      /Rejected on device/
    );
    assert.equal(calls, 1);
  } finally {
    clear();
  }
});

test('boot status reads require advertised support and flash encryption safety', () => {
  const encrypted = { flash_encryption: true } as SecurityStatus;
  const plaintext = { flash_encryption: false } as SecurityStatus;
  assert.equal(canReadBootStatus(0, 11, plaintext), false);
  for (const release of [null, 0, 10]) {
    assert.equal(canReadBootStatus(FEATURE_UPDATE_BOOT_STATUS, release, encrypted), false);
    assert.equal(canReadBootStatus(FEATURE_UPDATE_BOOT_STATUS, release, null), false);
    assert.equal(canReadBootStatus(FEATURE_UPDATE_BOOT_STATUS, release, plaintext), true);
  }
  assert.equal(canReadBootStatus(FEATURE_UPDATE_BOOT_STATUS, 11, encrypted), true);
  assert.equal(canReadBootStatus(FEATURE_UPDATE_BOOT_STATUS, 11, null), true);
});

test('secret inputs reject malformed encodings before any device operation', async () => {
  assert.throws(() => parseHex('zz'), /hexadecimal/);
  assert.throws(() => parseHex('abc'), /hexadecimal/);
  assert.deepEqual(parseHex('0xa1 b2'), new Uint8Array([161, 178]));
  assert.throws(() => asciiLabel('a\nb'), /ASCII/);
  assert.throws(() => asciiLabel('x'.repeat(33)), /ASCII/);
  await assert.rejects(seedFromPhrase('abandon '.repeat(24), ''), /valid 24-word/);
  const phrase = `${'abandon '.repeat(23)}art`;
  const a = await seedFromPhrase(phrase, 'test');
  const b = await seedFromPhrase(phrase.replaceAll(' ', '  '), 'test');
  assert.equal(a.length, 64);
  assert.deepEqual(a, b);
  a.fill(0);
  b.fill(0);
});
