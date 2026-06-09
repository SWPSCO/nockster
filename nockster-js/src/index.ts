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
  DeviceAddressBookEntry,
  VaultEntryInfo,
  UpdateTrust,
  UpdateStatus,
  UpdateBootStatus,
  UpdateManifest,
  UpdateBundle,
  UpdateReleaseIndexMetadata,
  UpdateReleaseIndex,
  FetchedUpdateRelease,
  UpdateCompatibilityOptions,
  UpdateStreamStatusExpectation,
  FetchUpdateReleaseArtifactsOptions,
  FetchLatestUpdateReleaseOptions,
} from './protocol.js';
export {
  serializeRequest,
  deserializeResponse,
  serializeMsg,
  deserializeMsg,
  getErrorMessage,
  parseUpdateBundleJson,
  parseUpdateReleaseIndexJson,
  assertUpdateReleaseIndexMatchesBundle,
  assertUpdateFirmwareMatchesBundle,
  getUpdateBundleCompatibilityBlocker,
  assertUpdateBundleCompatible,
  updateSlotName,
  updateOtaStateName,
  getPostInstallUpdateBootStatusFailures,
  assertPostInstallUpdateBootStatus,
  getUpdateStreamStatusFailures,
  assertUpdateStreamStatus,
  assertPrivateUpdateReleaseUrls,
  fetchUpdateReleaseArtifacts,
  fetchLatestUpdateRelease,
  serializeDeviceAddressBookEntries,
  deserializeDeviceAddressBookEntries,
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
  FEATURE_CHEETAH,
  FEATURE_FRAG,
  FEATURE_XPUB,
  FEATURE_SECURITY_STATUS,
  FEATURE_BUILD_INFO,
  FEATURE_TOUCH_CALIBRATION,
  FEATURE_TOUCH_DIAGNOSTICS,
  FEATURE_SEED_LABELS,
  FEATURE_PIN_CHANGE_UI,
  FEATURE_TOUCH_CALIBRATION_UI,
  FEATURE_SECURE_UPDATE,
  FEATURE_RELEASE_INFO,
  FEATURE_UPDATE_BOOT_STATUS,
  FEATURE_DEVICE_REBOOT,
  FEATURE_DEVICE_ADDRESS_BOOK,
  FEATURE_PREIMAGE_VAULT,
  FEATURE_MASTER_PUBKEY_EXPORT,
  MAX_VAULT_ENTRIES,
  MAX_VAULT_PREIMAGE_LEN,
  MAX_DEVICE_ADDRESS_BOOK_ENTRIES,
  MAX_ADDRESS_BOOK_LABEL_LEN,
  MAX_ADDRESS_BOOK_PKH_LEN,
  NOCKSTER_UPDATE_HARDWARE_TARGET,
  UPDATE_BUILD_PROFILE_DEV,
  UPDATE_BUILD_PROFILE_CHIP_SECURITY,
  UPDATE_BUILD_PROFILE_PRODUCTION,
  UPDATE_SLOT_NONE,
  UPDATE_SLOT_OTA0,
  UPDATE_SLOT_OTA1,
  UPDATE_SLOT_UNKNOWN,
  UPDATE_OTA_STATE_NEW,
  UPDATE_OTA_STATE_PENDING_VERIFY,
  UPDATE_OTA_STATE_VALID,
  UPDATE_OTA_STATE_INVALID,
  UPDATE_OTA_STATE_ABORTED,
  UPDATE_OTA_STATE_UNAVAILABLE,
  UPDATE_OTA_STATE_UNKNOWN,
  UPDATE_OTA_STATE_UNDEFINED,
  UPDATE_MANIFEST_VERSION,
  UPDATE_RELEASE_INDEX_FORMAT,
  MAX_UPDATE_RELEASE_VERSION,
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
