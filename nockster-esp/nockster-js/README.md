# @swps/nockster-js

TypeScript/JavaScript library for communicating with Nockster hardware wallet.

### Notes

- WebHID/Web Serial only works in Chrome/Edge/Opera
- WebHID/Web Serial requires secure context (HTTPS or localhost)
- Browser will prompt user to select a device
- Only one connection per device at a time
- All integers (u16, u32, u64) use varint encoding in postcard
- `npm test` builds the package and runs parser/validation checks for signed
  firmware update bundles and latest-release indexes

```
nockster-js/
├── src/              # TypeScript source files
│   ├── index.ts      # Main entry point with exports
│   ├── postcard.ts   # Postcard serialization (Reader/Writer)
│   ├── protocol.ts   # Protocol types and message handlers
│   ├── cheetah.ts    # Cheetah public key encoding
│   ├── cobs.ts       # COBS framing
│   ├── device.ts     # NocksterDevice class (WebHID/Web Serial)
│   └── types.d.ts    # WebHID/Web Serial API type definitions
```

### 1. **COBS Framing** (`cobs.ts`)
- `COBSEncoder.encode()` - Encode messages for USB transmission
- `COBSFrameReader` - Stream-based frame extraction from USB byte streams

### 2. **Postcard Serialization** (`postcard.ts`)
- `PostcardWriter` - Serialize messages to binary format
- `PostcardReader` - Deserialize binary messages
- Full varint support (u16, u32, u64 all use varint encoding)

### 3. **Protocol** (`protocol.ts`)
- Type definitions for all Request and Response variants
- `serializeMsg()` / `deserializeMsg()` - Message encoding/decoding
- `getErrorMessage()` - Human-readable error messages
- `parseUpdateBundleJson()` / `parseUpdateReleaseIndexJson()` - Strict signed
  firmware update artifact parsing
- `assertUpdateReleaseIndexMatchesBundle()` - Catches stale latest-release
  index metadata before a firmware image is downloaded
- `assertUpdateFirmwareMatchesBundle()` - Checks downloaded firmware bytes
  against the signed manifest before streaming them to the device
- `getUpdateBundleCompatibilityBlocker()` / `assertUpdateBundleCompatible()` -
  Apply the browser update manifest policy for target hardware, protocol,
  rollback release, build profile, and image size before image transfer
- `getPostInstallUpdateBootStatusFailures()` /
  `assertPostInstallUpdateBootStatus()` - Validate that an installed OTA image
  is selected in a bootable `new` OTA slot before reporting success
- `getUpdateStreamStatusFailures()` / `assertUpdateStreamStatus()` - Validate
  begin/chunk/finish update progress against the signed manifest and expected
  byte offset before continuing a stream
- `fetchLatestUpdateRelease()` / `fetchUpdateReleaseArtifacts()` - Fetch
  hosted update artifacts with HTTPS/localhost enforcement, no-store caching,
  optional release-index metadata checks, firmware `Content-Length` checks, and
  manifest hash preflight. Pass `bearerToken` to apply the private-release
  origin/HTTPS policy, default tokened fetch credentials to `omit`, and attach
  the bearer header to latest-index and release artifact fetches.
- `assertPrivateUpdateReleaseUrls()` - Enforce the private bearer-token update
  policy: one artifact origin and HTTPS, except localhost testing
- All error code constants exported

### 4. **Cheetah Crypto** (`cheetah.ts`)
- `formatCheetahPubkey()` - Convert (x, y) coordinates to base58 string
- `serializeCheetahPublicKey()` - 97-byte serialization format

### 5. **Device Connection** (`device.ts`)
- `NocksterDevice` class - High-level WebHID/Web Serial API wrapper
- `connect()` / `disconnect()` - Connection management
- `call()` - Send request and wait for response
- Optional debug logging
- Automatic message ID tracking and response matching
- `initializePIN(pin, seed)` - Persist first seed and PIN
- `addSeed(seed)` - Append additional seed slots while unlocked
- `deleteSeed(slot)` - Remove a specific seed slot without wiping others
- `resetPIN(currentPin, newPin)` - Rotate the device PIN while unlocked
- `changePinOnDevice(currentPin)` - Rotate the PIN while entering the new PIN
  twice on-device
