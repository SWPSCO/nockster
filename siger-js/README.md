# siger-js

TypeScript/JavaScript library for communicating with Siger hardware wallet.

### Notes

- Web Serial only works in Chrome/Edge/Opera
- Web Serial requires secure context (HTTPS or localhost)
- Browser will prompt user to select serial port
- Only one connection per device at a time
- All integers (u16, u32, u64) use varint encoding in postcard

```
siger-js/
├── src/              # TypeScript source files
│   ├── index.ts      # Main entry point with exports
│   ├── postcard.ts   # Postcard serialization (Reader/Writer)
│   ├── protocol.ts   # Protocol types and message handlers
│   ├── cheetah.ts    # Cheetah public key encoding
│   ├── cobs.ts       # COBS framing
│   ├── device.t      # SigerDevice class (Web Serial)
│   └── types.d.ts    # Web Serial API type definitions
```

### 1. **COBS Framing** (`cobs.ts`)
- `COBSEncoder.encode()` - Encode messages for serial transmission
- `COBSFrameReader` - Stream-based frame extraction from serial data

### 2. **Postcard Serialization** (`postcard.ts`)
- `PostcardWriter` - Serialize messages to binary format
- `PostcardReader` - Deserialize binary messages
- Full varint support (u16, u32, u64 all use varint encoding)

### 3. **Protocol** (`protocol.ts`)
- Type definitions for all Request and Response variants
- `serializeMsg()` / `deserializeMsg()` - Message encoding/decoding
- `getErrorMessage()` - Human-readable error messages
- All error code constants exported

### 4. **Cheetah Crypto** (`cheetah.ts`)
- `formatCheetahPubkey()` - Convert (x, y) coordinates to base58 string
- `serializeCheetahPublicKey()` - 97-byte serialization format
- Compatible with nockchain's pubkey format

### 5. **Device Connection** (`device.ts`)
- `SigerDevice` class - High-level Web Serial API wrapper
- `connect()` / `disconnect()` - Connection management
- `call()` - Send request and wait for response
- Optional debug logging
- Automatic message ID tracking and response matching


## Exports

```typescript
// Classes
export { SigerDevice } from './device';
export { PostcardReader, PostcardWriter } from './postcard';
export { COBSEncoder, COBSFrameReader } from './cobs';

// Functions
export { serializeRequest, deserializeResponse } from './protocol';
export { serializeMsg, deserializeMsg } from './protocol';
export { getErrorMessage } from './protocol';
export { serializeCheetahPublicKey, base58Encode, formatCheetahPubkey } from './cheetah';

// Types
export type { Request, Response, Msg, Frame } from './protocol';

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
} from './protocol';
```

## Usage

```typescript
import { SigerDevice } from 'siger-js';

const device = new SigerDevice({ debug: false });

// check if Web Serial is supported
if (!SigerDevice.isSupported()) {
  console.error('Web Serial API not supported in this browser');
}

await device.connect({ baudRate: 115200 });

if (device.isConnected()) {
  console.log('Connected to Siger device');
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
    import { formatCheetahPubkey } from 'siger-js';
    const pubkey = formatCheetahPubkey(response.cheetah_x, response.cheetah_y);
    console.log('Public key:', pubkey);
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
```

```typescript
// initialize with pin
import { SigerDevice } from 'siger-js';

const device = new SigerDevice();
await device.connect({ baudRate: 115200 });

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
// errors
import { getErrorMessage, ERR_DEVICE_LOCKED, ERR_WRONG_PIN } from 'siger-js';

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
} from 'siger-js';

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
frameReader.push(encoded);
const frames = frameReader.getFrames();

// deserialize message
const decoded = deserializeMsg(frames[0]);
console.log('Received:', decoded);
```

## Protocol

The Siger protocol uses:

- `Postcard` for message serialization
- `COBS` for packet framing
- Varint encoding for integers (u16, u32, u64 are all varints)

### Requests

```typescript
type Request =
  | { type: 'Hello' }
  | { type: 'GetInfo' }
  | { type: 'Ping' }
  | { type: 'Unlock'; pin: string }
  | { type: 'Lock' }
  | { type: 'GetLockStatus' }
  | { type: 'InitializePIN'; pin: string; seed64: Uint8Array };
```

### Responses

```typescript
type Response =
  | { type: 'Hello'; proto_v: number; compressed_pk: boolean }
  | { type: 'Info'; proto_v: number; fw_major: number; fw_minor: number;
      features: number; has_seed: boolean; cheetah_x: bigint[]; cheetah_y: bigint[] }
  | { type: 'Pong' }
  | { type: 'Ok' }
  | { type: 'OkLockStatus'; locked: boolean; attempts_remaining: number }
  | { type: 'Err'; code: number };
```

## Cheetah pubkeys

Cheetah public keys are elliptic curve points represented as (x, y) coordinates, each consisting of 6 u64 limbs. The library provides utilities to serialize these to the 97-byte format used by nockchain and encode them as base58 strings.

```typescript
import { serializeCheetahPublicKey, base58Encode, formatCheetahPubkey } from 'siger-js';

// from device info response
const x = response.cheetah_x; // bigint[6]
const y = response.cheetah_y; // bigint[6]

// get base58 public key
const pubkey = formatCheetahPubkey(x, y);
// => "32bePYRuJ3heGVEbznc6xSCaTymgz9b..."
```
