import { createHash, createPublicKey, verify } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { releaseMetadata } from './plan.mjs';

export const updateOrigin = 'https://bin.aeroe.io/fletch/updates';
const formats = {
  'linux-x64': [
    ['linux-x86_64-appimage', 'AppImage'],
    ['linux-x86_64-deb', 'deb']
  ],
  'macos-arm64': [['darwin-aarch64', 'app.tar.gz']],
  'windows-x64': [['windows-x86_64-nsis', 'exe']]
};

export function publicKey(value) {
  const lines = Buffer.from(value || '', 'base64')
    .toString('utf8')
    .trim()
    .split(/\r?\n/);
  const bytes = Buffer.from(lines[1] || '', 'base64');
  if (
    !lines[0]?.startsWith('untrusted comment: ') ||
    bytes.length !== 42 ||
    bytes.subarray(0, 2).toString() !== 'Ed'
  )
    throw new Error(
      'Set NOCKSTER_DESKTOP_UPDATER_PUBLIC_KEY to the contents of the Tauri .pub file'
    );
  return bytes;
}

// Verify the minisign artifact signature and its authenticated version using Node's
// Ed25519 implementation. Publication fails if CI signs with a different key.
export function verifyPackage(bytes, encodedSignature, encodedKey, version) {
  const keyBytes = publicKey(encodedKey);
  const lines = Buffer.from(encodedSignature.trim(), 'base64')
    .toString('utf8')
    .trim()
    .split(/\r?\n/);
  const signature = Buffer.from(lines[1] || '', 'base64');
  const globalSignature = Buffer.from(lines[3] || '', 'base64');
  if (
    signature.length !== 74 ||
    signature.subarray(0, 2).toString() !== 'ED' ||
    !signature.subarray(2, 10).equals(keyBytes.subarray(2, 10)) ||
    !lines[2]?.startsWith('trusted comment: ') ||
    globalSignature.length !== 64
  )
    throw new Error('Invalid updater signature');
  const comment = lines[2].slice('trusted comment: '.length);
  const key = createPublicKey({
    key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), keyBytes.subarray(10)]),
    format: 'der',
    type: 'spki'
  });
  if (
    !verify(null, createHash('blake2b512').update(bytes).digest(), key, signature.subarray(10)) ||
    !verify(
      null,
      Buffer.concat([signature.subarray(10), Buffer.from(comment)]),
      key,
      globalSignature
    )
  )
    throw new Error('Updater signature verification failed');
  if (comment.split('\t').find(field => field.startsWith('version:')) !== `version:${version}`)
    throw new Error('Updater signature version mismatch');
}

function releaseId() {
  const version = releaseMetadata(process.env.NOCKSTER_VERSION, 1, 1).version;
  const run = process.env.GITHUB_RUN_ID;
  const attempt = process.env.GITHUB_RUN_ATTEMPT;
  if (!/^\d+$/.test(run || '') || !/^\d+$/.test(attempt || ''))
    throw new Error('Missing GitHub release identity');
  return { version, build: `${run}.${attempt}` };
}

export function updatePolicy(policy) {
  if (
    !Number.isSafeInteger(policy.minimumUpdaterProtocol) ||
    policy.minimumUpdaterProtocol < 1 ||
    typeof policy.manualInstallRequired !== 'boolean'
  )
    throw new Error('Invalid desktop update policy');
  return {
    minimumUpdaterProtocol: policy.minimumUpdaterProtocol,
    manualInstallRequired: policy.manualInstallRequired
  };
}

