import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  encryptBackup,
  decryptBackup,
  restoreBackup,
  maxBackupBytes,
  vaultKey
} from '../apps/urbit/src/backup.ts';
import { scopedStorage } from '../packages/wallet/src/platform/browserStorage.ts';

const b64 = (length: number) => Buffer.alloc(length, 7).toString('base64');
const vault = {
  kdf: { kdfType: 'Argon2id', salt: b64(32), m: 131072, t: 3, p: 1 },
  wrappedVmk: { nonce: b64(12), ct: b64(32), tag: b64(16) },
  data: b64(96),
  log: [{ operation: 'test wallet', timestamp: 1 }]
};
const password = 'synthetic backup passphrase';

function memoryStorage(): Storage {
  const data = new Map<string, string>();
  return {
    get length() {
      return data.size;
    },
    key: index => [...data.keys()][index] ?? null,
    getItem: key => data.get(key) ?? null,
    setItem: (key, value) => {
      data.set(key, value);
    },
    removeItem: key => {
      data.delete(key);
    },
    clear: () => data.clear()
  };
}

test('backup authenticates all wallet data, uses fresh entropy, and rejects tampering', async () => {
  const first = await encryptBackup(vault, password);
  const second = await encryptBackup(vault, password);
  assert.notEqual(first, second);
  assert.equal(first.includes(password), false);
  assert.equal(first.includes('test wallet'), false);
  assert.deepEqual(await decryptBackup(first, password), vault);
  await assert.rejects(decryptBackup(first, 'incorrect backup password'), /passphrase or damaged/);
  for (const field of ['salt', 'nonce', 'ciphertext']) {
    const altered = JSON.parse(first);
    const bytes = Buffer.from(altered[field], 'base64');
    bytes[0] ^= 1;
    altered[field] = bytes.toString('base64');
    await assert.rejects(decryptBackup(JSON.stringify(altered), password), /passphrase or damaged/);
  }
  const altered = JSON.parse(first);
  altered.format = 'unknown';
  await assert.rejects(decryptBackup(JSON.stringify(altered), password), /Unsupported/);
  await assert.rejects(decryptBackup('x'.repeat(maxBackupBytes + 1), password), /too large/);
  await assert.rejects(encryptBackup(vault, 'short'), /16 characters/);
  await assert.rejects(
    encryptBackup({ ...vault, kdf: { ...vault.kdf, m: 2 ** 32 } }, password),
    /parameters/
  );
});

test('restoration preserves an existing vault and clearing scoped storage preserves other apps', () => {
  const origin = memoryStorage();
  origin.setItem('another-app', 'preserve');
  const storage = scopedStorage(origin, 'nockster:');
  restoreBackup(storage, vault);
  assert.equal(origin.getItem(vaultKey), null);
  assert.equal(storage.getItem(vaultKey), JSON.stringify(vault));
  assert.throws(() => restoreBackup(storage, { ...vault, data: b64(97) }), /already has a vault/);
  assert.equal(storage.getItem(vaultKey), JSON.stringify(vault));
  assert.equal(storage.length, 1);
  assert.equal(storage.key(0), vaultKey);
  storage.clear();
  assert.equal(origin.getItem('another-app'), 'preserve');
  assert.equal(storage.length, 0);
});
