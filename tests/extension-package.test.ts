import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync } from 'node:crypto';
import { mkdtemp, mkdir, writeFile, readFile, rm } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { packageExtension, extensionId, verifyCrx } from '../scripts/package-extension.mjs';

test('store ZIPs omit the manifest key while signed CRXs retain their identity and matching payloads', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'nockster-package-test-'));
  try {
    const source = join(dir, 'ext');
    await mkdir(join(source, 'dist'), { recursive: true });
    await mkdir(join(source, 'icons'));
    await writeFile(
      join(source, 'manifest.json'),
      JSON.stringify({
        manifest_version: 3,
        name: 'Packaging test',
        version: '1.2.3',
        background: { service_worker: 'dist/background.js', type: 'module' }
      })
    );
    await writeFile(join(source, 'dist/background.js'), 'console.log("Package test");');
    await writeFile(
      join(source, 'dist/nockster_core_bg.wasm'),
      Buffer.from([0, 97, 115, 109, 1, 0, 0, 0])
    );
    await writeFile(join(source, 'private.pem'), 'This file must not enter a package');
    const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
    const der = publicKey.export({ type: 'spki', format: 'der' });
    const expectedId = extensionId(der);
    const pemBase64 = Buffer.from(privateKey.export({ type: 'pkcs8', format: 'pem' })).toString(
      'base64'
    );
    const output = join(dir, 'output');
    const result = await packageExtension({ source, output, pemBase64, expectedId });
    assert.equal(result.extensionId, expectedId);
    const crx = await readFile(join(output, 'nockster-extension-1.2.3.crx'));
    const crxZip = join(dir, 'crx.zip');
    await writeFile(crxZip, verifyCrx(crx, expectedId));
    const manifest = JSON.parse(
      execFileSync('unzip', ['-p', join(output, result.files[0]), 'manifest.json'], {
        encoding: 'utf8'
      })
    );
    assert.equal(Object.hasOwn(manifest, 'key'), false);
    const signedManifest = JSON.parse(execFileSync('unzip', ['-p', crxZip, 'manifest.json'], { encoding: 'utf8' }));
    assert.equal(signedManifest.key, der.toString('base64'));
    delete signedManifest.key;
    assert.deepEqual(manifest, signedManifest);
    assert.equal(manifest.background.service_worker, 'dist/background.js');
    const entries = execFileSync('unzip', ['-Z1', join(output, result.files[0])], {
      encoding: 'utf8'
    });
    assert.match(entries, /dist\/nockster_core_bg.wasm/);
    assert.doesNotMatch(entries, /\.pem|\.key/);
    const signedEntries = execFileSync('unzip', ['-Z1', crxZip], { encoding: 'utf8' });
    const files = (listing: string) => listing.trim().split('\n').filter(path => !path.endsWith('/')).sort();
    assert.deepEqual(files(entries), files(signedEntries));
    for (const path of files(entries).filter(path => path !== 'manifest.json'))
      assert.deepEqual(execFileSync('unzip', ['-p', join(output, result.files[0]), path]),
        execFileSync('unzip', ['-p', crxZip, path]));
    const tampered = Buffer.from(crx);
    tampered[tampered.length - 1] ^= 1;
    assert.throws(() => verifyCrx(tampered, expectedId), /signature verification failed/);
    assert.throws(() => verifyCrx(crx, 'a'.repeat(32)), /extension ID/);
    await assert.rejects(
      packageExtension({
        source,
        output: join(dir, 'wrong-key'),
        pemBase64,
        expectedId: 'a'.repeat(32)
      }),
      /Signing key does not match/
    );
    const sourceManifest = JSON.stringify({ ...manifest, key: der.toString('base64') });
    await writeFile(join(source, 'manifest.json'), sourceManifest);
    const unsignedOutput = join(dir, 'unsigned');
    const unsigned = await packageExtension({ source, output: unsignedOutput, pemBase64: '' });
    assert.equal(unsigned.signed, false);
    assert.deepEqual(unsigned.files, ['nockster-extension-1.2.3.zip']);
    assert.deepEqual(JSON.parse(execFileSync('unzip', ['-p', join(unsignedOutput, unsigned.files[0]), 'manifest.json'],
      { encoding: 'utf8' })), manifest);
    assert.equal(await readFile(join(source, 'manifest.json'), 'utf8'), sourceManifest);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
