/// <reference path="./types.d.ts" />
/**
 * nockster-js - TypeScript library for communicating with Nockster hardware wallet
 *
 * This library provides:
 * - COBS framing for USB communication (HID/Serial)
 * - Postcard serialization/deserialization
 * - Protocol types and message handling
 * - Cheetah public key encoding
 * - Device connection management via WebHID/Web Serial API
 */

// Core serialization
export { PostcardReader, PostcardWriter } from './postcard.js';
export { COBSEncoder, COBSFrameReader } from './cobs.js';

// Protocol types and functions
export type {
  Request,
  Response,
  Msg,
  Frame,
  CheetahPubInfo,
  SpendMeta,
  SpendOutputMeta,
  Xpub,
  SecurityStatus,
  BuildInfo,
  ReleaseInfo,
  TouchCalibration,
  SeedSlotLabel,
  UpdateTrust,
  UpdateStatus,
  UpdateBootStatus,
  UpdateManifest,
  UpdateBundle,
} from './protocol.js';
export {
  serializeRequest,
  deserializeResponse,
  serializeMsg,
  deserializeMsg,
  getErrorMessage,
  parseUpdateBundleJson,
  bytesToHex,
  hexToBytes,
  // Error codes
  ERR_BAD_COBS_OR_POSTCARD,
  ERR_OVERFLOW,
  ERR_ENCODE_TOO_BIG,
  ERR_UNSUPPORTED_VERSION,
  ERR_NO_SEED,
  ERR_WRONG_PUBKEY,
  ERR_DEVICE_LOCKED,
  ERR_WRONG_PIN,
  ERR_PIN_LOCKED_OUT,
  ERR_ALREADY_INITIALIZED,
  ERR_REJECTED_BY_USER,
  ERR_PIN_MISMATCH,
  PROTO_V1,
  ERR_BUSY,
  ERR_FLASH,
  ERR_CRYPTO,
  FEATURE_SECURITY_STATUS,
  FEATURE_BUILD_INFO,
  FEATURE_SECURE_UPDATE,
  FEATURE_RELEASE_INFO,
  FEATURE_UPDATE_BOOT_STATUS,
  MAX_UPDATE_IMAGE_SIZE,
  MAX_UPDATE_CHUNK_LEN,
} from './protocol.js';

// Cheetah public key encoding
export {
  serializeCheetahPublicKey,
  base58Encode,
  formatCheetahPubkey,
} from './cheetah.js';

// Device connection (WebHID/Web Serial API)
export { NocksterDevice } from './device.js';
