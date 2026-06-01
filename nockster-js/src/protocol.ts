import { PostcardReader, PostcardWriter } from './postcard.js';

/**
 * Protocol types matching nockster-core/src/lib.rs
 */

export type Frame =
  | { type: 'One'; request: Request }
  | { type: 'FragBegin'; id: number; total_len: number; kind: FragKind }
  | { type: 'FragPart'; id: number; offset: number; chunk: Uint8Array; last: boolean };

export type FragKind = 'SetSeed' | 'SignDraft';

export interface SpendOutputMeta {
  gift: bigint;
  recipient_pkh_b58: string;
  is_refund?: boolean;
}

export interface SpendMeta {
  outputs: SpendOutputMeta[];
}

export interface Xpub {
  depth: number;
  fp4: Uint8Array;
  child: number;
  chain_code: Uint8Array;
  pubkey33: Uint8Array;
}

export interface SecurityStatus {
  chip_security_available: boolean;
  mac: Uint8Array;
  flash_encryption: boolean;
  flash_crypt_cnt: number;
  secure_boot: boolean;
  secure_version: number;
  key_purposes: Uint8Array;
  hmac_key_slots: number;
  hmac_user_key_slots: number;
  read_protected_key_slots: number;
  pad_jtag_disabled: boolean;
  usb_jtag_disabled: boolean;
  soft_jtag_disabled: boolean;
  soft_jtag_disable_bits: number;
  usb_serial_jtag_disabled: boolean;
  download_mode_disabled: boolean;
  usb_serial_jtag_download_disabled: boolean;
  usb_otg_download_disabled: boolean;
  secure_download_enabled: boolean;
  direct_boot_disabled: boolean;
  usb_rom_print_disabled: boolean;
  power_glitch_enabled: boolean;
  nvs_initialized: boolean;
  nvs_schema_version: number;
  nvs_slot_count: number;
}

export interface BuildInfo {
  git_commit: string;
  git_dirty: boolean;
  build_profile: string;
  protocol_v: number;
  tx_types_rev: string;
}

export interface ReleaseInfo {
  release_version: number;
}

export interface TouchCalibration {
  raw_x_min: number;
  raw_x_max: number;
  raw_y_min: number;
  raw_y_max: number;
  mirror_x: boolean;
  mirror_y: boolean;
}

export interface SeedSlotLabel {
  slot: number;
  label: string;
}

export interface UpdateTrust {
  configured: boolean;
  pubkey_sha256: Uint8Array;
}

export interface UpdateStatus {
  active: boolean;
  manifest_verified: boolean;
  image_verified: boolean;
  release_version: number;
  bytes_received: number;
  image_size: number;
}

export interface UpdateBootStatus {
  partition_table_ok: boolean;
  ota_data_present: boolean;
  ota0_present: boolean;
  ota1_present: boolean;
  current_slot: number;
  next_slot: number;
  ota_state: number;
  ota0_offset: number;
  ota0_size: number;
  ota1_offset: number;
  ota1_size: number;
}

export interface UpdateManifest {
  manifest_version: number;
  release_version: number;
  image_size: number;
  image_sha256: Uint8Array;
  signing_pubkey_sha256: Uint8Array;
  hardware_target: string;
  build_profile: string;
  protocol_v: number;
  git_commit: string;
  tx_types_rev: string;
}