export function packageUpdates(platform) {
  if (!formats[platform]) throw new Error('Unknown updater platform');
  const { version, build } = releaseId();
  const key = process.env.NOCKSTER_DESKTOP_UPDATER_PUBLIC_KEY;
  publicKey(key);
  if (!process.env.TAURI_SIGNING_PRIVATE_KEY)
    throw new Error('Set NOCKSTER_DESKTOP_UPDATER_PRIVATE_KEY in Actions secrets');
  const policy = updatePolicy(JSON.parse(readFileSync('apps/desktop/update-policy.json', 'utf8')));
  const output = 'release/desktop/updates';
  mkdirSync(output, { recursive: true });
  const platforms = {};
  for (const [target, extension] of formats[platform]) {
    const filename = `nockster-${platform}-${version}-${build}.${extension}`;
    const path = `${output}/${filename}`;
    if (platform === 'macos-arm64') {
      const app = 'apps/desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/macos';
      execFileSync('tar', ['-czf', resolve(path), '-C', app, 'Nockster.app']);
    } else {
      copyFileSync(`release/desktop/nockster-${platform}-latest.${extension}`, path);
    }
    execFileSync(
      process.execPath,
      [
        resolve('node_modules/@tauri-apps/cli/tauri.js'),
        'signer',
        'sign',
        '--app-version',
        version,
        path
      ],
      { stdio: 'pipe' }
    );
    const signature = readFileSync(`${path}.sig`, 'utf8').trim();
    verifyPackage(readFileSync(path), signature, key, version);
    platforms[target] = { url: `${updateOrigin}/${filename}`, signature };
  }
  writeFileSync(
    `${output}/${platform}.json`,
    JSON.stringify({ version, build, ...policy, platforms }, null, 2)
  );
}

export function mergeManifests(directory, encodedKey) {
  const parts = Object.keys(formats).map(platform =>
    JSON.parse(readFileSync(`${directory}/${platform}.json`, 'utf8'))
  );
  const { version, build } = parts[0];
  const policy = updatePolicy(parts[0]);
  releaseMetadata(version, 1, 1);
  if (!/^\d+\.\d+$/.test(build)) throw new Error('Invalid release identity');
  const platforms = {};
  for (const [index, part] of parts.entries()) {
    if (JSON.stringify(updatePolicy(part)) !== JSON.stringify(policy))
      throw new Error('Mixed updater policies');
    if (part.version !== version || part.build !== build) throw new Error('Mixed updater releases');
    const expected = formats[Object.keys(formats)[index]].map(([target]) => target).sort();
    if (JSON.stringify(Object.keys(part.platforms).sort()) !== JSON.stringify(expected))
      throw new Error('Missing updater target');
    for (const [target, entry] of Object.entries(part.platforms)) {
      const url = new URL(entry.url);
      if (
        url.origin !== new URL(updateOrigin).origin ||
        !url.pathname.startsWith('/fletch/updates/') ||
        url.search ||
        url.hash
      )
        throw new Error('Invalid updater URL');
      const filename = basename(url.pathname);
      if (!filename.includes(`-${version}-${build}.`))
        throw new Error('Updater URL does not match its release');
      verifyPackage(readFileSync(`${directory}/${filename}`), entry.signature, encodedKey, version);
      platforms[target] = entry;
    }
  }
  return { version, build, ...policy, platforms };
}

export function isNewerRelease(candidate, current) {
  if (!current) return true;
  releaseMetadata(current.version, 1, 1);
  if (!/^\d+\.\d+$/.test(current.build)) throw new Error('Invalid published release identity');
  const versionOrder = candidate.version
    .split('.')
    .map(Number)
    .reduce(
      (order, part, index) => order || Math.sign(part - Number(current.version.split('.')[index])),
      0
    );
  if (versionOrder !== 0) return versionOrder > 0;
  const [run, attempt] = candidate.build.split('.').map(BigInt);
  const [currentRun, currentAttempt] = current.build.split('.').map(BigInt);
  return run > currentRun || (run === currentRun && attempt > currentAttempt);
}

async function main() {
  const [action, platform] = process.argv.slice(2);
  if (action === 'package') return packageUpdates(platform);
  if (action !== 'publish-manifest') throw new Error('Expected package or publish-manifest');
  const manifest = mergeManifests(
    'release/desktop/updates',
    process.env.NOCKSTER_DESKTOP_UPDATER_PUBLIC_KEY
  );
  const response = await fetch(`${updateOrigin}/latest.json`, {
    cache: 'no-store',
    signal: AbortSignal.timeout(20000)
  });
  if (!response.ok && response.status !== 404)
    throw new Error(`Cannot read current updater release: ${response.status}`);
  const current = response.ok ? await response.json() : null;
  if (!isNewerRelease(manifest, current)) {
    console.log('A newer desktop release is already published. Skipping publication.');
    return;
  }
  writeFileSync('release/desktop/updates/latest.json', JSON.stringify(manifest, null, 2));
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href)
  await main();
