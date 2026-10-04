const sharp = require('sharp');
const fs = require('node:fs/promises');
const path = require('node:path');

const root = path.resolve(__dirname, '..');
const android = 'apps/mobile/android/app/src/main/res';
const ios = 'apps/mobile/ios/App/App/Assets.xcassets';

async function generateIcons() {
  const svg = await fs.readFile(path.join(root, 'packages/wallet/public/nockster-logo.svg'));
  await fs.copyFile(
    path.join(root, 'packages/wallet/public/nockster-logo.svg'),
    path.join(root, 'apps/extension/static/nockster-logo.svg')
  );

  async function render(file, size, markSize, { transparent = false, round = false } = {}) {
    const mark = await sharp(svg).resize(markSize, markSize).png().toBuffer();
    const layers = [{ input: mark, gravity: 'centre' }];
    if (round) {
      layers.push({
        input: Buffer.from(
          `<svg width="${size}" height="${size}"><circle cx="${size / 2}" cy="${size / 2}" r="${size / 2}" fill="white"/></svg>`
        ),
        blend: 'dest-in'
      });
    }
    let output = sharp({
      create: {
        width: size,
        height: size,
        channels: 4,
        background: transparent ? { r: 0, g: 0, b: 0, alpha: 0 } : '#ffffff'
      }
    }).composite(layers);
    if (!transparent && !round) output = output.removeAlpha();
    await output.png().toFile(path.join(root, file));
  }

  for (const size of [16, 48, 128]) {
    await render(`apps/extension/ext/icons/icon-${size}.png`, size, size, { transparent: true });
  }
  await render(`${ios}/AppIcon.appiconset/AppIcon-512@2x.png`, 1024, 760);
  await render(`${ios}/NocksterMark.imageset/nockster-mark.png`, 256, 256, { transparent: true });
  await render(`${android}/drawable/nockster_mark.png`, 256, 256, { transparent: true });

  for (const [density, scale] of Object.entries({
    mdpi: 1,
    hdpi: 1.5,
    xhdpi: 2,
    xxhdpi: 3,
    xxxhdpi: 4
  })) {
    const dir = `${android}/mipmap-${density}`;
    const size = 48 * scale;
    await render(`${dir}/ic_launcher.png`, size, Math.round(size * 0.82));
    await render(`${dir}/ic_launcher_round.png`, size, Math.round(size * 0.82), { round: true });
    // The mark fits inside Android's adaptive-icon safe zone for every launcher mask.
    await render(`${dir}/ic_launcher_foreground.png`, 108 * scale, 64 * scale, {
      transparent: true
    });
  }

  for (const name of await fs.readdir(path.join(root, `${ios}/Splash.imageset`))) {
    if (name.endsWith('.png')) await render(`${ios}/Splash.imageset/${name}`, 1366, 220);
  }
  for (const entry of await fs.readdir(path.join(root, android), { withFileTypes: true })) {
    if (!entry.isDirectory() || !entry.name.startsWith('drawable')) continue;
    const file = `${android}/${entry.name}/splash.png`;
    try {
      await fs.access(path.join(root, file));
    } catch (error) {
      if (error.code === 'ENOENT') continue;
      throw error;
    }
    await render(file, 480, 160);
  }
  console.log('Generated extension, iOS, and Android assets from packages/wallet/public/nockster-logo.svg.');
}

generateIcons().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
