# Application Icons

This directory should contain the application icons in various formats.

## Required Icons

- `32x32.png` - Taskbar/notification icon
- `128x128.png` - Application list icon
- `128x128@2x.png` - High-DPI application icon
- `icon.icns` - macOS bundle icon
- `icon.ico` - Windows executable icon

## Generating Icons

The easiest way to generate all required icons is to use the Tauri CLI:

1. Create a high-resolution source image (1024x1024 PNG recommended)
2. Run the icon generation command:

```bash
cd src-tauri
cargo tauri icon /path/to/your-icon-1024.png
```

This will automatically generate all required sizes and formats.

## Manual Icon Creation

If you prefer to create icons manually:

### PNG Icons
Use any image editor to create the required sizes:
- Start with a 1024x1024 design
- Export at each required size
- Keep transparency if desired

### macOS (.icns)
Use `iconutil` on macOS:
```bash
mkdir icon.iconset
# Copy PNGs into iconset with specific names
iconutil -c icns icon.iconset
```

### Windows (.ico)
Use online converters or tools like ImageMagick:
```bash
convert icon.png -define icon:auto-resize=256,128,64,48,32,16 icon.ico
```

## Icon Guidelines

- **Use vector graphics** when possible for crisp rendering at all sizes
- **Keep it simple** - complex designs don't scale well to small sizes
- **High contrast** - ensure visibility on light and dark backgrounds
- **Square aspect ratio** - avoid non-square designs
- **Transparent background** - use PNG with alpha channel
- **Consistent branding** - match your app's visual identity

## Placeholder Icons

Until you create custom icons, Tauri will use default placeholder icons. The app will still function, but custom icons improve the user experience.
