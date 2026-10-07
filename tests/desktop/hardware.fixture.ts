import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import {
  COBSEncoder,
  COBSFrameReader,
  PostcardReader,
  PostcardWriter,
  PROTO_V1,
  FEATURE_CHEETAH,
  FEATURE_FRAG,
  FEATURE_SEED_LABELS,
  FEATURE_SECURE_UPDATE,
  FEATURE_RELEASE_INFO,
  FEATURE_BUILD_INFO,
  FEATURE_SECURITY_STATUS,
  FEATURE_DEVICE_REBOOT,
  FEATURE_PIN_CHANGE_UI,
  FEATURE_DEVICE_ADDRESS_BOOK,
  FEATURE_PREIMAGE_VAULT,
  FEATURE_MASTER_PUBKEY_EXPORT,
  ERR_WRONG_PIN,
  ERR_REJECTED_BY_USER
} from '@swps/nockster-js';

const fixture = JSON.parse(
  readFileSync(new URL('../fixtures/nockster-signed.json', import.meta.url), 'utf8')
);
export const address = '4Lu3cSW34WPwvDkTwKh7xB6yMZrvh3bFW7w26UDJVdboxADGkr2bTnL';

// Only native USB I/O is simulated. The app uses the published SDK and real WASM.
export class HardwarePeer {
  connected = false;
  plugged = true;
  locked = true;
  generation = 0;
  opens = 0;
  requests: number[] = [];
  label = 'Hardware savings';
  slot = 0;
  hasSeed = true;
  holdApproval = false;
  approvalId: number | null = null;
  frames = new COBSFrameReader();
  received: number[] = [];
  guard: boolean[] = [];
  rejectManifest = false;
  updateActive = false;
  updateSize = 0;
  updateRelease = 0;
  updateBytes: number[] = [];
  updateVerified = false;

  invoke = async (command: string, args: any): Promise<unknown> => {
    if (command === 'hardware_devices')
      return this.plugged ? [{ id: 'synthetic-usb', name: 'Nockster', serial: 'TEST-0001' }] : [];
    if (command === 'hardware_open') {
      assert.equal(args.id, 'synthetic-usb');
      this.connected = true;
      this.opens++;
      return ++this.generation;
    }
    if (command === 'hardware_close') {
      if (args.session === this.generation) this.connected = false;
      return;
    }
    if (command === 'hardware_protect_update') {
      this.guard.push(args.active);
      return;
    }
    if (!this.connected || !this.plugged) throw new Error('Nockster disconnected');
    assert.equal(args.session, this.generation);
    if (command === 'hardware_write') {
      for (const frame of this.frames.push(new Uint8Array(args.data))) this.handle(frame);
      return;
    }
    if (command === 'hardware_read') {
      if (!this.received.length) await new Promise(resolve => setTimeout(resolve, 5));
      return this.received.splice(0, 62);
    }
    throw new Error(`Unexpected native hardware command ${command}`);
  };

