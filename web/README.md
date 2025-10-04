# siger-wasm/siger-js demo webui

Browser-based interface for Siger hardware wallet using Web Serial API

## features

- Connect to ESP32-S3 device via USB
- Check device status (locked/unlocked, pin attempts)
- Lock/unlock with pin
- View device info (firmware version, has seed)
- Connectivity test

## requirements

- Chrome, Edge, or Opera (Web Serial API support)
- Siger hardware wallet (ESP32-S3)

## Quick Start

```bash
npm install
npm run dev
```

## Protocol

- 0x00-terminated COBS frames
- Postcard binary format
- 115200 baud

## Usage

1. Click "Connect Device"
2. Select "USB JTAG/serial debug unit" from browser prompt
   - **Note:** This generic name is a limitation of the ESP32-S3's built-in USB-JTAG peripheral and cannot be customized :(
4. Enter pin and click "Unlock" to access wallet
5. Click "Lock" to clear seed from memory

## Security

- Pin is sent over USB serial (not exposed to network)
- Seed remains encrypted in device NVS flash
- Browser cannot access seed directly
- Device must be unlocked for each operation
- Works offline (no internet required)

## Compatibility

| Browser | Web Serial | Status |
|---------|-----------|--------|
| Chrome  |        | Supported |
| Edge    |        | Supported |
| Opera   |        | Supported |
| Firefox | ❌        | Not yet |
| Safari  | ❌        | Not yet |

## Deployment

```bash
# Build for production
npm run build

# Preview production build
npm run preview

# Deploy to GitHub Pages, Vercel, etc.
# Requires HTTPS for Web Serial API
```

## Future Features

- [ ] Transaction signing UI
- [ ] Draft file upload and signing
- [ ] Display transaction details
- [ ] Export signed transactions
- [ ] BIP-39 seed initialization
- [ ] QR code scanning for drafts
