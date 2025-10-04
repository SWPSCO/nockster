# siger-js

TypeScript/JavaScript library for communicating with Siger hardware wallet.

## Features

- **Web Serial API** - Connect to Siger device via USB serial
- **COBS Framing** - Consistent Overhead Byte Stuffing for reliable message framing
- **Postcard Serialization** - Rust-compatible binary serialization format
- **Cheetah Public Keys** - Base58 encoding compatible with nockchain
- **Full Protocol Support** - All device commands (info, unlock, lock, sign, etc.)

## Installation

```bash
# Local development (from another repo)
npm install file:../siger-esp/siger-js

# Or using npm link
cd siger-esp/siger-js
npm link
cd ../your-wallet-repo
npm link siger-js
```

## Usage

### Basic Connection

```typescript
import { SigerDevice } from 'siger-js';

// Create device instance
const device = new SigerDevice({ debug: false });

// Check if Web Serial is supported
if (!SigerDevice.isSupported()) {
  console.error('Web Serial API not supported in this browser');
}

// Connect to device (prompts user to select serial port)
await device.connect({ baudRate: 115200 });

// Check connection
if (device.isConnected()) {
  console.log('Connected to Siger device');
}
```

### Get Device Info

```typescript
const response = await device.call({ type: 'GetInfo' });

if (response.type === 'Info') {
  console.log('Protocol version:', response.proto_v);
  console.log('Firmware:', `${response.fw_major}.${response.fw_minor}`);
  console.log('Has seed:', response.has_seed);

  if (response.has_seed) {
    // Format public key as base58
    import { formatCheetahPubkey } from 'siger-js';
    const pubkey = formatCheetahPubkey(response.cheetah_x, response.cheetah_y);
    console.log('Public key:', pubkey);
  }
}
```

### Lock/Unlock

```typescript
// Check lock status
const status = await device.call({ type: 'GetLockStatus' });
if (status.type === 'OkLockStatus') {
  console.log('Locked:', status.locked);
  console.log('Attempts remaining:', status.attempts_remaining);
}

// Unlock with PIN
const unlockResp = await device.call({
  type: 'Unlock',
  pin: '1234'
});

if (unlockResp.type === 'Ok') {
  console.log('Device unlocked');
} else if (unlockResp.type === 'Err') {
  console.error('Unlock failed:', getErrorMessage(unlockResp.code));
}

// Lock device
await device.call({ type: 'Lock' });
```

### Initialize Device

```typescript
import { SigerDevice } from 'siger-js';

const device = new SigerDevice();
await device.connect({ baudRate: 115200 });

// Initialize with PIN and seed (64 bytes)
const seed = new Uint8Array(64); // Your BIP39 seed
const response = await device.call({
  type: 'InitializePIN',
  pin: '1234',
  seed64: seed
});

if (response.type === 'Ok') {
  console.log('Device initialized');
}
```

### Error Handling

```typescript
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

### Low-Level Serialization

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

// Serialize a message
const msg = {
  v: PROTO_V1,
  id: 1,
  msg: { type: 'Ping' }
};
const serialized = serializeMsg(msg);

// Encode with COBS
const encoded = COBSEncoder.encode(serialized);

// Decode COBS frames
const frameReader = new COBSFrameReader();
frameReader.push(encoded);
const frames = frameReader.getFrames();

// Deserialize message
const decoded = deserializeMsg(frames[0]);
console.log('Received:', decoded);
```

## Protocol

The Siger protocol uses:

- **Postcard** for message serialization (compatible with Rust `postcard` crate)
- **COBS** for framing (Consistent Overhead Byte Stuffing)
- **Varint encoding** for integers (u16, u32, u64 are all varints)

### Request Types

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

### Response Types

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

## Cheetah Public Keys

Cheetah public keys are elliptic curve points represented as (x, y) coordinates, each consisting of 6 u64 limbs. The library provides utilities to serialize these to the 97-byte format used by nockchain and encode them as base58 strings.

```typescript
import { serializeCheetahPublicKey, base58Encode, formatCheetahPubkey } from 'siger-js';

// From device info response
const x = response.cheetah_x; // bigint[6]
const y = response.cheetah_y; // bigint[6]

// Get base58 public key
const pubkey = formatCheetahPubkey(x, y);
// => "32bePYRuJ3heGVEbznc6xSCaTymgz9b..."

// Or serialize manually
const serialized = serializeCheetahPublicKey(x, y); // Uint8Array(97)
const b58 = base58Encode(serialized);
```

## Browser Compatibility

Requires Web Serial API support:
- Chrome/Edge 89+
- Opera 76+
- Not supported in Firefox or Safari

## Development

```bash
# Build library
npm run build

# Use in another project
npm link
```

## License

Same as siger-esp parent project