  reply(id: number, variant: number, body: (writer: PostcardWriter) => void = () => {}) {
    const w = new PostcardWriter();
    w.writeU8(PROTO_V1);
    w.writeVarint(id);
    w.writeVarint(variant);
    body(w);
    this.received.push(...COBSEncoder.encode(w.toBytes()));
  }
  rejectApproval() {
    assert.notEqual(this.approvalId, null);
    this.reply(this.approvalId!, 14, w => w.writeVarint(ERR_REJECTED_BY_USER));
    this.approvalId = null;
  }
  updateReply(id: number) {
    this.reply(id, 20, w => {
      w.writeBool(this.updateActive);
      w.writeBool(this.updateSize > 0);
      w.writeBool(this.updateVerified);
      w.writeVarint(this.updateRelease);
      w.writeVarint(this.updateBytes.length);
      w.writeVarint(this.updateSize);
    });
  }
  handle(frame: Uint8Array) {
    const r = new PostcardReader(frame);
    assert.equal(r.readU8(), PROTO_V1);
    const id = r.readVarint();
    const kind = r.readVarint();
    if (kind === 1) {
      this.reply(id, 5);
      return;
    }
    if (kind === 2) {
      this.reply(id, 14, w => w.writeVarint(ERR_REJECTED_BY_USER));
      return;
    }
    assert.equal(kind, 0);
    const request = r.readVarint();
    this.requests.push(request);
    if (request === 19)
      this.reply(id, 13, w => {
        w.writeBool(this.locked);
        w.writeU8(3);
      });
    else if (request === 16) {
      if (r.readString() !== '123456') this.reply(id, 14, w => w.writeVarint(ERR_WRONG_PIN));
      else {
        this.locked = false;
        this.reply(id, 5);
      }
    } else if (request === 17) {
      this.locked = true;
      this.reply(id, 5);
    } else if (request === 1)
      this.reply(id, 3, w => {
        w.writeU8(PROTO_V1);
        w.writeVarint(1);
        w.writeVarint(0);
        w.writeVarint(
          FEATURE_CHEETAH |
            FEATURE_FRAG |
            FEATURE_SEED_LABELS |
            FEATURE_SECURE_UPDATE |
            FEATURE_RELEASE_INFO |
            FEATURE_BUILD_INFO |
            FEATURE_SECURITY_STATUS |
            FEATURE_DEVICE_REBOOT |
            FEATURE_PIN_CHANGE_UI |
            FEATURE_DEVICE_ADDRESS_BOOK |
            FEATURE_PREIMAGE_VAULT |
            FEATURE_MASTER_PUBKEY_EXPORT
        );
        w.writeBool(this.hasSeed);
        w.writeVarint(this.hasSeed ? 1 : 0);
        if (this.hasSeed) {
          w.writeU8(this.slot);
          w.writeVarint(0);
          for (const limb of [...fixture.publicKey.x, ...fixture.publicKey.y])
            w.writeU64Varint(BigInt(limb));
        }
      });
    else if (request === 27)
      this.reply(id, 18, w => {
        w.writeVarint(this.hasSeed ? 1 : 0);
        if (this.hasSeed) {
          w.writeU8(this.slot);
          w.writeString(this.label);
        }
      });
    else if (request === 28) {
      assert.equal(r.readU8(), this.slot);
      this.label = r.readString();
      this.reply(id, 5);
    } else if (request === 23)
      this.reply(id, 16, w => {
        w.writeString('synthetic-test-commit');
        w.writeBool(false);
        w.writeString('production');
        w.writeU8(PROTO_V1);
        w.writeString('synthetic-test-types');
      });
    else if (request === 38) this.reply(id, 21, w => w.writeVarint(11));
    else if (request === 22)
      this.reply(id, 15, w => {
        w.writeBool(true);
        w.writeFixedBytes(new Uint8Array(6));
        w.writeBool(true);
        w.writeU8(1);
        w.writeBool(true);
        w.writeVarint(1);
        w.writeFixedBytes(new Uint8Array(6));
        w.writeU8(1);
        w.writeU8(1);
        w.writeU8(1);
        w.writeBool(true);
        w.writeBool(true);
        w.writeBool(true);
        w.writeU8(1);
        for (let i = 0; i < 9; i++) w.writeBool(true);
        w.writeU8(1);
        w.writeU8(1);
      });
    else if (request === 31)
      this.reply(id, 19, w => {
        w.writeBool(true);
        w.writeFixedBytes(new Uint8Array(32));
      });
    else if (request === 32) {
      if (this.rejectManifest) this.reply(id, 14, w => w.writeVarint(ERR_REJECTED_BY_USER));
      else this.reply(id, 5);
    } else if (request === 37) this.updateReply(id);
    else if (request === 33) {
      r.readU8();
      this.updateRelease = r.readVarint();
      this.updateSize = r.readVarint();
      r.readFixedBytes(32);
      r.readFixedBytes(32);
      r.readString();
      r.readString();
      r.readU8();
      r.readString();
      r.readString();
      r.readFixedBytes(64);
      r.readBytes();
      assert.equal(r.readBool(), true);
      this.updateActive = true;
      this.updateBytes = [];
      this.updateVerified = false;
      this.updateReply(id);
    } else if (request === 34) {
      assert.equal(r.readVarint(), this.updateBytes.length);
      this.updateBytes.push(...r.readBytes());
      this.updateReply(id);
    } else if (request === 35) {
      this.updateActive = false;
      this.updateVerified = true;
      this.updateReply(id);
    } else if (request === 36) {
      this.updateActive = false;
      this.reply(id, 5);
    } else if (request === 41) this.reply(id, 23, w => w.writeVarint(0));
    else if (request === 43) this.reply(id, 24, w => w.writeVarint(0));
    else if (request === 47) {
      assert.equal(r.readU8(), this.slot);
      assert.equal(r.readVarint(), 0);
      if (this.holdApproval) this.approvalId = id;
      else this.reply(id, 5);
    } else if (request === 48) {
      assert.equal(r.readU8(), this.slot);
      assert.equal(r.readVarint(), 0);
      assert.ok(r.readBytes().length > 0);
      this.approvalId = id;
    } else if (request === 13) {
      assert.ok(r.readString().length > 0);
      assert.equal(r.readFixedBytes(64).length, 64);
      this.hasSeed = true;
      this.locked = false;
      this.reply(id, 5);
    } else if (request === 20) {
      assert.equal(r.readU8(), this.slot);
      this.reply(id, 5);
    } else if (request === 2) this.reply(id, 4);
    else if ([15, 21].includes(request)) {
      this.hasSeed = false;
      this.locked = false;
      this.reply(id, 5);
    } else throw new Error(`Unexpected protocol request ${request}`);
  }
}
