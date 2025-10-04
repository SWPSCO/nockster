# siger-js Library Structure

## What Was Created

A standalone TypeScript library extracted from the web UI that can be imported into any JavaScript/TypeScript project, including your WASM web wallet.

## Directory Structure

```
siger-js/
├── src/                 # TypeScript source files
│   ├── index.ts        # Main entry point with exports
│   ├── postcard.ts     # Postcard serialization (Reader/Writer)
│   ├── protocol.ts     # Protocol types and message handlers
│   ├── cheetah.ts      # Cheetah public key encoding
│   ├── cobs.ts         # COBS framing
│   ├── device.ts       # SigerDevice class (Web Serial)
│   └── types.d.ts      # Web Serial API type definitions
├── dist/               # Compiled JavaScript + type definitions
├── package.json        # NPM package metadata
├── tsconfig.json       # TypeScript compiler config
├── README.md          # Library documentation
├── USAGE.md           # Integration examples
└── .gitignore         # Git ignore rules
```

## Key Features

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

## Exported API

### Main Exports (from `index.ts`)

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

## Installation Methods

### Method 1: Local Path (Recommended for Development)

```bash
# From your wallet repo
npm install ../siger-esp/siger-js
```

### Method 2: npm link (Alternative)

```bash
# In siger-js directory
npm link

# In your wallet repo
npm link siger-js
```

### Method 3: Copy to your monorepo

```bash
cp -r siger-esp/siger-js your-wallet-repo/packages/
```

## TypeScript Support

The library is fully typed with `.d.ts` declaration files generated for all exports. Your IDE will have full autocomplete and type checking.

## Dependencies

Only one runtime dependency:
- `bs58` - Base58 encoding (used for public keys)

Dev dependency:
- `typescript` - For building the library

## Building

```bash
cd siger-js
npm install
npm run build
```

This compiles TypeScript to JavaScript in the `dist/` directory.

## Usage in Your WASM Wallet

```typescript
import { SigerDevice, formatCheetahPubkey } from 'siger-js';

// Create device with optional debug logging
const device = new SigerDevice({ debug: false });

// Connect via Web Serial API
await device.connect({ baudRate: 115200 });

// Get device info and public key
const info = await device.call({ type: 'GetInfo' });
if (info.type === 'Info' && info.has_seed) {
  const pubkey = formatCheetahPubkey(info.cheetah_x, info.cheetah_y);
  console.log('Public key:', pubkey);
}

// Unlock device
await device.call({ type: 'Unlock', pin: '1234' });

// Clean up
device.disconnect();
```

See `USAGE.md` for complete integration examples including React hooks.

## Important Notes

1. **Web Serial API** - Only works in Chrome/Edge/Opera browsers
2. **HTTPS Required** - Web Serial requires secure context (HTTPS or localhost)
3. **User Permission** - Browser will prompt user to select serial port
4. **Single Connection** - Only one connection per device at a time
5. **Varint Encoding** - All integers (u16, u32, u64) use varint encoding in postcard

## Differences from Web UI

The library version:
- No React dependencies
- No UI components
- Removed debug console.log calls (optional via `debug: true`)
- Clean API surface with explicit exports
- Suitable for any JavaScript environment (not just web UI)

## Testing the Library

You can test the library works by creating a simple test file:

```typescript
// test.ts
import { SigerDevice } from 'siger-js';

async function test() {
  const device = new SigerDevice({ debug: true });
  await device.connect({ baudRate: 115200 });
  const info = await device.call({ type: 'GetInfo' });
  console.log('Device info:', info);
  device.disconnect();
}

test();
```

## Next Steps

1. Install the library in your wallet repo
2. Import `SigerDevice` and other exports as needed
3. Implement hardware wallet connection UI
4. Use the device for transaction signing
5. Handle errors with `getErrorMessage()`

The library provides all the low-level primitives you need while staying out of your way for UI and application logic.
