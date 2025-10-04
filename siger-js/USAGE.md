# Using siger-js in Your WASM Web Wallet

## Installation

From your wallet repository:

```bash
# Option 1: Install from local path
npm install ../siger-esp/siger-js

# Option 2: Use npm link
cd ../siger-esp/siger-js
npm link
cd -
npm link siger-js
```

## Quick Start Example

Here's a complete example of integrating Siger hardware wallet into your WASM web wallet:

```typescript
import { SigerDevice, formatCheetahPubkey, getErrorMessage } from 'siger-js';

class HardwareWalletSupport {
  private device: SigerDevice | null = null;

  async connect(): Promise<boolean> {
    if (!SigerDevice.isSupported()) {
      throw new Error('Web Serial not supported in this browser');
    }

    this.device = new SigerDevice({ debug: false });

    try {
      await this.device.connect({ baudRate: 115200 });
      return true;
    } catch (err) {
      console.error('Failed to connect:', err);
      return false;
    }
  }

  async getPublicKey(): Promise<string | null> {
    if (!this.device?.isConnected()) {
      throw new Error('Device not connected');
    }

    const response = await this.device.call({ type: 'GetInfo' });

    if (response.type === 'Info' && response.has_seed) {
      return formatCheetahPubkey(response.cheetah_x, response.cheetah_y);
    } else if (response.type === 'Err') {
      throw new Error(getErrorMessage(response.code));
    }

    return null;
  }

  async unlock(pin: string): Promise<boolean> {
    if (!this.device?.isConnected()) {
      throw new Error('Device not connected');
    }

    const response = await this.device.call({
      type: 'Unlock',
      pin
    });

    if (response.type === 'Ok') {
      return true;
    } else if (response.type === 'Err') {
      throw new Error(getErrorMessage(response.code));
    }

    return false;
  }

  async isLocked(): Promise<boolean> {
    if (!this.device?.isConnected()) {
      throw new Error('Device not connected');
    }

    const response = await this.device.call({ type: 'GetLockStatus' });

    if (response.type === 'OkLockStatus') {
      return response.locked;
    } else if (response.type === 'Err') {
      throw new Error(getErrorMessage(response.code));
    }

    return true;
  }

  disconnect() {
    this.device?.disconnect();
    this.device = null;
  }
}

// Usage in your wallet UI
const hw = new HardwareWalletSupport();

// Connect button handler
async function onConnectHardwareWallet() {
  try {
    await hw.connect();
    const pubkey = await hw.getPublicKey();
    console.log('Connected! Public key:', pubkey);
    // Display pubkey in UI, enable signing, etc.
  } catch (err) {
    console.error('Error:', err);
  }
}

// Unlock handler
async function onUnlock(pin: string) {
  try {
    await hw.unlock(pin);
    console.log('Device unlocked');
  } catch (err) {
    console.error('Unlock failed:', err);
  }
}
```

## React Integration

```typescript
import { useState, useCallback } from 'react';
import { SigerDevice, formatCheetahPubkey } from 'siger-js';

export function useHardwareWallet() {
  const [device, setDevice] = useState<SigerDevice | null>(null);
  const [pubkey, setPubkey] = useState<string | null>(null);
  const [connected, setConnected] = useState(false);
  const [locked, setLocked] = useState(true);

  const connect = useCallback(async () => {
    const dev = new SigerDevice();
    await dev.connect({ baudRate: 115200 });
    setDevice(dev);
    setConnected(true);

    const info = await dev.call({ type: 'GetInfo' });
    if (info.type === 'Info' && info.has_seed) {
      const pk = formatCheetahPubkey(info.cheetah_x, info.cheetah_y);
      setPubkey(pk);
    }

    const status = await dev.call({ type: 'GetLockStatus' });
    if (status.type === 'OkLockStatus') {
      setLocked(status.locked);
    }
  }, []);

  const unlock = useCallback(async (pin: string) => {
    if (!device) return;
    const response = await device.call({ type: 'Unlock', pin });
    if (response.type === 'Ok') {
      setLocked(false);
    }
  }, [device]);

  const disconnect = useCallback(() => {
    device?.disconnect();
    setDevice(null);
    setConnected(false);
    setPubkey(null);
  }, [device]);

  return {
    connect,
    disconnect,
    unlock,
    connected,
    locked,
    pubkey,
    device
  };
}

// Component
function HardwareWalletButton() {
  const { connect, disconnect, unlock, connected, locked, pubkey } = useHardwareWallet();
  const [pin, setPin] = useState('');

  if (!connected) {
    return <button onClick={connect}>Connect Hardware Wallet</button>;
  }

  return (
    <div>
      <p>Public Key: {pubkey}</p>
      {locked && (
        <div>
          <input
            type="password"
            value={pin}
            onChange={e => setPin(e.target.value)}
            placeholder="PIN"
          />
          <button onClick={() => unlock(pin)}>Unlock</button>
        </div>
      )}
      <button onClick={disconnect}>Disconnect</button>
    </div>
  );
}
```

## TypeScript Types

The library is fully typed. Import types as needed:

```typescript
import type {
  Request,
  Response,
  Msg,
  Frame
} from 'siger-js';

// Request types
const req1: Request = { type: 'Ping' };
const req2: Request = { type: 'Unlock', pin: '1234' };

// Response handler with exhaustive type checking
function handleResponse(response: Response) {
  switch (response.type) {
    case 'Info':
      console.log('Device info:', response.fw_major, response.fw_minor);
      break;
    case 'Ok':
      console.log('Success');
      break;
    case 'Err':
      console.error('Error code:', response.code);
      break;
    // TypeScript ensures all cases are handled
  }
}
```

## Error Codes

```typescript
import {
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
  getErrorMessage
} from 'siger-js';

// Check specific errors
const response = await device.call({ type: 'Unlock', pin });
if (response.type === 'Err') {
  if (response.code === ERR_WRONG_PIN) {
    console.log('Wrong PIN');
  } else if (response.code === ERR_PIN_LOCKED_OUT) {
    console.log('Device locked due to too many attempts');
  } else {
    console.log('Error:', getErrorMessage(response.code));
  }
}
```

## Low-Level Protocol Access

If you need to work with the raw protocol (e.g., for signing transactions):

```typescript
import {
  PostcardWriter,
  PostcardReader,
  COBSEncoder,
  serializeMsg,
  deserializeMsg,
  PROTO_V1
} from 'siger-js';

// Custom serialization
const writer = new PostcardWriter();
writer.writeU8(42);
writer.writeVarint(1000);
writer.writeString("hello");
const bytes = writer.toBytes();

// Custom deserialization
const reader = new PostcardReader(bytes);
const value = reader.readU8();
const varint = reader.readVarint();
const str = reader.readString();

// COBS encoding
const encoded = COBSEncoder.encode(bytes);

// Send raw bytes over device.port if needed
```

## Notes

- The device must be connected via USB
- User must grant permission in browser's serial port dialog
- Only one connection per device at a time
- Always disconnect when done to release the port
- Use `debug: true` in SigerDevice constructor for verbose console logging
