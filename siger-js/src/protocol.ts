import { PostcardReader, PostcardWriter } from './postcard';

/**
 * Protocol types matching siger-core/src/lib.rs
 */

export type Frame =
  | { type: 'One'; request: Request };

export type Request =
  | { type: 'Hello' }
  | { type: 'GetInfo' }
  | { type: 'Ping' }
  | { type: 'Wipe' }
  | { type: 'SetSeed'; seed64: Uint8Array }
  | { type: 'GetFingerprint' }
  | { type: 'GetPubkey'; path: number[]; compressed?: boolean }
  | { type: 'GetXpub'; path: number[] }
  | { type: 'SignDigest'; path: number[]; digest32: Uint8Array }
  | { type: 'GetCheetahPub'; slot: number; path: number[] }
  | { type: 'SignSpendHash'; slot: number; path: number[]; msg5: bigint[] }
  | {
      type: 'SignSpendHashFor';
      slot: number;
      path: number[];
      msg5: bigint[];
      pubkey: { x: bigint[]; y: bigint[] };
    }
  | { type: 'Health' }
  | { type: 'InitializePIN'; pin: string; seed64: Uint8Array }
  | { type: 'AddSeed'; seed64: Uint8Array }
  | { type: 'DeleteSeed'; slot: number }
  | { type: 'Unlock'; pin: string }
  | { type: 'Lock' }
  | { type: 'ResetPIN'; current_pin: string; new_pin: string }
  | { type: 'GetLockStatus' }
  | { type: 'SelectSeed'; slot: number }
  | { type: 'Reset' };

export interface CheetahPubInfo {
  slot: number;
  path: number[];
  x: bigint[];
  y: bigint[];
}

export type Response =
  | { type: 'Hello'; proto_v: number; compressed_pk: boolean }
  | { type: 'Info'; proto_v: number; fw_major: number; fw_minor: number; features: number; has_seed: boolean; cheetah_pubs: CheetahPubInfo[] }
  | { type: 'Pong' }
  | { type: 'Ok' }
  | { type: 'OkLockStatus'; locked: boolean; attempts_remaining: number }
  | { type: 'OkCheetahPub'; x: bigint[]; y: bigint[] }
  | { type: 'OkCheetahSig'; chal: bigint[]; sig: bigint[] }
  | { type: 'Err'; code: number };

export interface Msg<T> {
  v: number;
  id: number;
  msg: T;
}

// Error codes from siger-core
export const ERR_BAD_COBS_OR_POSTCARD = 100;
export const ERR_OVERFLOW = 102;
export const ERR_ENCODE_TOO_BIG = 103;
export const ERR_UNSUPPORTED_VERSION = 110;
export const ERR_NO_SEED = 120;
export const ERR_WRONG_PUBKEY = 0x0103;
export const ERR_DEVICE_LOCKED = 130;
export const ERR_WRONG_PIN = 131;
export const ERR_PIN_LOCKED_OUT = 132;
export const ERR_ALREADY_INITIALIZED = 133;

export const PROTO_V1 = 1;

/**
 * Serialize a Request to bytes using Postcard format
 */
