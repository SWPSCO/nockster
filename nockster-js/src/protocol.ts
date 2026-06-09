import { PostcardReader, PostcardWriter } from './postcard.js';

/**
 * Protocol types matching nockster-core/src/lib.rs
 */

export type Frame =
  | { type: 'One'; request: Request }
  | { type: 'FragBegin'; id: number; total_len: number; kind: FragKind }
  | { type: 'FragPart'; id: number; offset: number; chunk: Uint8Array; last: boolean };

export type FragKind = 'SetSeed' | 'SignDraft' | 'AddressBook';

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

export interface DeviceAddressBookEntry {
  label: string;
  pkh: string;
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

export interface UpdateReleaseIndexMetadata {
  format?: string;
  releaseVersion?: number;
  imageSize?: number;
  imageSha256Hex?: string;
  hardwareTarget?: string;
  buildProfile?: string;
  protocolV?: number;
  gitCommit?: string;
  txTypesRev?: string;
}

export interface UpdateReleaseIndex {
  bundleUrl: URL;
  firmwareUrl: URL;
  metadata: UpdateReleaseIndexMetadata;
}

export interface FetchedUpdateRelease {
  bundle: UpdateBundle;
  bundleUrl: URL;
  bundleName: string;
  firmware: Uint8Array;
  firmwareUrl: URL;
  firmwareName: string;
  index?: UpdateReleaseIndex;
}

export interface UpdateCompatibilityOptions {
  hardwareTarget?: string;
  protocolV?: number;
  releaseVersion?: number | null;
  buildInfo?: BuildInfo | null;
  currentBuildProfile?: string | null;
}

export interface UpdateStreamStatusExpectation {
  expectedActive: boolean;
  expectedManifestVerified: boolean;
  expectedImageVerified: boolean;
  expectedBytesReceived: number;
}

type UpdateFetch = (input: RequestInfo | URL, init?: RequestInit) => Promise<globalThis.Response>;

export interface FetchUpdateReleaseArtifactsOptions {
  fetchImpl?: UpdateFetch;
  bundleInit?: RequestInit;
  firmwareInit?: RequestInit;
  bearerToken?: string;
  indexMetadata?: UpdateReleaseIndexMetadata;
  origin?: string | URL | null;
  enforceHttpsOrLocal?: boolean;
  validateBundle?: (bundle: UpdateBundle) => void;
}

export interface FetchLatestUpdateReleaseOptions extends Omit<
  FetchUpdateReleaseArtifactsOptions,
  'bundleInit' | 'firmwareInit' | 'indexMetadata'
> {
  indexInit?: RequestInit;
  bundleInit?: RequestInit;
  firmwareInit?: RequestInit;
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
  | { type: 'GetUpdateBootStatus' }
  | { type: 'Reboot' }
  | { type: 'GetAddressBook' }
  | { type: 'VaultStore'; label: string; preimage: Uint8Array }
  | { type: 'VaultList' }
  | { type: 'VaultReveal'; slot: number }
  | { type: 'VaultDelete'; slot: number }
  | { type: 'GetMasterPubkey'; slot: number };

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
  | { type: 'OkAddressBook'; entries: DeviceAddressBookEntry[] }
  | { type: 'OkVaultEntries'; entries: VaultEntryInfo[] }
  | { type: 'OkVaultPreimage'; commitment: bigint[]; preimage: Uint8Array }
  | { type: 'OkMasterPubkey'; x: bigint[]; y: bigint[]; chain_code: Uint8Array }
  | { type: 'Err'; code: number };

/** One preimage-vault slot. The commitment is the device-computed Tip5
 * hash-noun digest of the stored preimage (the `%hax` lock value). */
export interface VaultEntryInfo {
  slot: number;
  commitment: bigint[];
  label: string;
  preimage_len: number;
}

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
export const FEATURE_CHEETAH = 1 << 0;
export const FEATURE_FRAG = 1 << 1;
export const FEATURE_XPUB = 1 << 2;
export const FEATURE_SECURITY_STATUS = 1 << 3;
export const FEATURE_BUILD_INFO = 1 << 4;
export const FEATURE_TOUCH_CALIBRATION = 1 << 5;
export const FEATURE_TOUCH_DIAGNOSTICS = 1 << 6;
export const FEATURE_SEED_LABELS = 1 << 7;
export const FEATURE_PIN_CHANGE_UI = 1 << 8;
export const FEATURE_TOUCH_CALIBRATION_UI = 1 << 9;
export const FEATURE_SECURE_UPDATE = 1 << 10;
export const FEATURE_RELEASE_INFO = 1 << 11;
export const FEATURE_UPDATE_BOOT_STATUS = 1 << 12;
export const FEATURE_DEVICE_REBOOT = 1 << 13;
export const FEATURE_DEVICE_ADDRESS_BOOK = 1 << 14;
export const FEATURE_PREIMAGE_VAULT = 1 << 15;
export const FEATURE_MASTER_PUBKEY_EXPORT = 1 << 16;
export const MAX_VAULT_ENTRIES = 8;
export const MAX_VAULT_PREIMAGE_LEN = 320;
export const NOCKSTER_UPDATE_HARDWARE_TARGET = 'esp32s3-touch-lcd-1.47';
export const UPDATE_BUILD_PROFILE_DEV = 'dev';
export const UPDATE_BUILD_PROFILE_CHIP_SECURITY = 'chip-security';
export const UPDATE_BUILD_PROFILE_PRODUCTION = 'production';
export const UPDATE_SLOT_NONE = 0;
export const UPDATE_SLOT_OTA0 = 1;
export const UPDATE_SLOT_OTA1 = 2;
export const UPDATE_SLOT_UNKNOWN = 0xff;
export const UPDATE_OTA_STATE_NEW = 0;
export const UPDATE_OTA_STATE_PENDING_VERIFY = 1;
export const UPDATE_OTA_STATE_VALID = 2;
export const UPDATE_OTA_STATE_INVALID = 3;
export const UPDATE_OTA_STATE_ABORTED = 4;
export const UPDATE_OTA_STATE_UNAVAILABLE = 0xfd;
export const UPDATE_OTA_STATE_UNKNOWN = 0xfe;
export const UPDATE_OTA_STATE_UNDEFINED = 0xff;
export const UPDATE_BUNDLE_FORMAT = 'nockster-update-bundle-v1';
export const UPDATE_RELEASE_INDEX_FORMAT = 'nockster-release-index-v1';
export const UPDATE_SIGNATURE_SCHEME = 'secp256k1-ecdsa-sha256-prehash-v1';
export const UPDATE_MANIFEST_VERSION = 1;
export const MAX_UPDATE_RELEASE_VERSION = 0xffff_ffff;
export const MAX_UPDATE_IMAGE_SIZE = 4 * 1024 * 1024;
export const MAX_UPDATE_CHUNK_LEN = 512;
export const MAX_DEVICE_ADDRESS_BOOK_ENTRIES = 50;
export const MAX_ADDRESS_BOOK_LABEL_LEN = 32;
export const MAX_ADDRESS_BOOK_PKH_LEN = 64;

function serializeFragKind(w: PostcardWriter, kind: FragKind) {
  // nockster-core::FragKind: SetSeed=0, SignDraft=1, AddressBook=2
  switch (kind) {
    case 'SetSeed':
      w.writeVarint(0);
      break;
    case 'SignDraft':
      w.writeVarint(1);
      break;
    case 'AddressBook':
      w.writeVarint(2);
      break;
  }
}

function deserializeFragKind(kind: number): FragKind {
  switch (kind) {
    case 0:
      return 'SetSeed';
    case 1:
      return 'SignDraft';
    case 2:
      return 'AddressBook';
    default:
      throw new Error(`Unknown frag kind: ${kind}`);
  }
}

function validateDeviceAddressBookEntry(entry: DeviceAddressBookEntry): DeviceAddressBookEntry {
  const label = (entry.label ?? '').trim();
  const pkh = (entry.pkh ?? '').trim();
  if (!label) throw new Error('address label required');
  if (label.length > MAX_ADDRESS_BOOK_LABEL_LEN) {
    throw new Error(`address label max ${MAX_ADDRESS_BOOK_LABEL_LEN} chars`);
  }
  if (!/^[\x20-\x7e]+$/.test(label)) {
    throw new Error('address label must be printable ASCII');
  }
  if (!pkh) throw new Error('address pkh required');
  if (pkh.length > MAX_ADDRESS_BOOK_PKH_LEN) {
    throw new Error(`address pkh max ${MAX_ADDRESS_BOOK_PKH_LEN} chars`);
  }
  if (!/^[1-9A-HJ-NP-Za-km-z]+$/.test(pkh)) {
    throw new Error('address pkh must be base58');
  }
  return { label, pkh };
}

function serializeDeviceAddressBookEntry(w: PostcardWriter, entry: DeviceAddressBookEntry) {
  const normalized = validateDeviceAddressBookEntry(entry);
  w.writeString(normalized.label);
  w.writeString(normalized.pkh);
}

function deserializeDeviceAddressBookEntriesFromReader(r: PostcardReader): DeviceAddressBookEntry[] {
  const count = r.readVarint();
  const entries: DeviceAddressBookEntry[] = [];
  for (let i = 0; i < count; i++) {
    entries.push(validateDeviceAddressBookEntry({
      label: r.readString(),
      pkh: r.readString(),
    }));
  }
  return entries;
}

export function serializeDeviceAddressBookEntries(entries: DeviceAddressBookEntry[]): Uint8Array {
  if (entries.length > MAX_DEVICE_ADDRESS_BOOK_ENTRIES) {
    throw new Error(`device address book max ${MAX_DEVICE_ADDRESS_BOOK_ENTRIES} entries`);
  }
  const w = new PostcardWriter();
  w.writeVarint(entries.length);
  for (const entry of entries) {
    serializeDeviceAddressBookEntry(w, entry);
  }
  return w.toBytes();
}

export function deserializeDeviceAddressBookEntries(data: Uint8Array): DeviceAddressBookEntry[] {
  const r = new PostcardReader(data);
  const entries = deserializeDeviceAddressBookEntriesFromReader(r);
  if (r.hasMore()) {
    throw new Error('trailing bytes in address book payload');
  }
  return entries;
}

function serializeSpendMetaOptional(w: PostcardWriter, meta: SpendMeta | undefined) {
  if (!meta || !Array.isArray(meta.outputs) || meta.outputs.length === 0) {
    // Option::None must be written explicitly: postcard cannot decode an
    // omitted trailing field, so leaving it out makes firmware reject the
    // frame as bad postcard.
    w.writeVarint(0);
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

function expectCompressedSec1Pubkey(value: Uint8Array, label: string): Uint8Array {
  const bytes = expectBytes(value, 33, label);
  if (bytes[0] !== 0x02 && bytes[0] !== 0x03) {
    throw new Error(`${label} must be a compressed SEC1 public key`);
  }
  return bytes;
}

function expectString(value: unknown, label: string): string {
  if (typeof value !== 'string' || value.trim().length === 0) {
    throw new Error(`${label} must be a non-empty string`);
  }
  return value;
}

function expectUint(value: unknown, label: string, max: number): number {
  if (typeof value !== 'number' || !Number.isInteger(value) || value < 0 || value > max) {
    throw new Error(`${label} must be an integer between 0 and ${max}`);
  }
  return value;
}

function expectUpdateManifestVersion(value: unknown): number {
  const version = expectUint(value, 'manifest_version', 0xff);
  if (version !== UPDATE_MANIFEST_VERSION) {
    throw new Error(`unsupported update manifest version: ${version}`);
  }
  return version;
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

function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) {
    return false;
  }
  let diff = 0;
  for (let i = 0; i < a.length; i += 1) {
    diff |= a[i] ^ b[i];
  }
  return diff === 0;
}

function bufferSourceForDigest(bytes: Uint8Array): BufferSource {
  if (bytes.buffer instanceof ArrayBuffer) {
    return new Uint8Array(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  }

  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return copy;
}

export function hexToBytes(value: string, label = 'hex'): Uint8Array {
  const cleaned = value
    .trim()
    .replace(/^0x/i, '')
    .replace(/[\s_:]/g, '');
  if (cleaned.length % 2 !== 0) {
    throw new Error(`${label} has an odd number of hex digits`);
  }
  if (!/^[0-9a-fA-F]*$/.test(cleaned)) {
    throw new Error(`invalid ${label}`);
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
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
    throw new Error('update bundle must be a JSON object');
  }
  if (raw.format !== UPDATE_BUNDLE_FORMAT) {
    throw new Error(`unsupported update bundle format: ${raw.format ?? 'missing'}`);
  }
  if (raw.signature_scheme !== UPDATE_SIGNATURE_SCHEME) {
    throw new Error(`unsupported update signature scheme: ${raw.signature_scheme ?? 'missing'}`);
  }
  const manifest = raw.manifest;
  if (!manifest || typeof manifest !== 'object' || Array.isArray(manifest)) {
    throw new Error('update bundle is missing manifest');
  }

  const signature64 = hexToBytes(String(raw.signature_hex ?? ''), 'signature');
  const signing_pubkey_sec1 = hexToBytes(String(raw.signing_pubkey_sec1_hex ?? ''), 'signing pubkey');
  const imageSize = expectUint(manifest.image_size, 'image_size', MAX_UPDATE_IMAGE_SIZE);
  if (imageSize === 0) {
    throw new Error('image_size must be nonzero');
  }
  return {
    format: raw.format,
    signature_scheme: raw.signature_scheme,
    manifest: {
      manifest_version: expectUpdateManifestVersion(manifest.manifest_version),
      release_version: expectUint(manifest.release_version, 'release_version', MAX_UPDATE_RELEASE_VERSION),
      image_size: imageSize,
      image_sha256: expectBytes(hexToBytes(String(manifest.image_sha256_hex ?? ''), 'image sha256'), 32, 'image_sha256'),
      signing_pubkey_sha256: expectBytes(
        hexToBytes(String(manifest.signing_pubkey_sha256_hex ?? ''), 'signing pubkey sha256'),
        32,
        'signing_pubkey_sha256',
      ),
      hardware_target: expectString(manifest.hardware_target, 'hardware_target'),
      build_profile: expectString(manifest.build_profile, 'build_profile'),
      protocol_v: expectUint(manifest.protocol_v, 'protocol_v', 0xff),
      git_commit: expectString(manifest.git_commit, 'git_commit'),
      tx_types_rev: expectString(manifest.tx_types_rev, 'tx_types_rev'),
    },
    signing_pubkey_sec1: expectCompressedSec1Pubkey(signing_pubkey_sec1, 'signing_pubkey_sec1'),
    signature64: expectBytes(signature64, 64, 'signature64'),
  };
}

function parseReleaseIndexObject(input: string | unknown): Record<string, unknown> {
  const raw = typeof input === 'string' ? JSON.parse(input) : input;
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) {
    throw new Error('release index must be a JSON object');
  }
  return raw as Record<string, unknown>;
}

function parseReleaseIndexBaseUrl(value: string | URL): URL {
  try {
    const url = value instanceof URL ? new URL(value.href) : new URL(value);
    if (url.protocol !== 'https:' && url.protocol !== 'http:') {
      throw new Error('release index URL must use http or https');
    }
    return url;
  } catch (error: any) {
    const message = error?.message ?? error?.toString() ?? 'invalid URL';
    throw new Error(`release index URL: ${message}`);
  }
}

function parseReleaseIndexArtifactUrl(value: string, base: URL, label: string): URL {
  try {
    const url = new URL(value, base);
    if (url.protocol !== 'https:' && url.protocol !== 'http:') {
      throw new Error(`${label} must use http or https`);
    }
    return url;
  } catch (error: any) {
    const message = error?.message ?? error?.toString() ?? 'invalid URL';
    throw new Error(`${label}: ${message}`);
  }
}

function readReleaseIndexUrlField(value: Record<string, unknown>, snake: string, camel: string): string {
  const raw = value[snake] ?? value[camel];
  if (typeof raw !== 'string' || !raw.trim()) {
    throw new Error(`release index is missing ${snake}`);
  }
  return raw.trim();
}

function readOptionalIndexString(value: Record<string, unknown>, snake: string, camel: string): string | undefined {
  const raw = value[snake] ?? value[camel];
  if (raw === undefined || raw === null) {
    return undefined;
  }
  if (typeof raw !== 'string' || !raw.trim()) {
    throw new Error(`release index ${snake} must be a non-empty string`);
  }
  return raw.trim();
}

function readOptionalIndexHexBytes(
  value: Record<string, unknown>,
  snake: string,
  camel: string,
  byteLen: number,
): string | undefined {
  const raw = readOptionalIndexString(value, snake, camel);
  if (raw === undefined) {
    return undefined;
  }
  const label = `release index ${snake}`;
  return bytesToHex(expectBytes(hexToBytes(raw, label), byteLen, label));
}

function readOptionalIndexNumber(value: Record<string, unknown>, snake: string, camel: string): number | undefined {
  const raw = value[snake] ?? value[camel];
  if (raw === undefined || raw === null) {
    return undefined;
  }
  if (typeof raw !== 'number' || !Number.isInteger(raw) || raw < 0) {
    throw new Error(`release index ${snake} must be a non-negative integer`);
  }
  return raw;
}

function readOptionalIndexUint(
  value: Record<string, unknown>,
  snake: string,
  camel: string,
  max: number,
): number | undefined {
  const raw = readOptionalIndexNumber(value, snake, camel);
  if (raw === undefined) {
    return undefined;
  }
  if (raw > max) {
    throw new Error(`release index ${snake} must be at most ${max}`);
  }
  return raw;
}

export function parseUpdateReleaseIndexJson(input: string | unknown, indexUrl: string | URL): UpdateReleaseIndex {
  const raw = parseReleaseIndexObject(input);
  const base = parseReleaseIndexBaseUrl(indexUrl);
  const format = readOptionalIndexString(raw, 'format', 'format');
  if (format !== undefined && format !== UPDATE_RELEASE_INDEX_FORMAT) {
    throw new Error(`unsupported release index format: ${format}`);
  }

  return {
    bundleUrl: parseReleaseIndexArtifactUrl(
      readReleaseIndexUrlField(raw, 'bundle_url', 'bundleUrl'),
      base,
      'bundle URL',
    ),
    firmwareUrl: parseReleaseIndexArtifactUrl(
      readReleaseIndexUrlField(raw, 'firmware_url', 'firmwareUrl'),
      base,
      'firmware URL',
    ),
    metadata: {
      format,
      releaseVersion: readOptionalIndexUint(raw, 'release_version', 'releaseVersion', MAX_UPDATE_RELEASE_VERSION),
      imageSize: readOptionalIndexUint(raw, 'image_size', 'imageSize', MAX_UPDATE_IMAGE_SIZE),
      imageSha256Hex: readOptionalIndexHexBytes(raw, 'image_sha256_hex', 'imageSha256Hex', 32),
      hardwareTarget: readOptionalIndexString(raw, 'hardware_target', 'hardwareTarget'),
      buildProfile: readOptionalIndexString(raw, 'build_profile', 'buildProfile'),
      protocolV: readOptionalIndexUint(raw, 'protocol_v', 'protocolV', 0xff),
      gitCommit: readOptionalIndexString(raw, 'git_commit', 'gitCommit'),
      txTypesRev: readOptionalIndexString(raw, 'tx_types_rev', 'txTypesRev'),
    },
  };
}

export function assertUpdateReleaseIndexMatchesBundle(
  index: UpdateReleaseIndexMetadata,
  bundle: UpdateBundle,
): void {
  const mismatches: string[] = [];
  const manifest = bundle.manifest;
  const expectedSha = index.imageSha256Hex?.toLowerCase();
  if (index.releaseVersion !== undefined && index.releaseVersion !== manifest.release_version) {
    mismatches.push(`release_version ${index.releaseVersion} != bundle ${manifest.release_version}`);
  }
  if (index.imageSize !== undefined && index.imageSize !== manifest.image_size) {
    mismatches.push(`image_size ${index.imageSize} != bundle ${manifest.image_size}`);
  }
  if (expectedSha !== undefined && expectedSha !== bytesToHex(manifest.image_sha256)) {
    mismatches.push('image_sha256_hex does not match bundle manifest');
  }
  if (index.hardwareTarget !== undefined && index.hardwareTarget !== manifest.hardware_target) {
    mismatches.push(`hardware_target ${index.hardwareTarget} != bundle ${manifest.hardware_target}`);
  }
  if (index.buildProfile !== undefined && index.buildProfile !== manifest.build_profile) {
    mismatches.push(`build_profile ${index.buildProfile} != bundle ${manifest.build_profile}`);
  }
  if (index.protocolV !== undefined && index.protocolV !== manifest.protocol_v) {
    mismatches.push(`protocol_v ${index.protocolV} != bundle ${manifest.protocol_v}`);
  }
  if (index.gitCommit !== undefined && index.gitCommit !== manifest.git_commit) {
    mismatches.push('git_commit does not match bundle manifest');
  }
  if (index.txTypesRev !== undefined && index.txTypesRev !== manifest.tx_types_rev) {
    mismatches.push('tx_types_rev does not match bundle manifest');
  }
  if (mismatches.length) {
    throw new Error(`release index metadata mismatch: ${mismatches.join('; ')}`);
  }
}

function updateBuildProfileAllowed(current: string, candidate: string): boolean {
  const supported = [
    UPDATE_BUILD_PROFILE_DEV,
    UPDATE_BUILD_PROFILE_CHIP_SECURITY,
    UPDATE_BUILD_PROFILE_PRODUCTION,
  ];
  if (!supported.includes(current) || !supported.includes(candidate)) {
    return false;
  }
  return current !== UPDATE_BUILD_PROFILE_PRODUCTION || candidate === UPDATE_BUILD_PROFILE_PRODUCTION;
}

export function getUpdateBundleCompatibilityBlocker(
  bundle: UpdateBundle | null,
  options: UpdateCompatibilityOptions = {},
): string | null {
  if (!bundle) {
    return null;
  }
  const manifest = bundle.manifest;
  const hardwareTarget = options.hardwareTarget ?? NOCKSTER_UPDATE_HARDWARE_TARGET;
  const protocol = options.protocolV ?? options.buildInfo?.protocol_v ?? PROTO_V1;
  const releaseVersion = options.releaseVersion;
  const currentBuildProfile = options.currentBuildProfile ?? options.buildInfo?.build_profile ?? null;

  if (manifest.hardware_target !== hardwareTarget) {
    return `Bundle target ${manifest.hardware_target} does not match this device target ${hardwareTarget}.`;
  }
  if (manifest.protocol_v !== protocol) {
    return `Bundle protocol ${manifest.protocol_v} does not match device protocol ${protocol}.`;
  }
  if (manifest.image_size <= 0) {
    return 'Bundle image size must be nonzero.';
  }
  if (manifest.image_size > MAX_UPDATE_IMAGE_SIZE) {
    return `Bundle image size ${manifest.image_size} exceeds ${MAX_UPDATE_IMAGE_SIZE} bytes.`;
  }
  if (releaseVersion !== undefined && releaseVersion !== null && manifest.release_version <= releaseVersion) {
    return `Bundle release ${manifest.release_version} is not newer than device release ${releaseVersion}.`;
  }
  if (currentBuildProfile !== null && !updateBuildProfileAllowed(currentBuildProfile, manifest.build_profile)) {
    return `Bundle profile ${manifest.build_profile} is not accepted by device profile ${currentBuildProfile}.`;
  }

  return null;
}

export function assertUpdateBundleCompatible(
  bundle: UpdateBundle,
  options: UpdateCompatibilityOptions = {},
): void {
  const blocker = getUpdateBundleCompatibilityBlocker(bundle, options);
  if (blocker) {
    throw new Error(blocker);
  }
}

export function updateSlotName(slot: number): string {
  switch (slot) {
    case UPDATE_SLOT_NONE:
      return 'factory/none';
    case UPDATE_SLOT_OTA0:
      return 'ota_0';
    case UPDATE_SLOT_OTA1:
      return 'ota_1';
    case UPDATE_SLOT_UNKNOWN:
      return 'unknown';
    default:
      return 'invalid';
  }
}

export function updateOtaStateName(state: number): string {
  switch (state) {
    case UPDATE_OTA_STATE_NEW:
      return 'new';
    case UPDATE_OTA_STATE_PENDING_VERIFY:
      return 'pending-verify';
    case UPDATE_OTA_STATE_VALID:
      return 'valid';
    case UPDATE_OTA_STATE_INVALID:
      return 'invalid';
    case UPDATE_OTA_STATE_ABORTED:
      return 'aborted';
    case UPDATE_OTA_STATE_UNAVAILABLE:
      return 'unavailable';
    case UPDATE_OTA_STATE_UNKNOWN:
      return 'unknown';
    case UPDATE_OTA_STATE_UNDEFINED:
      return 'undefined';
    default:
      return 'invalid';
  }
}

export function getPostInstallUpdateBootStatusFailures(status: UpdateBootStatus): string[] {
  const failures: string[] = [];
  if (!status.partition_table_ok) {
    failures.push('partition table is not readable');
  }
  if (!status.ota_data_present) {
    failures.push('otadata partition is missing');
  }
  if (!status.ota0_present || !status.ota1_present) {
    failures.push('both OTA app slots must be present');
  }
  if (status.current_slot !== UPDATE_SLOT_OTA0 && status.current_slot !== UPDATE_SLOT_OTA1) {
    failures.push(`selected boot slot is ${updateSlotName(status.current_slot)}, expected ota_0 or ota_1`);
  }
  if (status.ota_state !== UPDATE_OTA_STATE_NEW) {
    failures.push(`selected OTA image state is ${updateOtaStateName(status.ota_state)}, expected new`);
  }
  return failures;
}

export function assertPostInstallUpdateBootStatus(status: UpdateBootStatus): void {
  const failures = getPostInstallUpdateBootStatusFailures(status);
  if (failures.length) {
    throw new Error(`post-install activation validation failed: ${failures.join('; ')}`);
  }
}

function yesNo(value: boolean): string {
  return value ? 'yes' : 'no';
}

export function getUpdateStreamStatusFailures(
  status: UpdateStatus,
  bundle: UpdateBundle,
  expectation: UpdateStreamStatusExpectation,
): string[] {
  const manifest = bundle.manifest;
  const failures: string[] = [];
  if (status.active !== expectation.expectedActive) {
    failures.push(`active is ${yesNo(status.active)}, expected ${yesNo(expectation.expectedActive)}`);
  }
  if (status.manifest_verified !== expectation.expectedManifestVerified) {
    failures.push(`manifest_verified is ${yesNo(status.manifest_verified)}, expected ${yesNo(expectation.expectedManifestVerified)}`);
  }
  if (status.image_verified !== expectation.expectedImageVerified) {
    failures.push(`image_verified is ${yesNo(status.image_verified)}, expected ${yesNo(expectation.expectedImageVerified)}`);
  }
  if (status.release_version !== manifest.release_version) {
    failures.push(`release_version is ${status.release_version}, expected ${manifest.release_version}`);
  }
  if (status.image_size !== manifest.image_size) {
    failures.push(`image_size is ${status.image_size}, expected ${manifest.image_size}`);
  }
  if (status.bytes_received !== expectation.expectedBytesReceived) {
    failures.push(`bytes_received is ${status.bytes_received}, expected ${expectation.expectedBytesReceived}`);
  }
  return failures;
}

export function assertUpdateStreamStatus(
  status: UpdateStatus,
  bundle: UpdateBundle,
  phase: string,
  expectation: UpdateStreamStatusExpectation,
): void {
  const failures = getUpdateStreamStatusFailures(status, bundle, expectation);
  if (failures.length) {
    throw new Error(`${phase}: invalid device update status: ${failures.join('; ')}`);
  }
}

export async function assertUpdateFirmwareMatchesBundle(
  bundle: UpdateBundle,
  firmware: Uint8Array,
): Promise<void> {
  if (bundle.manifest.image_size <= 0) {
    throw new Error('bundle image size must be nonzero');
  }
  if (bundle.manifest.image_size > MAX_UPDATE_IMAGE_SIZE) {
    throw new Error(`bundle image size exceeds ${MAX_UPDATE_IMAGE_SIZE} bytes`);
  }
  if (firmware.length !== bundle.manifest.image_size) {
    throw new Error(`firmware size mismatch: bundle expects ${bundle.manifest.image_size}, got ${firmware.length}`);
  }
  if (!globalThis.crypto?.subtle) {
    throw new Error('SHA-256 is unavailable in this browser context');
  }

  const digest = new Uint8Array(await globalThis.crypto.subtle.digest('SHA-256', bufferSourceForDigest(firmware)));
  if (!bytesEqual(digest, bundle.manifest.image_sha256)) {
    throw new Error(
      `firmware sha256 mismatch: bundle expects ${bytesToHex(bundle.manifest.image_sha256)}, got ${bytesToHex(digest)}`
    );
  }
}

function resolveUpdateFetch(fetchImpl?: UpdateFetch): UpdateFetch {
  if (fetchImpl) {
    return fetchImpl;
  }
  if (!globalThis.fetch) {
    throw new Error('fetch is unavailable in this browser context');
  }
  return globalThis.fetch.bind(globalThis);
}

function parseUpdateFetchUrl(value: string | URL, label: string): URL {
  try {
    const url = value instanceof URL ? new URL(value.href) : new URL(value);
    if (url.protocol !== 'https:' && url.protocol !== 'http:') {
      throw new Error(`${label} must use http or https`);
    }
    return url;
  } catch (error: any) {
    const message = error?.message ?? error?.toString() ?? 'invalid URL';
    throw new Error(`${label}: ${message}`);
  }
}

function updateReleaseOrigin(value?: string | URL | null): string | null {
  if (value === null) {
    return null;
  }
  if (value !== undefined) {
    return new URL(value instanceof URL ? value.href : value).origin;
  }
  return typeof globalThis.location === 'undefined' ? null : globalThis.location.origin;
}

function updateFetchCredentialsForUrl(url: URL, origin: string | null, bearerToken?: string): RequestCredentials {
  if (bearerToken?.trim()) {
    return 'omit';
  }
  return origin !== null && url.origin === origin ? 'same-origin' : 'omit';
}

function updateFetchHeaders(init: RequestInit | undefined, bearerToken: string | undefined): Headers | undefined {
  const trimmedToken = bearerToken?.trim() ?? '';
  if (!trimmedToken) {
    return init?.headers === undefined ? undefined : new Headers(init.headers);
  }
  const headers = new Headers(init?.headers);
  headers.set('authorization', `Bearer ${trimmedToken}`);
  return headers;
}

function updateFetchInit(
  url: URL,
  origin: string | null,
  init?: RequestInit,
  bearerToken?: string,
): RequestInit {
  return {
    credentials: updateFetchCredentialsForUrl(url, origin, bearerToken),
    ...init,
    headers: updateFetchHeaders(init, bearerToken),
    cache: 'no-store',
  };
}

function isLocalUpdateReleaseHost(url: URL): boolean {
  return url.hostname === 'localhost'
    || url.hostname === '127.0.0.1'
    || url.hostname === '::1'
    || url.hostname === '[::1]';
}

function assertHttpsOrLocalUpdateReleaseUrl(url: URL, label: string): void {
  if (url.protocol !== 'https:' && !isLocalUpdateReleaseHost(url)) {
    throw new Error(`${label} must use HTTPS, except for localhost testing`);
  }
}

export function assertPrivateUpdateReleaseUrls(
  bundleUrlInput: string | URL,
  firmwareUrlInput: string | URL,
  bearerToken: string,
): void {
  if (!bearerToken.trim()) {
    return;
  }
  const bundleUrl = parseUpdateFetchUrl(bundleUrlInput, 'bundle URL');
  const firmwareUrl = parseUpdateFetchUrl(firmwareUrlInput, 'firmware URL');
  if (bundleUrl.origin !== firmwareUrl.origin) {
    throw new Error('bearer-token update fetch requires bundle and firmware URLs on the same origin');
  }
  if (
    (bundleUrl.protocol !== 'https:' || firmwareUrl.protocol !== 'https:')
    && (!isLocalUpdateReleaseHost(bundleUrl) || !isLocalUpdateReleaseHost(firmwareUrl))
  ) {
    throw new Error('bearer-token update fetch requires HTTPS, except for localhost testing');
  }
}

function updateArtifactNameFromUrl(value: string, fallback: string): string {
  try {
    const path = new URL(value).pathname.split('/').filter(Boolean).pop();
    return path ? decodeURIComponent(path) : fallback;
  } catch {
    return fallback;
  }
}

function assertFirmwareContentLengthMatchesBundle(response: globalThis.Response, bundle: UpdateBundle): void {
  const firmwareLength = response.headers.get('content-length');
  if (firmwareLength === null) {
    return;
  }
  const parsedLength = Number(firmwareLength);
  if (!Number.isFinite(parsedLength) || parsedLength !== bundle.manifest.image_size) {
    throw new Error(`firmware size mismatch: bundle expects ${bundle.manifest.image_size}, server reports ${firmwareLength}`);
  }
}

export async function fetchUpdateReleaseArtifacts(
  bundleUrlInput: string | URL,
  firmwareUrlInput: string | URL,
  options: FetchUpdateReleaseArtifactsOptions = {},
): Promise<FetchedUpdateRelease> {
  const fetchImpl = resolveUpdateFetch(options.fetchImpl);
  const origin = updateReleaseOrigin(options.origin);
  const enforceHttpsOrLocal = options.enforceHttpsOrLocal ?? true;
  const bundleUrl = parseUpdateFetchUrl(bundleUrlInput, 'bundle URL');
  const firmwareUrl = parseUpdateFetchUrl(firmwareUrlInput, 'firmware URL');
  const bearerToken = options.bearerToken?.trim() ?? '';

  if (enforceHttpsOrLocal) {
    assertHttpsOrLocalUpdateReleaseUrl(bundleUrl, 'bundle URL');
    assertHttpsOrLocalUpdateReleaseUrl(firmwareUrl, 'firmware URL');
  }
  assertPrivateUpdateReleaseUrls(bundleUrl, firmwareUrl, bearerToken);

  const bundleResp = await fetchImpl(bundleUrl, updateFetchInit(bundleUrl, origin, options.bundleInit, bearerToken));
  if (!bundleResp.ok) {
    throw new Error(`bundle fetch failed: HTTP ${bundleResp.status}`);
  }
  const bundle = parseUpdateBundleJson(await bundleResp.text());
  if (options.indexMetadata) {
    assertUpdateReleaseIndexMatchesBundle(options.indexMetadata, bundle);
  }
  options.validateBundle?.(bundle);

  const firmwareResp = await fetchImpl(firmwareUrl, updateFetchInit(firmwareUrl, origin, options.firmwareInit, bearerToken));
  if (!firmwareResp.ok) {
    throw new Error(`firmware fetch failed: HTTP ${firmwareResp.status}`);
  }
  assertFirmwareContentLengthMatchesBundle(firmwareResp, bundle);
  const firmware = new Uint8Array(await firmwareResp.arrayBuffer());
  await assertUpdateFirmwareMatchesBundle(bundle, firmware);

  return {
    bundle,
    bundleUrl,
    bundleName: updateArtifactNameFromUrl(bundleUrl.href, 'remote bundle'),
    firmware,
    firmwareUrl,
    firmwareName: updateArtifactNameFromUrl(firmwareUrl.href, 'remote firmware'),
  };
}

export async function fetchLatestUpdateRelease(
  indexUrlInput: string | URL,
  options: FetchLatestUpdateReleaseOptions = {},
): Promise<FetchedUpdateRelease> {
  const fetchImpl = resolveUpdateFetch(options.fetchImpl);
  const origin = updateReleaseOrigin(options.origin);
  const enforceHttpsOrLocal = options.enforceHttpsOrLocal ?? true;
  const indexUrl = parseUpdateFetchUrl(indexUrlInput, 'release index URL');
  const bearerToken = options.bearerToken?.trim() ?? '';

  if (enforceHttpsOrLocal) {
    assertHttpsOrLocalUpdateReleaseUrl(indexUrl, 'release index URL');
  }

  const indexResp = await fetchImpl(indexUrl, updateFetchInit(indexUrl, origin, options.indexInit, bearerToken));
  if (!indexResp.ok) {
    throw new Error(`release index fetch failed: HTTP ${indexResp.status}`);
  }
  const index = parseUpdateReleaseIndexJson(await indexResp.json(), indexUrl);
  if (bearerToken && (index.bundleUrl.origin !== indexUrl.origin || index.firmwareUrl.origin !== indexUrl.origin)) {
    throw new Error('bearer-token latest-release fetch requires index, bundle, and firmware URLs on the same origin');
  }
  const release = await fetchUpdateReleaseArtifacts(index.bundleUrl, index.firmwareUrl, {
    fetchImpl,
    bundleInit: options.bundleInit,
    firmwareInit: options.firmwareInit,
    bearerToken,
    indexMetadata: index.metadata,
    origin,
    enforceHttpsOrLocal,
    validateBundle: options.validateBundle,
  });

  return {
    ...release,
    bundleName: updateArtifactNameFromUrl(index.bundleUrl.href, 'latest bundle'),
    firmwareName: updateArtifactNameFromUrl(index.firmwareUrl.href, 'latest firmware'),
    index,
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
    case 'Reboot':
      w.writeVarint(40);
      break;
    case 'GetAddressBook':
      w.writeVarint(41);
      break;
    case 'VaultStore':
      w.writeVarint(42);
      w.writeString(req.label);
      w.writeBytes(req.preimage);
      break;
    case 'VaultList':
      w.writeVarint(43);
      break;
    case 'VaultReveal':
      w.writeVarint(44);
      w.writeU8(req.slot);
      break;
    case 'VaultDelete':
      w.writeVarint(45);
      w.writeU8(req.slot);
      break;
    case 'GetMasterPubkey':
      w.writeVarint(46);
      w.writeU8(req.slot);
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
    case 23:
      return { type: 'OkAddressBook', entries: deserializeDeviceAddressBookEntriesFromReader(r) };
    case 24: {
      const count = r.readVarint();
      const entries: VaultEntryInfo[] = [];
      for (let i = 0; i < count; i++) {
        entries.push({
          slot: r.readU8(),
          commitment: r.readU64Array(5),
          label: r.readString(),
          preimage_len: r.readVarint(),
        });
      }
      return { type: 'OkVaultEntries', entries };
    }
    case 25:
      return {
        type: 'OkVaultPreimage',
        commitment: r.readU64Array(5),
        preimage: r.readBytes(),
      };
    case 26:
      return {
        type: 'OkMasterPubkey',
        x: r.readU64Array(6),
        y: r.readU64Array(6),
        chain_code: r.readFixedBytes(32),
      };
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