- `verifyUpdateBundle(bundle)` / `streamUpdateBundle(bundle, firmware)` -
  Exercise the signed firmware update path over WebHID/Web Serial
- `reboot()` - Ask firmware to perform a non-destructive reboot when the
  `FEATURE_DEVICE_REBOOT` bit is advertised


## Exports

```typescript
// Classes
export { NocksterDevice } from './device';
export { PostcardReader, PostcardWriter } from './postcard';
export { COBSEncoder, COBSFrameReader } from './cobs';

// Functions
export { serializeRequest, deserializeResponse } from './protocol';
export { serializeMsg, deserializeMsg } from './protocol';
export { getErrorMessage } from './protocol';
export { parseUpdateBundleJson, parseUpdateReleaseIndexJson } from './protocol';
export { assertUpdateReleaseIndexMatchesBundle, assertUpdateFirmwareMatchesBundle } from './protocol';
export { getUpdateBundleCompatibilityBlocker, assertUpdateBundleCompatible } from './protocol';
export { getPostInstallUpdateBootStatusFailures, assertPostInstallUpdateBootStatus } from './protocol';
export { getUpdateStreamStatusFailures, assertUpdateStreamStatus } from './protocol';
export { updateSlotName, updateOtaStateName } from './protocol';
export { assertPrivateUpdateReleaseUrls } from './protocol';
export { fetchLatestUpdateRelease, fetchUpdateReleaseArtifacts } from './protocol';
export { serializeCheetahPublicKey, base58Encode, formatCheetahPubkey } from './cheetah';

// Types
export type { Request, Response, Msg, Frame } from './protocol';
export type { UpdateBundle, UpdateReleaseIndex, UpdateReleaseIndexMetadata, FetchedUpdateRelease, UpdateCompatibilityOptions } from './protocol';

// Constants
export {
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
  PROTO_V1,
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
} from './protocol';
```

## Usage

```typescript
import { NocksterDevice } from '@swps/nockster-js';

const device = new NocksterDevice({ debug: false });

// check if WebHID/Web Serial is supported
if (!NocksterDevice.isSupported()) {
  console.error('WebHID/Web Serial API not supported in this browser');
}

await device.connect();

if (device.isConnected()) {
  console.log('Connected to Nockster device');
}
```

```typescript
// get device info
const response = await device.call({ type: 'GetInfo' });

if (response.type === 'Info') {
  console.log('Protocol version:', response.proto_v);
  console.log('Firmware:', `${response.fw_major}.${response.fw_minor}`);
  console.log('Has seed:', response.has_seed);

  if (response.has_seed) {
    import { formatCheetahPubkey } from '@swps/nockster-js';
    const first = response.cheetah_pubs[0];
    if (first) {
      const pubkey = formatCheetahPubkey(first.x, first.y);
      console.log('First public key:', pubkey);
      console.log('Derivation path:', ['m', ...first.path].join('/'));
    }
  }
}
```

```typescript
// check lock status
const status = await device.call({ type: 'GetLockStatus' });
if (status.type === 'OkLockStatus') {
  console.log('Locked:', status.locked);
  console.log('Attempts remaining:', status.attempts_remaining);
}

// unlock with pin
const unlockResp = await device.call({
  type: 'Unlock',
  pin: '1234'
});

if (unlockResp.type === 'Ok') {
  console.log('Device unlocked');
} else if (unlockResp.type === 'Err') {
  console.error('Unlock failed:', getErrorMessage(unlockResp.code));
}

// lock device
await device.call({ type: 'Lock' });

// delete a seed slot (device must be unlocked)
await device.deleteSeed(1);
```