export interface UpdateBundle {
  format: string;
  signature_scheme: string;
  manifest: UpdateManifest;
  signing_pubkey_sec1: Uint8Array;
  signature64: Uint8Array;
}

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
  | { type: 'SignSpendHash'; slot: number; path: number[]; msg5: bigint[]; meta?: SpendMeta }
  | {
      type: 'SignSpendHashFor';
      slot: number;
      path: number[];
      msg5: bigint[];
      pubkey: { x: bigint[]; y: bigint[] };
      meta?: SpendMeta;
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
  | { type: 'Reset' }
  | { type: 'GetSecurityStatus' }
  | { type: 'GetBuildInfo' }
  | { type: 'GetTouchCalibration' }
  | { type: 'SetTouchCalibration'; calibration: TouchCalibration }
  | { type: 'ShowTouchDiagnostics'; enabled: boolean }
  | { type: 'GetSeedLabels' }
  | { type: 'SetSeedLabel'; slot: number; label: string }
  | { type: 'ChangePinOnDevice'; current_pin: string }
  | { type: 'StartTouchCalibration' }
  | { type: 'GetUpdateTrust' }
  | { type: 'VerifyUpdateManifest'; manifest: UpdateManifest; signature64: Uint8Array; signing_pubkey_sec1: Uint8Array }
  | { type: 'BeginUpdate'; manifest: UpdateManifest; signature64: Uint8Array; signing_pubkey_sec1: Uint8Array; write_flash: boolean }
  | { type: 'UpdateChunk'; offset: number; chunk: Uint8Array }
  | { type: 'FinishUpdate' }
  | { type: 'CancelUpdate' }
  | { type: 'GetUpdateStatus' }
  | { type: 'GetReleaseInfo' }
  | { type: 'GetUpdateBootStatus' };

export interface CheetahPubInfo {
  slot: number;
  path: number[];
  x: bigint[];
  y: bigint[];
}

export type Response =
  | { type: 'Hello'; proto_v: number; compressed_pk: boolean }
  | { type: 'FragBegin'; id: number; total_len: number; kind: FragKind }
  | { type: 'FragPart'; id: number; offset: number; chunk: Uint8Array; last: boolean }
  | { type: 'Info'; proto_v: number; fw_major: number; fw_minor: number; features: number; has_seed: boolean; cheetah_pubs: CheetahPubInfo[] }
  | { type: 'Pong' }
  | { type: 'Ok' }
  | { type: 'OkSig'; sig64: Uint8Array }
  | { type: 'OkFingerprint'; fp4: Uint8Array }
  | { type: 'OkPubkey'; uncompressed: Uint8Array }
  | { type: 'OkPubkeyCompressed'; compressed: Uint8Array }
  | { type: 'OkXpub'; xpub: Xpub }
  | { type: 'OkLockStatus'; locked: boolean; attempts_remaining: number }
  | { type: 'OkCheetahPub'; x: bigint[]; y: bigint[] }
  | { type: 'OkCheetahSig'; chal: bigint[]; sig: bigint[] }
  | { type: 'OkSecurityStatus'; status: SecurityStatus }
  | { type: 'OkBuildInfo'; info: BuildInfo }
  | { type: 'OkTouchCalibration'; calibration: TouchCalibration }
  | { type: 'OkSeedLabels'; labels: SeedSlotLabel[] }
  | { type: 'OkUpdateTrust'; trust: UpdateTrust }
  | { type: 'OkUpdateStatus'; status: UpdateStatus }
  | { type: 'OkReleaseInfo'; info: ReleaseInfo }
  | { type: 'OkUpdateBootStatus'; status: UpdateBootStatus }
  | { type: 'Err'; code: number };

export interface Msg<T> {
  v: number;
  id: number;
  msg: T;
}

// Error codes from nockster-core
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
export const ERR_REJECTED_BY_USER = 134;
export const ERR_BUSY = 135;
export const ERR_PIN_MISMATCH = 136;
export const ERR_FLASH = 140;
export const ERR_CRYPTO = 141;

export const PROTO_V1 = 1;
export const FEATURE_SECURITY_STATUS = 1 << 3;
export const FEATURE_BUILD_INFO = 1 << 4;
export const FEATURE_SECURE_UPDATE = 1 << 10;
export const FEATURE_RELEASE_INFO = 1 << 11;
export const FEATURE_UPDATE_BOOT_STATUS = 1 << 12;
export const UPDATE_BUNDLE_FORMAT = 'nockster-update-bundle-v1';
export const UPDATE_SIGNATURE_SCHEME = 'secp256k1-ecdsa-sha256-prehash-v1';
export const MAX_UPDATE_IMAGE_SIZE = 4 * 1024 * 1024;
export const MAX_UPDATE_CHUNK_LEN = 512;

function serializeFragKind(w: PostcardWriter, kind: FragKind) {
  // nockster-core::FragKind: SetSeed=0, SignDraft=1
  switch (kind) {
    case 'SetSeed':
      w.writeVarint(0);
      break;
    case 'SignDraft':
      w.writeVarint(1);
      break;
  }
}

function deserializeFragKind(kind: number): FragKind {
  switch (kind) {
    case 0:
      return 'SetSeed';
    case 1:
      return 'SignDraft';
    default:
      throw new Error(`Unknown frag kind: ${kind}`);
  }
}

function serializeSpendMetaOptional(w: PostcardWriter, meta: SpendMeta | undefined) {
  if (!meta || !Array.isArray(meta.outputs) || meta.outputs.length === 0) {
    return;
  }
  // Option::Some
  w.writeVarint(1);
  // SpendMeta { outputs: Vec<SpendOutputMeta> }
  w.writeVarint(meta.outputs.length);
  for (const out of meta.outputs) {
    w.writeU64Varint(out.gift);
    w.writeString(out.recipient_pkh_b58);
    w.writeBool(out.is_refund ?? false);
  }
}

function expectBytes(value: Uint8Array, len: number, label: string): Uint8Array {
  if (!(value instanceof Uint8Array) || value.length !== len) {
    throw new Error(`${label} must be ${len} bytes`);
  }
  return value;
}

function serializeTouchCalibration(w: PostcardWriter, calibration: TouchCalibration) {
  w.writeVarint(calibration.raw_x_min);
  w.writeVarint(calibration.raw_x_max);
  w.writeVarint(calibration.raw_y_min);
  w.writeVarint(calibration.raw_y_max);
  w.writeBool(calibration.mirror_x);
  w.writeBool(calibration.mirror_y);
}

function deserializeTouchCalibration(r: PostcardReader): TouchCalibration {
  return {
    raw_x_min: r.readVarint(),
    raw_x_max: r.readVarint(),
    raw_y_min: r.readVarint(),
    raw_y_max: r.readVarint(),
    mirror_x: r.readBool(),
    mirror_y: r.readBool(),
  };
}

function serializeUpdateManifest(w: PostcardWriter, manifest: UpdateManifest) {
  w.writeU8(manifest.manifest_version);
  w.writeVarint(manifest.release_version);
  w.writeVarint(manifest.image_size);
  w.writeFixedBytes(expectBytes(manifest.image_sha256, 32, 'image_sha256'));
  w.writeFixedBytes(expectBytes(manifest.signing_pubkey_sha256, 32, 'signing_pubkey_sha256'));
  w.writeString(manifest.hardware_target);
  w.writeString(manifest.build_profile);
  w.writeU8(manifest.protocol_v);
  w.writeString(manifest.git_commit);
  w.writeString(manifest.tx_types_rev);
}

function deserializeUpdateStatus(r: PostcardReader): UpdateStatus {
  return {
    active: r.readBool(),
    manifest_verified: r.readBool(),
    image_verified: r.readBool(),
    release_version: r.readVarint(),
    bytes_received: r.readVarint(),
    image_size: r.readVarint(),
  };
}

function deserializeUpdateBootStatus(r: PostcardReader): UpdateBootStatus {
  return {
    partition_table_ok: r.readBool(),
    ota_data_present: r.readBool(),
    ota0_present: r.readBool(),
    ota1_present: r.readBool(),
    current_slot: r.readU8(),
    next_slot: r.readU8(),
    ota_state: r.readU8(),
    ota0_offset: r.readVarint(),
    ota0_size: r.readVarint(),
    ota1_offset: r.readVarint(),
    ota1_size: r.readVarint(),
  };
}

export function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

export function hexToBytes(value: string, label = 'hex'): Uint8Array {
  const cleaned = value
    .trim()
    .replace(/^0x/i, '')
    .replace(/[\s_:]/g, '');
  if (cleaned.length % 2 !== 0) {
    throw new Error(`${label} has an odd number of hex digits`);
  }
  const out = new Uint8Array(cleaned.length / 2);
  for (let i = 0; i < out.length; i++) {
    const byte = Number.parseInt(cleaned.slice(i * 2, i * 2 + 2), 16);
    if (Number.isNaN(byte)) {
      throw new Error(`invalid ${label}`);
    }
    out[i] = byte;
  }
  return out;
}

export function parseUpdateBundleJson(input: string | unknown): UpdateBundle {
  const raw = typeof input === 'string' ? JSON.parse(input) : input as any;
  if (!raw || typeof raw !== 'object') {
    throw new Error('update bundle must be a JSON object');
  }
  if (raw.format !== UPDATE_BUNDLE_FORMAT) {
    throw new Error(`unsupported update bundle format: ${raw.format ?? 'missing'}`);
  }
  if (raw.signature_scheme !== UPDATE_SIGNATURE_SCHEME) {
    throw new Error(`unsupported update signature scheme: ${raw.signature_scheme ?? 'missing'}`);
  }
  const manifest = raw.manifest;
  if (!manifest || typeof manifest !== 'object') {
    throw new Error('update bundle is missing manifest');
  }

  const signature64 = hexToBytes(String(raw.signature_hex ?? ''), 'signature');
  const signing_pubkey_sec1 = hexToBytes(String(raw.signing_pubkey_sec1_hex ?? ''), 'signing pubkey');
  return {
    format: raw.format,
    signature_scheme: raw.signature_scheme,
    manifest: {
      manifest_version: Number(manifest.manifest_version),
      release_version: Number(manifest.release_version),
      image_size: Number(manifest.image_size),
      image_sha256: expectBytes(hexToBytes(String(manifest.image_sha256_hex ?? ''), 'image sha256'), 32, 'image_sha256'),
      signing_pubkey_sha256: expectBytes(
        hexToBytes(String(manifest.signing_pubkey_sha256_hex ?? ''), 'signing pubkey sha256'),
        32,
        'signing_pubkey_sha256',
      ),
      hardware_target: String(manifest.hardware_target ?? ''),
      build_profile: String(manifest.build_profile ?? ''),
      protocol_v: Number(manifest.protocol_v),
      git_commit: String(manifest.git_commit ?? ''),
      tx_types_rev: String(manifest.tx_types_rev ?? ''),
    },
    signing_pubkey_sec1,
    signature64: expectBytes(signature64, 64, 'signature64'),
  };
}

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
        w.writeVarint(p);
      }
      w.writeBool(req.compressed ?? false);
      break;
    case 'GetXpub':
      w.writeVarint(7);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeVarint(p);
      }
      break;
    case 'SignDigest':
      w.writeVarint(8);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeVarint(p);
      }
      w.writeFixedBytes(req.digest32);
      break;
    case 'GetCheetahPub':
      w.writeVarint(9);
      w.writeU8(req.slot);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeVarint(p);
      }
      break;
    case 'SignSpendHash':
      w.writeVarint(10);
      w.writeU8(req.slot);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeVarint(p);
      }
      for (const val of req.msg5) {
        w.writeU64Varint(val);
      }
      serializeSpendMetaOptional(w, req.meta);
      break;
    case 'SignSpendHashFor':
      w.writeVarint(11);
      w.writeU8(req.slot);
      w.writeVarint(req.path.length);
      for (const p of req.path) {
        w.writeVarint(p);
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
      serializeSpendMetaOptional(w, req.meta);
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
    case 'GetSecurityStatus':
      w.writeVarint(22);
      break;
    case 'GetBuildInfo':
      w.writeVarint(23);
      break;
    case 'GetTouchCalibration':
      w.writeVarint(24);
      break;
    case 'SetTouchCalibration':
      w.writeVarint(25);
      serializeTouchCalibration(w, req.calibration);
      break;
    case 'ShowTouchDiagnostics':
      w.writeVarint(26);
      w.writeBool(req.enabled);
      break;
    case 'GetSeedLabels':
      w.writeVarint(27);
      break;
    case 'SetSeedLabel':
      w.writeVarint(28);
      w.writeU8(req.slot);
      w.writeString(req.label);
      break;
    case 'ChangePinOnDevice':
      w.writeVarint(29);
      w.writeString(req.current_pin);
      break;
    case 'StartTouchCalibration':
      w.writeVarint(30);
      break;
    case 'GetUpdateTrust':
      w.writeVarint(31);
      break;
    case 'VerifyUpdateManifest':
      w.writeVarint(32);
      serializeUpdateManifest(w, req.manifest);
      w.writeFixedBytes(expectBytes(req.signature64, 64, 'signature64'));
      w.writeBytes(req.signing_pubkey_sec1);
      break;
    case 'BeginUpdate':
      w.writeVarint(33);
      serializeUpdateManifest(w, req.manifest);
      w.writeFixedBytes(expectBytes(req.signature64, 64, 'signature64'));
      w.writeBytes(req.signing_pubkey_sec1);
      w.writeBool(req.write_flash);
      break;
    case 'UpdateChunk':
      w.writeVarint(34);
      w.writeVarint(req.offset);
      w.writeBytes(req.chunk);
      break;
    case 'FinishUpdate':
      w.writeVarint(35);
      break;
    case 'CancelUpdate':
      w.writeVarint(36);
      break;
    case 'GetUpdateStatus':
      w.writeVarint(37);
      break;
    case 'GetReleaseInfo':
      w.writeVarint(38);
      break;
    case 'GetUpdateBootStatus':
      w.writeVarint(39);
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
    case 1: { // FragBegin
      const id = r.readVarint();
      const total_len = r.readVarint();
      const kind = deserializeFragKind(r.readVarint());
      return { type: 'FragBegin', id, total_len, kind };
    }
    case 2: { // FragPart
      const id = r.readVarint();
      const offset = r.readVarint();
      const chunk = r.readBytes();
      const last = r.readBool();
      return { type: 'FragPart', id, offset, chunk, last };
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
          path.push(r.readVarint());
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
    case 6: { // OkSig
      const sig64 = r.readFixedBytes(64);
      return { type: 'OkSig', sig64 };
    }
    case 7: { // OkFingerprint
      const fp4 = r.readFixedBytes(4);
      return { type: 'OkFingerprint', fp4 };
    }
    case 8: { // OkPubkey
      const uncompressed = r.readFixedBytes(65);
      return { type: 'OkPubkey', uncompressed };
    }
    case 9: { // OkPubkeyCompressed
      const compressed = r.readFixedBytes(33);
      return { type: 'OkPubkeyCompressed', compressed };
    }
    case 10: { // OkXpub
      const depth = r.readU8();
      const fp4 = r.readFixedBytes(4);
      const child = r.readVarint(); // postcard encodes u32 as varint
      const chain_code = r.readFixedBytes(32);
      const pubkey33 = r.readFixedBytes(33);
      return { type: 'OkXpub', xpub: { depth, fp4, child, chain_code, pubkey33 } };
    }
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
      const code = r.readVarint();
      return { type: 'Err', code };
    }
    case 15: { // OkSecurityStatus
      const status: SecurityStatus = {
        chip_security_available: r.readBool(),
        mac: r.readFixedBytes(6),
        flash_encryption: r.readBool(),
        flash_crypt_cnt: r.readU8(),
        secure_boot: r.readBool(),
        secure_version: r.readVarint(),
        key_purposes: r.readFixedBytes(6),
        hmac_key_slots: r.readU8(),
        hmac_user_key_slots: r.readU8(),
        read_protected_key_slots: r.readU8(),
        pad_jtag_disabled: r.readBool(),
        usb_jtag_disabled: r.readBool(),
        soft_jtag_disabled: r.readBool(),
        soft_jtag_disable_bits: r.readU8(),
        usb_serial_jtag_disabled: r.readBool(),
        download_mode_disabled: r.readBool(),
        usb_serial_jtag_download_disabled: r.readBool(),
        usb_otg_download_disabled: r.readBool(),
        secure_download_enabled: r.readBool(),
        direct_boot_disabled: r.readBool(),
        usb_rom_print_disabled: r.readBool(),
        power_glitch_enabled: r.readBool(),
        nvs_initialized: r.readBool(),
        nvs_schema_version: r.readU8(),
        nvs_slot_count: r.readU8(),
      };
      return { type: 'OkSecurityStatus', status };
    }
    case 16: { // OkBuildInfo
      const info: BuildInfo = {
        git_commit: r.readString(),
        git_dirty: r.readBool(),
        build_profile: r.readString(),
        protocol_v: r.readU8(),
        tx_types_rev: r.readString(),
      };
      return { type: 'OkBuildInfo', info };
    }
    case 17: // OkTouchCalibration
      return { type: 'OkTouchCalibration', calibration: deserializeTouchCalibration(r) };
    case 18: { // OkSeedLabels
      const count = r.readVarint();
      const labels: SeedSlotLabel[] = [];
      for (let i = 0; i < count; i++) {
        labels.push({ slot: r.readU8(), label: r.readString() });
      }
      return { type: 'OkSeedLabels', labels };
    }
    case 19: { // OkUpdateTrust
      const trust: UpdateTrust = {
        configured: r.readBool(),
        pubkey_sha256: r.readFixedBytes(32),
      };
      return { type: 'OkUpdateTrust', trust };
    }
    case 20:
      return { type: 'OkUpdateStatus', status: deserializeUpdateStatus(r) };
    case 21:
      return { type: 'OkReleaseInfo', info: { release_version: r.readVarint() } };
    case 22:
      return { type: 'OkUpdateBootStatus', status: deserializeUpdateBootStatus(r) };
    default:
      throw new Error(`Unknown response variant: ${variant}`);
  }
}

