/**
 * siger-js - TypeScript library for communicating with Siger hardware wallet
 *
 * This library provides:
 * - COBS framing for USB communication (HID/Serial)
 * - Postcard serialization/deserialization
 * - Protocol types and message handling
 * - Cheetah public key encoding
 * - Device connection management via WebHID/Web Serial API
 */

// Core serialization
export { PostcardReader, PostcardWriter } from './postcard';
export { COBSEncoder, COBSFrameReader } from './cobs';

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
} from './protocol';
export {
  serializeRequest,
  deserializeResponse,
  serializeMsg,
  deserializeMsg,
  getErrorMessage,
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
  PROTO_V1,
  ERR_BUSY,
  ERR_FLASH,
  ERR_CRYPTO,
} from './protocol';

// Cheetah public key encoding
export {
  serializeCheetahPublicKey,
  base58Encode,
  formatCheetahPubkey,
} from './cheetah';

// Device connection (WebHID/Web Serial API)
export { SigerDevice } from './device';
