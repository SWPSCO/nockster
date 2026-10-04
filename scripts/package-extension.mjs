import { createHash, createPrivateKey, createPublicKey, verify, constants } from 'node:crypto';
import { cp, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export const EXTENSION_ID = 'nalokkijbnoknjoojndhgdlapkfjgamb';
const digest = bytes => createHash('sha256').update(bytes).digest();
const idFromBytes = bytes =>
  Array.from(bytes, byte => String.fromCharCode(97 + (byte >> 4), 97 + (byte & 15))).join('');
export const extensionId = publicKey => idFromBytes(digest(publicKey).subarray(0, 16));

// CRX3's header and proofs contain length-delimited protobuf fields.
function fields(bytes) {
  const result = new Map();
  let offset = 0;
  function integer() {
    let value = 0;
    for (let shift = 0; shift <= 28 && offset < bytes.length; shift += 7) {
      const byte = bytes[offset++];
      value += (byte & 127) * 2 ** shift;
      if (!(byte & 128)) return value;
    }
    throw new Error('Invalid CRX protobuf integer');
  }
  while (offset < bytes.length) {
    const tag = integer();
    if ((tag & 7) !== 2) throw new Error('Invalid CRX protobuf field');
    const size = integer();
    if (size > bytes.length - offset) throw new Error('Truncated CRX protobuf field');
    const field = Math.floor(tag / 8);
    const values = result.get(field) ?? [];
    values.push(bytes.subarray(offset, offset + size));
    result.set(field, values);
    offset += size;
  }
  return result;
}
function single(values, number) {
  const found = values.get(number);
  if (found?.length !== 1) throw new Error('Missing or repeated CRX field');
  return found[0];
}

export function verifyCrx(bytes, expectedId) {
  if (bytes.length < 12 || bytes.toString('ascii', 0, 4) !== 'Cr24' || bytes.readUInt32LE(4) !== 3)
    throw new Error('Expected a CRX3 package');
  const length = bytes.readUInt32LE(8);
  if (length > 1024 * 1024 || length > bytes.length - 12)
    throw new Error('Invalid CRX header size');
  const header = fields(bytes.subarray(12, 12 + length));
  const signed = single(header, 10000);
  const declaredId = single(fields(signed), 1);
  if (declaredId.length !== 16 || idFromBytes(declaredId) !== expectedId)
    throw new Error('CRX extension ID does not match the signing identity');
  const archive = bytes.subarray(12 + length);
  if (archive.readUInt32LE(0) !== 0x04034b50) throw new Error('Missing CRX ZIP archive');
  const size = Buffer.alloc(4);
  size.writeUInt32LE(signed.length);
  const payload = Buffer.concat([Buffer.from('CRX3 SignedData\0'), size, signed, archive]);
  for (const proof of header.get(2) ?? []) {
    const values = fields(proof);
    const publicKey = single(values, 1);
    if (extensionId(publicKey) !== expectedId) continue;
    if (
      verify(
        'sha256',
        payload,
        {
          key: createPublicKey({ key: publicKey, format: 'der', type: 'spki' }),
          padding: constants.RSA_PKCS1_PADDING
        },
        single(values, 2)
      )
    )
      return archive;
  }
  throw new Error('CRX signature verification failed');
}

export async function packageExtension({
  source = resolve('apps/extension/ext'),
  output = resolve('release/extension'),
  pemBase64 = process.env.FLETCH_EXT_KEY_B64,
  expectedId = EXTENSION_ID,
  chrome = process.env.CHROME_BIN || 'google-chrome'
} = {}) {
  const temporary = await mkdtemp(join(tmpdir(), 'nockster-package-'));
  const stage = join(temporary, 'extension');
  let pem;
  try {
    let publicKey;
    if (pemBase64) {
      pem = Buffer.from(pemBase64, 'base64');
      const privateKey = createPrivateKey({ key: pem, passphrase: '' });
      if (privateKey.asymmetricKeyType !== 'rsa')
        throw new Error('The extension signing key must be RSA');
      publicKey = createPublicKey(privateKey).export({ format: 'der', type: 'spki' });
      if (extensionId(publicKey) !== expectedId)
        throw new Error('Signing key does not match the Nockster extension ID');
    }
    await mkdir(stage);
    // Only these public build inputs enter the package. Keep dist/ paths intact for WASM loading.
    for (const entry of ['manifest.json', 'dist', 'icons'])
      await cp(join(source, entry), join(stage, entry), { recursive: true });
    const manifest = JSON.parse(await readFile(join(stage, 'manifest.json'), 'utf8'));
    if (!/^\d+(\.\d+){1,3}$/.test(manifest.version)) throw new Error('Invalid extension version');
    if (manifest.background?.service_worker !== 'dist/background.js')
      throw new Error('Unexpected extension entry point');
    await readFile(join(stage, 'dist/nockster_core_bg.wasm'));
    if (publicKey) {
      manifest.key = publicKey.toString('base64');
      await writeFile(join(stage, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
    }
    let crx;
    if (pem) {
      const keyPath = join(temporary, 'signing.pem');
      await writeFile(keyPath, pem, { mode: 0o600 });
      try {
        execFileSync(
          chrome,
          [
            '--headless',
            '--no-sandbox',
            '--disable-gpu',
            `--user-data-dir=${join(temporary, 'chrome')}`,
            `--pack-extension=${stage}`,
            `--pack-extension-key=${keyPath}`
          ],
          { stdio: 'pipe', timeout: 120_000 }
        );
        crx = await readFile(`${stage}.crx`);
        verifyCrx(crx, expectedId);
      } finally {
        await rm(keyPath, { force: true });
      }
    }
    // Store uploads omit the manifest key; the CRX retains its signing identity.
    delete manifest.key;
    await writeFile(join(stage, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
    const zipPath = join(temporary, 'verified.zip');
    execFileSync('zip', ['-q', '-r', '-X', zipPath, '.'], { cwd: stage });
    const archive = await readFile(zipPath);
    const entries = execFileSync('unzip', ['-Z1', zipPath], { encoding: 'utf8' })
      .trim()
      .split('\n');
    if (
      entries.some(
        entry => !/^(manifest\.json$|dist\/|icons\/)/.test(entry) || /\.(pem|key)$/i.test(entry)
      )
    )
      throw new Error('Unexpected file in extension archive');
    await mkdir(output, { recursive: true });
    const name = `nockster-extension-${manifest.version}`;
    const files = [[`${name}.zip`, archive]];
    if (crx) files.push([`${name}.crx`, crx]);
    for (const [name, contents] of files) await writeFile(join(output, name), contents);
    await writeFile(
      join(output, 'SHA256SUMS'),
      files.map(([name, bytes]) => `${digest(bytes).toString('hex')}  ${name}\n`).join('')
    );
    const release = {
      version: manifest.version,
      signed: Boolean(crx),
      extensionId: crx ? expectedId : null,
      commit: process.env.GITHUB_SHA ?? null,
      files: files.map(([name]) => name)
    };
    await writeFile(join(output, 'release.json'), JSON.stringify(release, null, 2) + '\n');
    return release;
  } finally {
    pem?.fill(0);
    await rm(temporary, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const pemBase64 = process.env.FLETCH_EXT_KEY_B64;
  delete process.env.FLETCH_EXT_KEY_B64;
  packageExtension({ pemBase64 })
    .then(result => console.log(JSON.stringify(result, null, 2)))
    .catch(error => {
      console.error(error.message);
      process.exitCode = 1;
    });
}