/**
 * Serialize a Msg<Frame> (requests are wrapped in Frame::One)
 */
export function serializeMsg(msg: Msg<Frame>): Uint8Array {
  const w = new PostcardWriter();
  w.writeU8(msg.v);
  w.writeVarint(msg.id); // u32 is serialized as varint in postcard

  // Frame enum: 0 = One, 1 = FragBegin, 2 = FragPart
  switch (msg.msg.type) {
    case 'One': {
      w.writeVarint(0);
      w.writeFixedBytes(serializeRequest(msg.msg.request));
      break;
    }
    case 'FragBegin': {
      w.writeVarint(1);
      w.writeVarint(msg.msg.id);
      w.writeVarint(msg.msg.total_len);
      serializeFragKind(w, msg.msg.kind);
      break;
    }
    case 'FragPart': {
      w.writeVarint(2);
      w.writeVarint(msg.msg.id);
      w.writeVarint(msg.msg.offset);
      w.writeBytes(msg.msg.chunk);
      w.writeBool(msg.msg.last);
      break;
    }
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
    case ERR_REJECTED_BY_USER:
      return 'Rejected by user';
    case ERR_BUSY:
      return 'Device is busy';
    case ERR_PIN_MISMATCH:
      return 'PIN entries did not match';
    case ERR_FLASH:
      return 'Flash error';
    case ERR_CRYPTO:
      return 'Crypto error';
    case ERR_WRONG_PUBKEY:
      return 'Wrong public key';
    default:
      return `Error code: 0x${code.toString(16).padStart(4, '0')}`;
  }
}
