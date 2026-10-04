import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import {
  NocksterDevice,
  PostcardReader,
  PostcardWriter,
  COBSEncoder,
  COBSFrameReader,
  PROTO_V1,
  FEATURE_CHEETAH,
  FEATURE_FRAG,
  ERR_REJECTED_BY_USER
} from '@swps/nockster-js';
import {
  hardwareAddresses,
  requireDeviceInfo,
  selectSigningWallet,
  withSigningDevice,
  isNockster,
  type DeviceInfo
} from '../packages/wallet/src/lib/utils/hardwareDevice.ts';
import { migrateHardwareWallets } from '../packages/wallet/src/lib/utils/hardwareMigration.ts';
import type { Wallet } from '../packages/wallet/src/lib/types/wallet';

const info: DeviceInfo = {
  type: 'Info',
  proto_v: PROTO_V1,
  fw_major: 1,
  fw_minor: 0,
  features: FEATURE_CHEETAH | FEATURE_FRAG,
  has_seed: true,
  cheetah_pubs: [{ slot: 3, path: [], x: [42n, 0n, 0n, 0n, 0n, 0n], y: [1n, 0n, 0n, 0n, 0n, 0n] }]
};
const addressOf = (x: string[]) => `pkh-${x[0]}`;

test('signing resolves the live root key slot and rejects locked, foreign and derived keys', async () => {
  const selected: number[] = [];
  const device = {
    getLockStatus: async () => ({
      type: 'OkLockStatus' as const,
      locked: false,
      attempts_remaining: 3
    }),
    getInfo: async () => info,
    selectSeed: async (slot: number) => {
      selected.push(slot);
      return { type: 'Ok' as const };
    }
  };
  await selectSigningWallet(device, 'pkh-42', addressOf);
  assert.deepEqual(selected, [3]);
  selected.length = 0;
  await assert.rejects(selectSigningWallet(device, 'pkh-17', addressOf), /does not contain/);
  await assert.rejects(
    selectSigningWallet(
      {
        ...device,
        getLockStatus: async () => ({ type: 'OkLockStatus', locked: true, attempts_remaining: 3 })
      },
      'pkh-42',
      addressOf
    ),
    /Unlock/
  );
  await assert.rejects(
    selectSigningWallet(
      {
        ...device,
        getInfo: async () => ({ ...info, cheetah_pubs: [{ ...info.cheetah_pubs[0], path: [1] }] })
      },
      'pkh-42',
      addressOf
    ),
    /does not contain/
  );
  assert.deepEqual(selected, []);
  assert.throws(() => requireDeviceInfo({ ...info, proto_v: 99 }), /protocol/);
  assert.throws(
    () =>
      hardwareAddresses(info, () => {
        throw new Error('invalid key');
      }),
    /invalid key/
  );
});

// A WebHID peer exercising the published SDK's COBS/Postcard framing.
class FakeHID extends EventTarget {
  vendorId = 0x303a;
  productId = 0x2001;
  opened = false;
  opens = 0;
  closes = 0;
  key = 42;
  slot = 3;
  locked = false;
  requests: number[] = [];
  frames: Uint8Array[] = [];
  reader = new COBSFrameReader();
  async open() {
    this.opened = true;
    this.opens++;
  }
  async close() {
    this.opened = false;
    this.closes++;
  }
  respond(id: number, payload: number[]) {
    const bytes = COBSEncoder.encode(Uint8Array.from([PROTO_V1, id, ...payload]));
    for (let offset = 0; offset < bytes.length; offset += 62) {
      const part = bytes.subarray(offset, offset + 62);
      const report = Uint8Array.from([part.length, ...part]);
      this.dispatchEvent(
        Object.assign(new Event('inputreport'), {
          reportId: 1,
          device: this,
          data: new DataView(report.buffer)
        })
      );
    }
  }
  async sendReport(reportId: number, data: Uint8Array) {
    assert.equal(reportId, 1);
    assert.equal(data.length, 63);
    for (const frame of this.reader.push(data.subarray(1, data[0] + 1))) {
      this.handleFrame(frame);
    }
  }
  handleFrame(frame: Uint8Array) {
    this.frames.push(frame);
    const id = frame[1];
    if (frame[2] === 1) {
      this.respond(id, [5]);
      return;
    } // FragBegin acknowledgement
    if (frame[2] === 2) {
      this.respond(id, [14, (ERR_REJECTED_BY_USER & 127) | 128, ERR_REJECTED_BY_USER >> 7]);
      return;
    }
    const request = frame[3];
    this.requests.push(request);
    if (request === 19) this.respond(id, [13, Number(this.locked), 3]);
    else if (request === 1)
      this.respond(id, [
        3,
        PROTO_V1,
        1,
        0,
        FEATURE_CHEETAH | FEATURE_FRAG,
        1,
        1,
        this.slot,
        0,
        this.key,
        0,
        0,
        0,
        0,
        0,
        1,
        0,
        0,
        0,
        0,
        0
      ]);
    else if (request === 20) {
      assert.equal(frame[4], this.slot);
      this.respond(id, [5]);
    } else throw new Error(`Unexpected request ${request}`);
  }
}

