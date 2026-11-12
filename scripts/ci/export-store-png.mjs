import sharp from 'sharp';

// Simulator captures are opaque but include an alpha channel that App Store
// Connect rejects. Re-encode RGB losslessly without changing visible pixels.
const [source, destination] = process.argv.slice(2);
if (!source || !destination) throw new Error('Usage: export-store-png.mjs source.png output.png');
const capture = sharp(source);
const metadata = await capture.metadata();
if (![[1320, 2868], [2064, 2752]].some(([w, h]) => metadata.width === w && metadata.height === h))
  throw new Error('Unexpected App Store screenshot dimensions');
if (!(await capture.stats()).isOpaque) throw new Error('Capture unexpectedly contains transparency');
await capture.removeAlpha().png().toFile(destination);