```typescript
// initialize with pin
import { NocksterDevice } from '@swps/nockster-js';

const device = new NocksterDevice();
await device.connect();

const seed = new Uint8Array(64); // bip39 seed
const response = await device.call({
  type: 'InitializePIN',
  pin: '1234',
  seed64: seed
});

if (response.type === 'Ok') {
  console.log('Device initialized');
}
```

```typescript
// install a site-provided signed firmware update after fetching its artifacts
import {
  NocksterDevice,
  fetchLatestUpdateRelease,
} from '@swps/nockster-js';

const device = new NocksterDevice();
await device.connect();

const release = await fetchLatestUpdateRelease(new URL('/updates/latest.json', window.location.href));
await device.streamUpdateBundle(release.bundle, release.firmware, { writeFlash: true });
await device.reboot();
```

```typescript
// errors
import { getErrorMessage, ERR_DEVICE_LOCKED, ERR_WRONG_PIN } from '@swps/nockster-js';

const response = await device.call({ type: 'GetInfo' });

if (response.type === 'Err') {
  const message = getErrorMessage(response.code);
  console.error('Error:', message);

  if (response.code === ERR_DEVICE_LOCKED) {
    console.log('Device needs to be unlocked');
  }
}
```

### Serialization

If you need to work with the protocol directly:

```typescript
import {
  PostcardWriter,
  PostcardReader,
  COBSEncoder,
  COBSFrameReader,
  serializeMsg,
  deserializeMsg,
  PROTO_V1
} from '@swps/nockster-js';

// serialize a message
const msg = {
  v: PROTO_V1,
  id: 1,
  msg: { type: 'Ping' }
};
const serialized = serializeMsg(msg);

// encode with COBS
const encoded = COBSEncoder.encode(serialized);

// decode COBS frames
const frameReader = new COBSFrameReader();
const frames = frameReader.push(encoded);

// deserialize message
const decoded = deserializeMsg(frames[0]);
console.log('Received:', decoded);
```

## Protocol

The Nockster protocol uses:

- `Postcard` for message serialization
- `COBS` for packet framing
- Varint encoding for integers (u16, u32, u64 are all varints)

### Requests

```typescript
type Request =
  | { type: 'Hello' }
  | { type: 'GetInfo' }
  | { type: 'Ping' }
  | { type: 'SetSeed'; seed64: Uint8Array }
  | { type: 'Unlock'; pin: string }
  | { type: 'Lock' }
  | { type: 'GetLockStatus' }
  | { type: 'Reset' }
  | { type: 'InitializePIN'; pin: string; seed64: Uint8Array }
  | { type: 'GetCheetahPub'; path: number[] }
  | { type: 'SignSpendHash'; path: number[]; msg5: bigint[] };
```

### Responses

```typescript
type Response =
  | { type: 'Hello'; proto_v: number; compressed_pk: boolean }
  | { type: 'Info'; proto_v: number; fw_major: number; fw_minor: number;
      features: number; has_seed: boolean;
      cheetah_pubs: Array<{ path: number[]; x: bigint[]; y: bigint[] }> }
  | { type: 'Pong' }
  | { type: 'Ok' }
  | { type: 'OkLockStatus'; locked: boolean; attempts_remaining: number }
  | { type: 'OkCheetahPub'; x: bigint[]; y: bigint[] }
  | { type: 'OkCheetahSig'; chal: bigint[]; sig: bigint[] }
  | { type: 'Err'; code: number };
```

## Cheetah pubkeys

Cheetah public keys are elliptic curve points represented as (x, y) coordinates, each consisting of 6 u64 limbs. The library provides utilities to serialize these to the 97-byte format used by nockchain and encode them as base58 strings.

```typescript
import { serializeCheetahPublicKey, base58Encode, formatCheetahPubkey } from '@swps/nockster-js';

// from device info response (first pubkey)
const [{ x, y }] = response.cheetah_pubs;

// get base58 public key
const pubkey = formatCheetahPubkey(x, y);
// => "32bePYRuJ3heGVEbznc6xSCaTymgz9b..."
```