export function serializeRequest(req: Request): Uint8Array {
  const w = new PostcardWriter();

  switch (req.type) {
    case 'Hello':
      w.writeVarint(0);
      break;
    case 'GetInfo':
      w.writeVarint(1);
      break;
    case 'Ping':
      w.writeVarint(2);
      break;
    case 'Wipe':
      w.writeVarint(3);
      break;
    case 'SetSeed':
      w.writeVarint(4);
      w.writeFixedBytes(req.seed64);
      break;
    case 'GetFingerprint':
      w.writeVarint(5);
      break;
    case 'GetPubkey':
      w.writeVarint(6);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeU32(p);
      }
      w.writeBool(req.compressed ?? false);
      break;
    case 'GetXpub':
      w.writeVarint(7);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeU32(p);
      }
      break;
    case 'SignDigest':
      w.writeVarint(8);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeU32(p);
      }
      w.writeFixedBytes(req.digest32);
      break;
    case 'GetCheetahPub':
      w.writeVarint(9);
      w.writeU8(req.slot);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeU32(p);
      }
      break;
    case 'SignSpendHash':
      w.writeVarint(10);
      w.writeU8(req.slot);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeU32(p);
      }
      for (const val of req.msg5) {
        w.writeU64Varint(val);
      }
      break;
    case 'SignSpendHashFor':
      w.writeVarint(11);
      w.writeU8(req.slot);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeU32(p);
      }
      for (const val of req.msg5) {
        w.writeU64Varint(val);
      }
      for (const limb of req.pubkey.x) {
        w.writeU64Varint(limb);
      }
      for (const limb of req.pubkey.y) {
        w.writeU64Varint(limb);
      }
      break;
    case 'Health':
      w.writeVarint(12);
      break;
    case 'InitializePIN':
      w.writeVarint(13);
      w.writeString(req.pin);
      w.writeFixedBytes(req.seed64);
      break;
    case 'AddSeed':
      w.writeVarint(14);
      w.writeFixedBytes(req.seed64);
      break;
    case 'DeleteSeed':
      w.writeVarint(15);
      w.writeU8(req.slot);
      break;
    case 'Unlock':
      w.writeVarint(16);
      w.writeString(req.pin);
      break;
    case 'Lock':
      w.writeVarint(17);
      break;
    case 'ResetPIN':
      w.writeVarint(18);
      w.writeString(req.current_pin);
      w.writeString(req.new_pin);
      break;
    case 'GetLockStatus':
      w.writeVarint(19);
      break;
    case 'SelectSeed':
      w.writeVarint(20);
      w.writeU8(req.slot);
      break;
    case 'Reset':
      w.writeVarint(21);
      break;
  }

  return w.toBytes();
}



/**
 * Deserialize a Response from bytes using Postcard format
 */
export function deserializeResponse(data: Uint8Array): Response {
  const r = new PostcardReader(data);
  const variant = r.readVarint();

  switch (variant) {
    case 0: { // Hello(Caps)
      const proto_v = r.readU8();
      const compressed_pk = r.readBool();
      return { type: 'Hello', proto_v, compressed_pk };
    }
    case 3: { // Info
      const proto_v = r.readU8();
      const fw_major = r.readVarint(); // postcard encodes u16 as varint
      const fw_minor = r.readVarint(); // postcard encodes u16 as varint
      const features = r.readVarint(); // postcard encodes u32 as varint
      const has_seed = r.readBool();
      const keyCount = r.readVarint();
      const cheetah_pubs: CheetahPubInfo[] = [];
      for (let i = 0; i < keyCount; i++) {
        const slot = r.readU8();
        const pathLen = r.readVarint();
        const path: number[] = [];
        for (let j = 0; j < pathLen; j++) {
          path.push(r.readU32());
        }
        const x = r.readU64Array(6);
        const y = r.readU64Array(6);
        cheetah_pubs.push({ slot, path, x, y });
      }
      return { type: 'Info', proto_v, fw_major, fw_minor, features, has_seed, cheetah_pubs };
    }
    case 4: // Pong
      return { type: 'Pong' };
    case 5: // Ok
      return { type: 'Ok' };
    case 11: { // OkCheetahPub
      const x = r.readU64Array(6);
      const y = r.readU64Array(6);
      return { type: 'OkCheetahPub', x, y };
    }
    case 12: { // OkCheetahSig
      const chal = r.readU64Array(8);
      const sig = r.readU64Array(8);
      return { type: 'OkCheetahSig', chal, sig };
    }
    case 13: { // OkLockStatus
      const locked = r.readBool();
      const attempts_remaining = r.readU8();
      return { type: 'OkLockStatus', locked, attempts_remaining };
    }
    case 14: { // Err
      const code = r.readU16();
      return { type: 'Err', code };
    }
    default:
      throw new Error(`Unknown response variant: ${variant}`);
  }
}

/**
 * Serialize a Msg<Frame> where Frame wraps Request
 */
