import { cpSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join } from 'node:path';

const [target, platform] = process.argv.slice(2);
const formats = {
  'x86_64-unknown-linux-gnu': ['deb', 'AppImage'],
  'aarch64-apple-darwin': ['dmg'],
  'x86_64-pc-windows-msvc': ['exe']
};
const platforms = {
  'x86_64-unknown-linux-gnu': 'linux-x64',
  'aarch64-apple-darwin': 'macos-arm64',
  'x86_64-pc-windows-msvc': 'windows-x64'
};
if (!formats[target] || platforms[target] !== platform) throw new Error('Unknown desktop target');
const output = 'release/desktop';
mkdirSync(output, { recursive: true });
if (platform !== 'macos-arm64') {
  const bundle = `apps/desktop/src-tauri/target/${target}/release/bundle`;
  for (const file of readdirSync(bundle, { recursive: true })) {
    if (formats[target].some(extension => file.endsWith(`.${extension}`)))
      cpSync(join(bundle, file), join(output, file.split(/[\\/]/).at(-1)));
  }
}
const files = readdirSync(output).filter(
  file =>
    !file.startsWith('nockster-') &&
    formats[target].some(extension => file.endsWith(`.${extension}`))
);
const latestFiles = [];
for (const extension of formats[target]) {
  const matches = files.filter(file => file.endsWith(`.${extension}`));
  if (matches.length !== 1)
    throw new Error(`Expected one ${platform} .${extension} installer, found ${matches.length}`);
  const latest = `nockster-${platform}-latest.${extension}`;
  cpSync(join(output, matches[0]), join(output, latest));
  latestFiles.push(latest);
  const digest = createHash('sha256')
    .update(readFileSync(join(output, latest)))
    .digest('hex');
  writeFileSync(join(output, `${latest}.sha256`), `${digest}  ${latest}\n`);
}
writeFileSync(
  join(output, `desktop-${platform}-SHA256SUMS`),
  [...files, ...latestFiles]
    .map(
      file =>
        `${createHash('sha256')
          .update(readFileSync(join(output, file)))
          .digest('hex')}  ${file}\n`
    )
    .join('')
);
