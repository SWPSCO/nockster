import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { dirname, extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const desk = resolve(process.argv[2] || join(root, 'zig-out'));
const release = JSON.parse(await readFile(join(root, 'release-version.json'), 'utf8'));
const output = join(root, 'release', 'urbit');
await mkdir(output, { recursive: true });
const files = [];
async function inventory(path, relative = '') {
  for (const entry of await readdir(path, { withFileTypes: true })) {
    const name = join(relative, entry.name);
    if (!/^[a-z0-9._/-]+$/.test(name)) throw new Error(`Invalid Clay filename: ${name}`);
    if (entry.isDirectory()) await inventory(join(path, entry.name), name);
    else if (entry.isFile()) {
      const content = await readFile(join(path, entry.name));
      files.push({ path: name, bytes: content.length, sha256: createHash('sha256').update(content).digest('hex') });
      if (name.startsWith('web/')) await readFile(join(desk, 'mar', extname(name).slice(1) + '.hoon'));
    } else throw new Error(`Unexpected non-file in desk: ${name}`);
  }
}
await inventory(desk);
if (!files.some(file => file.path.endsWith('.wasm')) || !files.some(file => file.path === 'web/index.html'))
  throw new Error('Build the complete Urbit desk before packaging.');
files.sort((a, b) => a.path.localeCompare(b.path));
const name = `nockster-urbit-${release.version}`;
await writeFile(join(output, `${name}-manifest.json`), JSON.stringify({ version: release.version, files }, null, 2) + '\n');
for (const [kind, source] of [['desk', desk], ['web', join(desk, 'web')], ['wasm', join(root, 'packages/wallet/src/pkg')]]) {
  execFileSync('tar', ['-czf', join(output, `${name}-${kind}.tar.gz`), '-C', source, '.'], { stdio: 'inherit' });
}
const sums = [];
for (const file of (await readdir(output)).filter(file => file.startsWith(name)).sort()) {
  sums.push(`${createHash('sha256').update(await readFile(join(output, file))).digest('hex')}  ${file}`);
}
await writeFile(join(output, 'SHA256SUMS'), sums.join('\n') + '\n');
console.log(`Urbit desk, static frontend, WASM module, and checksums: ${output}`);