test('WebHID sessions skip a foreign device and close every connection', async t => {
  const foreign = new FakeHID();
  foreign.key = 7;
  const mine = new FakeHID();
  const original = Object.getOwnPropertyDescriptor(globalThis, 'navigator')!;
  Object.defineProperty(globalThis, 'navigator', {
    configurable: true,
    value: { hid: Object.assign(new EventTarget(), { getDevices: async () => [foreign, mine] }) }
  });
  t.after(() => Object.defineProperty(globalThis, 'navigator', original));
  assert.equal(isNockster({ vendorId: 0x303a, productId: 0x1001 } as FakeHID), false);
  const result = await withSigningDevice('pkh-42', addressOf, async device => device.isConnected());
  assert.equal(result, true);
  assert.deepEqual(foreign.requests, [19, 1]);
  assert.deepEqual(mine.requests, [19, 1, 20]);
  assert.equal(foreign.closes, 1);
  assert.equal(mine.closes, 1);
});

test('an on-device rejection ends signing without retrying another device', async t => {
  const first = new FakeHID();
  const second = new FakeHID();
  const original = Object.getOwnPropertyDescriptor(globalThis, 'navigator')!;
  Object.defineProperty(globalThis, 'navigator', {
    configurable: true,
    value: { hid: Object.assign(new EventTarget(), { getDevices: async () => [first, second] }) }
  });
  t.after(() => Object.defineProperty(globalThis, 'navigator', original));
  await assert.rejects(
    withSigningDevice('pkh-42', addressOf, device =>
      device.signDraft(new Uint8Array(400).fill(42))
    ),
    /reject/i
  );
  assert.equal(first.closes, 1);
  assert.equal(second.opens, 0);
  assert.ok(first.frames.some(frame => frame[2] === 2));
});

test('hardware migration preserves accounts, history and pending reservations', () => {
  const original: Wallet = {
    id: 'hardware-303a:2001',
    name: 'Savings',
    addresses: ['a', 'b'],
    currentAddressIndex: 0,
    createdAt: 1,
    lastUsed: 2,
    watchOnly: true,
    hardware: { transport: 'hid' },
    balance: 99,
    notes: [],
    blob: new Uint8Array([1, 2]),
    pendingTransactions: [
      {
        txId: 'tx-b',
        fromAddress: 'b',
        signedTx: 'signed',
        inputNotes: ['note'],
        recipients: [],
        totalAmount: 1,
        fee: 1,
        timestamp: 1
      }
    ]
  };
  const migrated = migrateHardwareWallets([original]);
  assert.deepEqual(
    migrated.map(w => w.addresses),
    [['a'], ['b']]
  );
  assert.equal(migrated[0].id, original.id);
  assert.equal(migrated[0].name, 'Savings');
  assert.equal(migrated[0].balance, 99);
  assert.deepEqual(migrated[0].blob, original.blob);
  assert.deepEqual(migrated[0].pendingTransactions, []);
  assert.deepEqual(migrated[1].pendingTransactions, original.pendingTransactions);
  assert.equal(migrated[1].balance, undefined);
  assert.deepEqual(migrateHardwareWallets(migrated), migrated);
  assert.deepEqual(original.addresses, ['a', 'b']);
  const existing = { ...migrated[1], name: 'Already paired' };
  const merged = migrateHardwareWallets([original, existing]);
  assert.equal(merged.length, 2);
  assert.equal(merged[1].name, 'Already paired');
  assert.equal(merged[1].pendingTransactions?.length, 1);
});

const signingFixture = JSON.parse(
  readFileSync(new URL('./fixtures/nockster-signed.json', import.meta.url), 'utf8')
);

// The peer returns firmware-produced bytes and checks every uploaded draft fragment.
class SigningHID extends FakeHID {
  uploaded: number[] = [];
  total = 0;
  guiApproval = true;
  corruptOffset = false;
  onApproval: (() => void) | undefined;

  reply(id: number, write: (writer: PostcardWriter) => void) {
    const writer = new PostcardWriter();
    write(writer);
    this.respond(id, [...writer.toBytes()]);
  }