export function serializeMsg(msg: Msg<Request>): Uint8Array {
  const w = new PostcardWriter();
  w.writeU8(msg.v);
  w.writeVarint(msg.id); // u32 is serialized as varint in postcard

  // Frame enum: 0 = One, 1 = FragBegin, 2 = FragPart
  w.writeVarint(0); // Frame::One

  const req = msg.msg;
  switch (req.type) {
    case 'Hello':
      w.writeVarint(0);
      break;
    case 'GetInfo':
      w.writeVarint(1);
      break;
    case 'Ping':
      w.writeVarint(2);
      break;
    case 'Wipe':
      w.writeVarint(3);
      break;
    case 'SetSeed':
      w.writeVarint(4);
      w.writeFixedBytes(req.seed64);
      break;
    case 'GetFingerprint':
      w.writeVarint(5);
      break;
    case 'GetPubkey':
      w.writeVarint(6);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeU32(p);
      }
      w.writeBool(req.compressed ?? false);
      break;
    case 'GetXpub':
      w.writeVarint(7);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeU32(p);
      }
      break;
    case 'SignDigest':
      w.writeVarint(8);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeU32(p);
      }
      w.writeFixedBytes(req.digest32);
      break;
    case 'GetCheetahPub':
      w.writeVarint(9);
      w.writeU8(req.slot);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeU32(p);
      }
      break;
    case 'SignSpendHash':
      w.writeVarint(10);
      w.writeU8(req.slot);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeU32(p);
      }
      for (const val of req.msg5) {
        w.writeU64Varint(val);
      }
      break;
    case 'SignSpendHashFor':
      w.writeVarint(11);
      w.writeU8(req.slot);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeU32(p);
      }
      for (const val of req.msg5) {
        w.writeU64Varint(val);
      }
      for (const limb of req.pubkey.x) {
        w.writeU64Varint(limb);
      }
      for (const limb of req.pubkey.y) {
        w.writeU64Varint(limb);
      }
      break;
    case 'Health':
      w.writeVarint(12);
      break;
    case 'InitializePIN':
      w.writeVarint(13);
      w.writeString(req.pin);
      w.writeFixedBytes(req.seed64);
      break;
    case 'AddSeed':
      w.writeVarint(14);
      w.writeFixedBytes(req.seed64);
      break;
    case 'DeleteSeed':
      w.writeVarint(15);
      w.writeU8(req.slot);
      break;
    case 'Unlock':
      w.writeVarint(16);
      w.writeString(req.pin);
      break;
    case 'Lock':
      w.writeVarint(17);
      break;
    case 'ResetPIN':
      w.writeVarint(18);
      w.writeString(req.current_pin);
      w.writeString(req.new_pin);
      break;
    case 'GetLockStatus':
      w.writeVarint(19);
      break;
    case 'SelectSeed':
      w.writeVarint(20);
      w.writeU8(req.slot);
      break;
    case 'Reset':
      w.writeVarint(21);
      break;
  }

  return w.toBytes();
}



/**
 * Deserialize a Msg<Response>
 */
export function deserializeMsg(data: Uint8Array): Msg<Response> {
  const r = new PostcardReader(data);
  const v = r.readU8();
  const id = r.readVarint(); // u32 is serialized as varint in postcard

  // Rest is the response (no Frame wrapper on responses)
  const remaining = data.slice(r['offset']);
  const msg = deserializeResponse(remaining);

  return { v, id, msg };
}

/**
 * Get human-readable error message
 */
export function getErrorMessage(code: number): string {
  switch (code) {
    case ERR_BAD_COBS_OR_POSTCARD:
      return 'Invalid message format';
    case ERR_OVERFLOW:
      return 'Seed storage full';
    case ERR_ENCODE_TOO_BIG:
      return 'Message too large';
    case ERR_UNSUPPORTED_VERSION:
      return 'Unsupported protocol version';
    case ERR_NO_SEED:
      return 'Device not initialized';
    case ERR_DEVICE_LOCKED:
      return 'Device is locked';
    case ERR_WRONG_PIN:
      return 'Wrong PIN';
    case ERR_PIN_LOCKED_OUT:
      return 'Device locked out (too many failed attempts)';
    case ERR_ALREADY_INITIALIZED:
      return 'Device already initialized';
    case ERR_WRONG_PUBKEY:
      return 'Wrong public key';
    default:
      return `Error code: 0x${code.toString(16).padStart(4, '0')}`;
  }
}
