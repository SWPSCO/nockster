import test from 'node:test';
import assert from 'node:assert/strict';

import {
  COBSEncoder,
  COBSFrameReader,
  NocksterDevice,
  PROTO_V1,
} from '../dist/index.js';

class FakeTransport {
  constructor(onWrite) {
    this.connected = false;
    this.onData = null;
    this.onWrite = onWrite;
    this.writes = [];
  }

  async connect() {
    this.connected = true;
  }

  async disconnect() {
    this.connected = false;
  }

  isConnected() {
    return this.connected;
  }

  startReading(onData) {
    this.onData = onData;
  }

  async write(data) {
    this.writes.push(new Uint8Array(data));
    if (this.onWrite) {
      await this.onWrite(data, this);
    }
  }

  emitResponse(msgId, responsePayload) {
    if (!this.onData) {
      throw new Error('reader not started');
    }
    this.onData(encodeResponse(msgId, responsePayload));
  }
}

function encodeResponse(msgId, responsePayload) {
  return COBSEncoder.encode(Uint8Array.from([PROTO_V1, msgId, ...responsePayload]));
}

function decodeWrite(data) {
  const reader = new COBSFrameReader();
  const frames = reader.push(data);
  assert.equal(frames.length, 1);
  return frames[0];
}

async function waitFor(predicate) {
  for (let i = 0; i < 20; i += 1) {
    if (predicate()) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  throw new Error('condition was not reached');
}

test('NocksterDevice.reboot sends Reboot and accepts Ok', async () => {
  const transport = new FakeTransport((_data, fake) => {
    fake.emitResponse(1, [5]); // Response::Ok
  });
  const device = new NocksterDevice(transport);

  await device.connect();
  await device.reboot();

  assert.equal(transport.writes.length, 1);
  assert.deepEqual([...decodeWrite(transport.writes[0])], [
    PROTO_V1,
    1, // msg id
    0, // Frame::One
    40, // Request::Reboot
  ]);
});

test('NocksterDevice.reboot treats disconnect after request delivery as success', async () => {
  const transport = new FakeTransport();
  const device = new NocksterDevice(transport);

  await device.connect();
  const reboot = device.reboot();
  await waitFor(() => transport.writes.length === 1);
  await device.disconnect();

  await assert.doesNotReject(() => reboot);
});

test('NocksterDevice.reboot does not hide write-side disconnects', async () => {
  const transport = new FakeTransport(() => {
    throw new Error('Device disconnected before request delivery');
  });
  const device = new NocksterDevice(transport);

  await device.connect();
  await assert.rejects(
    () => device.reboot(),
    /Device disconnected before request delivery/,
  );
});