  override handleFrame(frame: Uint8Array) {
    const reader = new PostcardReader(frame);
    assert.equal(reader.readU8(), PROTO_V1);
    const id = reader.readVarint();
    const kind = reader.readVarint();
    if (kind === 0) return super.handleFrame(frame);
    this.frames.push(frame);
    const fragId = reader.readVarint();
    if (kind === 1) {
      this.total = reader.readVarint();
      assert.equal(reader.readVarint(), 1); // FragKind::SignDraft
      this.respond(id, [5]);
      return;
    }
    assert.equal(kind, 2);
    assert.equal(reader.readVarint(), this.uploaded.length);
    this.uploaded.push(...reader.readBytes());
    if (!reader.readBool()) {
      this.respond(id, [5]);
      return;
    }
    assert.equal(this.uploaded.length, this.total);
    assert.deepEqual(Buffer.from(this.uploaded), Buffer.from(signingFixture.draft, 'base64'));
    if (this.guiApproval) this.respond(id, [5]);
    if (this.onApproval) return this.onApproval();
    const signed = Buffer.from(signingFixture.signedTx, 'base64');
    this.reply(id, writer => {
      writer.writeVarint(1); // Response::FragBegin
      writer.writeVarint(fragId);
      writer.writeVarint(signed.length);
      writer.writeVarint(1); // FragKind::SignDraft
    });
    for (let offset = 0; offset < signed.length; offset += 180) {
      const chunk = signed.subarray(offset, offset + 180);
      this.reply(id, writer => {
        writer.writeVarint(2); // Response::FragPart
        writer.writeVarint(fragId);
        writer.writeVarint(offset + (this.corruptOffset ? 1 : 0));
        writer.writeBytes(chunk);
        writer.writeBool(offset + chunk.length === signed.length);
      });
    }
  }
}

for (const guiApproval of [true, false]) {
  test(`fragmented signing round-trips the firmware transaction (${guiApproval ? 'screen approval' : 'immediate response'})`, async t => {
    const hid = new SigningHID();
    hid.guiApproval = guiApproval;
    const delays: number[] = [];
    const setTimeout = globalThis.setTimeout;
    t.mock.method(globalThis, 'setTimeout', (callback, delay, ...args) => {
      delays.push(delay);
      return setTimeout(callback, delay, ...args);
    });
    const device = new NocksterDevice();
    await device.connectHidDevice(hid);
    t.after(() => device.disconnect());
    const signed = await device.signDraft(Buffer.from(signingFixture.draft, 'base64'));
    assert.deepEqual(Buffer.from(signed), Buffer.from(signingFixture.signedTx, 'base64'));
    assert.ok(delays.includes(2), 'multi-report HID writes must be paced');
    assert.ok(hid.frames.filter(frame => frame[2] === 2).length > 1);
  });
}

test('invalid signed-response fragment offsets are rejected', async t => {
  const hid = new SigningHID();
  hid.corruptOffset = true;
  const device = new NocksterDevice();
  await device.connectHidDevice(hid);
  t.after(() => device.disconnect());
  await assert.rejects(
    device.signDraft(Buffer.from(signingFixture.draft, 'base64')),
    /fragment offset mismatch/
  );
});

test(
  'unplugging during approval cancels the request and never retries another device',
  { timeout: 2000 },
  async t => {
    const first = new SigningHID();
    const second = new FakeHID();
    const hid = Object.assign(new EventTarget(), { getDevices: async () => [first, second] });
    const original = Object.getOwnPropertyDescriptor(globalThis, 'navigator')!;
    Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { hid } });
    t.after(() => Object.defineProperty(globalThis, 'navigator', original));
    first.onApproval = () => {
      // Approval remains pending until the physical disconnect event arrives.
      setTimeout(
        () => hid.dispatchEvent(Object.assign(new Event('disconnect'), { device: first })),
        0
      );
    };
    await assert.rejects(
      withSigningDevice('pkh-42', addressOf, device =>
        device.signDraft(Buffer.from(signingFixture.draft, 'base64'))
      ),
      /Device disconnected/
    );
    assert.equal(first.closes, 1);
    assert.equal(second.opens, 0);
  }
);

test('unplugging while opening closes the connection without issuing wallet requests', async t => {
  const first = new FakeHID();
  const hid = Object.assign(new EventTarget(), { getDevices: async () => [first] });
  const original = Object.getOwnPropertyDescriptor(globalThis, 'navigator')!;
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { hid } });
  t.after(() => Object.defineProperty(globalThis, 'navigator', original));
  first.open = async () => {
    hid.dispatchEvent(Object.assign(new Event('disconnect'), { device: first }));
    first.opened = true;
  };
  await assert.rejects(
    withSigningDevice('pkh-42', addressOf, async () => assert.fail('must not sign')),
    /Device disconnected/
  );
  assert.equal(first.closes, 1);
  assert.deepEqual(first.requests, []);
});
