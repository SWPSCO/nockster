import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import {
  isNewerRelease,
  mergeManifests,
  publicKey,
  updatePolicy,
  verifyPackage,
  updateOrigin
} from '../scripts/ci/desktop-updates.mjs';

const cli = resolve('node_modules/@tauri-apps/cli/tauri.js');
const run = (...args: string[]) =>
  execFileSync(process.execPath, [cli, ...args], { stdio: 'pipe' });

test('updater signatures bind the bytes, signing key, and release version', () => {
  const dir = mkdtempSync(join(tmpdir(), 'nockster-signing-'));
  try {
    const key = join(dir, 'updater.key');
    run('signer', 'generate', '--ci', '-p', '', '-w', key);
    const pubkey = readFileSync(`${key}.pub`, 'utf8').trim();
    const file = join(dir, 'package');
    const bytes = Buffer.from('Nockster updater test package\n');
    writeFileSync(file, bytes);
    run('signer', 'sign', '-f', key, '-p', '', '--app-version', '9.8.7', file);
    const signature = readFileSync(`${file}.sig`, 'utf8');
    verifyPackage(bytes, signature, pubkey, '9.8.7');
    assert.throws(() => verifyPackage(Buffer.from('tampered'), signature, pubkey, '9.8.7'));
    assert.throws(() => verifyPackage(bytes, signature, pubkey, '9.8.8'));
    const altered = Buffer.from(
      Buffer.from(signature.trim(), 'base64').toString().replace('version:9.8.7', 'version:9.8.8')
    ).toString('base64');
    assert.throws(() => verifyPackage(bytes, altered, pubkey, '9.8.8'));
    run('signer', 'generate', '--ci', '-p', '', '-w', join(dir, 'other.key'));
    assert.throws(() =>
      verifyPackage(bytes, signature, readFileSync(join(dir, 'other.key.pub'), 'utf8'), '9.8.7')
    );
    assert.throws(() => publicKey(''));
    assert.throws(() => publicKey('not a key'));

    const build = '123.1';
    const version = '9.8.7';
    const groups = {
      'linux-x64': ['linux-x86_64-appimage', 'linux-x86_64-deb'],
      'macos-arm64': ['darwin-aarch64'],
      'windows-x64': ['windows-x86_64-nsis']
    };
    for (const [platform, targets] of Object.entries(groups)) {
      const platforms = {};
      for (const target of targets) {
        const filename = `nockster-${target}-${version}-${build}.bin`;
        writeFileSync(join(dir, filename), bytes);
        platforms[target] = { url: `${updateOrigin}/${filename}`, signature };
      }
      writeFileSync(
        join(dir, `${platform}.json`),
        JSON.stringify({
          version,
          build,
          minimumUpdaterProtocol: 1,
          manualInstallRequired: false,
          platforms
        })
      );
    }
    assert.equal(Object.keys(mergeManifests(dir, pubkey).platforms).length, 4);
    const part = JSON.parse(readFileSync(join(dir, 'macos-arm64.json'), 'utf8'));
    part.version = '9.8.8';
    writeFileSync(join(dir, 'macos-arm64.json'), JSON.stringify(part));
    assert.throws(() => mergeManifests(dir, pubkey), /Mixed updater releases/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('publication ordering cannot replace a newer release with an older build', () => {
  assert.equal(isNewerRelease({ version: '1.0.11', build: '100.1' }, null), true);
  const current = { version: '1.0.11', build: '100.2' };
  assert.equal(isNewerRelease({ version: '1.0.10', build: '101.1' }, current), false);
  assert.equal(isNewerRelease({ version: '1.0.11', build: '100.1' }, current), false);
  assert.equal(isNewerRelease(current, current), false);
  assert.equal(isNewerRelease({ version: '1.0.11', build: '101.1' }, current), true);
  assert.equal(isNewerRelease({ version: '1.0.12', build: '100.1' }, current), true);
  assert.throws(() => updatePolicy({ minimumUpdaterProtocol: 0, manualInstallRequired: false }));
  assert.throws(() => updatePolicy({ minimumUpdaterProtocol: 1, manualInstallRequired: 'yes' }));
  assert.deepEqual(updatePolicy({ minimumUpdaterProtocol: 2, manualInstallRequired: true }), {
    minimumUpdaterProtocol: 2,
    manualInstallRequired: true
  });
});

test('CI packages every installer, embeds the public key, and verifies the merged feed', () => {
  const dir = mkdtempSync(join(tmpdir(), 'nockster-release-signing-'));
  try {
    const key = join(dir, 'updater.key');
    run('signer', 'generate', '--ci', '-p', '', '-w', key);
    const pubkey = readFileSync(`${key}.pub`, 'utf8').trim();
    mkdirSync(join(dir, 'apps/desktop/src-tauri'), { recursive: true });
    mkdirSync(join(dir, 'release/desktop'), { recursive: true });
    mkdirSync(join(dir, 'node_modules/@tauri-apps/cli'), { recursive: true });
    // The wrapper runs the installed CLI while each packaging process keeps its isolated cwd.
    writeFileSync(
      join(dir, 'node_modules/@tauri-apps/cli/tauri.js'),
      `require(${JSON.stringify(cli)});`
    );
    const app = join(
      dir,
      'apps/desktop/src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Nockster.app/Contents'
    );
    mkdirSync(app, { recursive: true });
    writeFileSync(join(app, 'test-package'), 'signed app fixture');
    for (const name of [
      'linux-x64-latest.deb',
      'linux-x64-latest.AppImage',
      'windows-x64-latest.exe'
    ])
      writeFileSync(join(dir, `release/desktop/nockster-${name}`), `installer fixture ${name}`);
    writeFileSync(
      join(dir, 'apps/desktop/update-policy.json'),
      JSON.stringify({ minimumUpdaterProtocol: 2, manualInstallRequired: true })
    );
    const env = {
      ...process.env,
      NOCKSTER_VERSION: '9.8.7',
      GITHUB_RUN_ID: '123',
      GITHUB_RUN_ATTEMPT: '1',
      NOCKSTER_DESKTOP_UPDATER_PUBLIC_KEY: pubkey,
      TAURI_SIGNING_PRIVATE_KEY: readFileSync(key, 'utf8'),
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ''
    };
    execFileSync(process.execPath, [resolve('scripts/ci/configure-desktop.mjs')], {
      cwd: dir,
      env,
      stdio: 'pipe'
    });
    const config = JSON.parse(
      readFileSync(join(dir, 'apps/desktop/src-tauri/tauri.ci.conf.json'), 'utf8')
    );
    assert.equal(config.version, '9.8.7');
    assert.equal(config.plugins.updater.pubkey, pubkey);
    for (const platform of ['linux-x64', 'macos-arm64', 'windows-x64'])
      execFileSync(
        process.execPath,
        [resolve('scripts/ci/desktop-updates.mjs'), 'package', platform],
        { cwd: dir, env, stdio: 'pipe' }
      );
    const feed = mergeManifests(join(dir, 'release/desktop/updates'), pubkey);
    assert.equal(feed.version, '9.8.7');
    assert.equal(feed.minimumUpdaterProtocol, 2);
    assert.equal(feed.manualInstallRequired, true);
    assert.equal(Object.keys(feed.platforms).length, 4);
    const entry = feed.platforms['linux-x86_64-deb'];
    writeFileSync(
      join(dir, 'release/desktop/updates', new URL(entry.url).pathname.split('/').at(-1)!),
      'tampered'
    );
    assert.throws(() => mergeManifests(join(dir, 'release/desktop/updates'), pubkey));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
