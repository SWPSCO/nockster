# Nockster Wallet - Tauri Desktop Application

Secure cross-platform desktop application for the Nockchain hardware wallet.

## Features

- **Cross-platform**: Works on macOS, Linux, and Windows
- **Security-focused**:
  - Content Security Policy (CSP) enabled
  - Asset protocol scoping
  - Prototype freezing
  - No remote domain IPC access
- **WebUSB/WebSerial Support**: Direct communication with hardware wallet
- **WASM Integration**: Uses compiled Rust crypto libraries via WebAssembly
- **File Operations**: Save and load transaction drafts securely

## Building

### Prerequisites

1. Install Tauri CLI:
```bash
cargo install tauri-cli --version "^2.0.0"
```

2. Platform-specific dependencies:

**Linux (Debian/Ubuntu):**
```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev \
  build-essential \
  curl \
  wget \
  file \
  libssl-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libudev-dev \
  libusb-1.0-0-dev
```

**macOS:**
```bash
xcode-select --install
```

**Windows:**
- Install Microsoft Visual Studio C++ Build Tools
- Install WebView2 (usually pre-installed on Windows 10+)

### Development

Run the app in development mode with hot-reload:

```bash
make tauri-dev
```

Or manually:
```bash
cd src-tauri
cargo tauri dev
```

### Production Build

Build optimized binaries for your platform:

```bash
make tauri-build
```

This will create platform-specific installers in `src-tauri/target/release/bundle/`:
- **macOS**: `.app` bundle and `.dmg` installer
- **Linux**: `.deb`, `.AppImage`, and `.rpm` packages
- **Windows**: `.msi` installer and `.exe` setup

## Architecture

```
┌─────────────────────────────────────────┐
│         Tauri Desktop App               │
│  ┌───────────────────────────────────┐  │
│  │     Rust Backend (main.rs)        │  │
│  │  - Security policies              │  │
│  │  - File system access             │  │
│  │  - System dialogs                 │  │
│  └───────────────────────────────────┘  │
│                  │                       │
│                  │ IPC                   │
│                  ↓                       │
│  ┌───────────────────────────────────┐  │
│  │     WebView (React App)           │  │
│  │  - UI components                  │  │
│  │  - WebUSB/WebSerial               │  │
│  │  - WASM crypto (nockster-wasm)       │  │
│  │  - Device protocol (nockster-js)     │  │
│  └───────────────────────────────────┘  │
│                  │                       │
│                  ↓ USB                   │
│           Hardware Wallet                │
└─────────────────────────────────────────┘
```

## Security Features

### Content Security Policy
- Strict CSP prevents XSS attacks
- WASM execution allowed via `wasm-unsafe-eval`
- No external resources loaded

### Asset Protocol
- Scoped to bundled resources only
- Prevents unauthorized file system access

### Permissions
- Minimal permission set
- File operations require explicit user interaction
- No network access to external domains

### Prototype Freezing
- JavaScript prototypes frozen to prevent tampering
- Enhanced runtime security

## Configuration

Edit `src-tauri/tauri.conf.json` to customize:
- Window dimensions and behavior
- Security policies
- Bundle metadata
- Platform-specific settings

## Icons

Replace the placeholder icons in `src-tauri/icons/` with your app icons:
- `32x32.png` - Taskbar icon
- `128x128.png` - Application list
- `128x128@2x.png` - Retina display support
- `icon.icns` - macOS bundle icon
- `icon.ico` - Windows executable icon

Generate icons from a single source:
```bash
# From a 1024x1024 PNG source image
cargo tauri icon path/to/icon.png
```

## Debugging

### Enable DevTools

DevTools automatically open in debug builds. To debug a release build:

1. Set `TAURI_DEBUG` environment variable:
```bash
TAURI_DEBUG=1 ./target/release/nockster-wallet
```

2. Or enable in code (main.rs):
```rust
window.open_devtools();
```

### Console Logs

Frontend logs appear in DevTools console. Rust backend logs:
```bash
RUST_LOG=debug cargo tauri dev
```

## Distribution

### Code Signing

**macOS:**
```bash
# Set up Developer ID certificate
export APPLE_CERTIFICATE="Developer ID Application: Your Name"
export APPLE_SIGNING_IDENTITY="Your Apple ID"

# Build and sign
cargo tauri build
```

**Windows:**
```bash
# Set certificate thumbprint in tauri.conf.json
{
  "bundle": {
    "windows": {
      "certificateThumbprint": "YOUR_CERT_THUMBPRINT"
    }
  }
}
```

### Auto-updates

Tauri supports automatic updates. See [Tauri Updater docs](https://v2.tauri.app/plugin/updater/) for setup.

## Troubleshooting

### Linux: WebUSB not working
Add udev rules for your device:
```bash
echo 'SUBSYSTEM=="usb", ATTRS{idVendor}=="YOUR_VID", MODE="0666"' | \
  sudo tee /etc/udev/rules.d/99-nockster.rules
sudo udevadm control --reload-rules
```

### macOS: App won't open
Run from terminal to see error messages:
```bash
open -a "Nockster Wallet"
```

### Windows: WebView2 issues
Install/update WebView2 Runtime:
https://developer.microsoft.com/en-us/microsoft-edge/webview2/

## License

Same as the main nockster-esp project.
