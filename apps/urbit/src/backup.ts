import { isStoredVault, type StoredVault } from '../../../packages/wallet/src/vault/types.ts';

// The format fixes the work factor; untrusted input cannot select a costly KDF.
const format = 'nockster-backup-1';
const context = new TextEncoder().encode(format);
export const maxBackupBytes = 2 * 1024 * 1024;
export const vaultKey = 'fletch_vault_v1';

function base64(bytes: Uint8Array): string {
  let text = '';
  for (let start = 0; start < bytes.length; start += 8192)
    text += String.fromCharCode(...bytes.subarray(start, start + 8192));
  return btoa(text);
}

function decode(value: unknown, length?: number): Uint8Array<ArrayBuffer> {
  if (typeof value !== 'string' || !/^[A-Za-z0-9+/]*={0,2}$/.test(value))
    throw new Error('Invalid backup encoding.');
  const bytes = Uint8Array.from(atob(value), char => char.charCodeAt(0));
  if (base64(bytes) !== value || (length !== undefined && bytes.length !== length))
    throw new Error('Invalid backup encoding.');
  return bytes;
}

async function deriveKey(password: string, salt: Uint8Array<ArrayBuffer>) {
  if (password.length < 16) throw new Error('Use a backup passphrase of at least 16 characters.');
  const bytes = new TextEncoder().encode(password);
  try {
    const material = await crypto.subtle.importKey('raw', bytes, 'PBKDF2', false, ['deriveKey']);
    return await crypto.subtle.deriveKey(
      { name: 'PBKDF2', hash: 'SHA-256', iterations: 600_000, salt },
      material,
      { name: 'AES-GCM', length: 256 },
      false,
      ['encrypt', 'decrypt']
    );
  } finally {
    bytes.fill(0);
  }
}

function validateVault(value: unknown): asserts value is StoredVault {
  if (!isStoredVault(value)) throw new Error('Invalid wallet backup.');
  const { kdf } = value;
  if (
    kdf.kdfType !== 'Argon2id' ||
    !Number.isSafeInteger(kdf.m) ||
    kdf.m < 8 ||
    kdf.m > 262144 ||
    !Number.isSafeInteger(kdf.t) ||
    kdf.t < 1 ||
    kdf.t > 10 ||
    !Number.isSafeInteger(kdf.p) ||
    kdf.p < 1 ||
    kdf.p > 16
  )
    throw new Error('Unsupported wallet encryption parameters.');
  decode(kdf.salt, 32);
  decode(value.wrappedVmk.nonce, 12);
  decode(value.wrappedVmk.ct, 32);
  decode(value.wrappedVmk.tag, 16);
  decode(value.data);
}

export async function encryptBackup(vault: unknown, password: string): Promise<string> {
  validateVault(vault);
  const plaintext = new TextEncoder().encode(
    JSON.stringify({ format, createdAt: new Date().toISOString(), vault })
  );
  if (plaintext.length > maxBackupBytes / 2) throw new Error('Wallet backup is too large.');
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const nonce = crypto.getRandomValues(new Uint8Array(12));
  try {
    const key = await deriveKey(password, salt);
    const ciphertext = await crypto.subtle.encrypt(
      { name: 'AES-GCM', iv: nonce, additionalData: context },
      key,
      plaintext
    );
    return JSON.stringify({
      format,
      salt: base64(salt),
      nonce: base64(nonce),
      ciphertext: base64(new Uint8Array(ciphertext))
    });
  } finally {
    plaintext.fill(0);
  }
}

export async function decryptBackup(text: string, password: string): Promise<StoredVault> {
  if (text.length > maxBackupBytes) throw new Error('Wallet backup is too large.');
  const envelope = JSON.parse(text);
  if (envelope?.format !== format) throw new Error('Unsupported backup format.');
  const salt = decode(envelope.salt, 16);
  const nonce = decode(envelope.nonce, 12);
  const ciphertext = decode(envelope.ciphertext);
  const key = await deriveKey(password, salt);
  let plaintext: ArrayBuffer;
  try {
    plaintext = await crypto.subtle.decrypt(
      { name: 'AES-GCM', iv: nonce, additionalData: context },
      key,
      ciphertext
    );
  } catch {
    throw new Error('Incorrect backup passphrase or damaged backup.');
  }
  try {
    const data = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(plaintext));
    if (data?.format !== format) throw new Error('Invalid wallet backup.');
    validateVault(data.vault);
    return data.vault;
  } finally {
    new Uint8Array(plaintext).fill(0);
  }
}

export function restoreBackup(storage: Storage, vault: StoredVault): void {
  validateVault(vault);
  if (storage.getItem(vaultKey) !== null)
    throw new Error('This browser already has a vault. Restore in a separate browser profile.');
  storage.setItem(vaultKey, JSON.stringify(vault));
}

const endpoint = '/nockster/backup';
export async function readShipBackup(): Promise<{ text: string | null; revision: string }> {
  const response = await fetch(endpoint, {
    credentials: 'same-origin',
    cache: 'no-store',
    redirect: 'error'
  });
  if (!response.ok)
    throw new Error(`Ship backup unavailable (${response.status}). Sign in to your ship.`);
  const revision = response.headers.get('etag');
  if (!revision || !/^"[0-9.]+"$/.test(revision)) throw new Error('Invalid ship backup revision.');
  const reader = response.body?.getReader();
  if (!reader) throw new Error('Missing ship backup response.');
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      size += value.length;
      if (size > maxBackupBytes) throw new Error('Wallet backup is too large.');
      chunks.push(value);
    }
  } finally {
    await reader.cancel();
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  return { text: size ? new TextDecoder('utf-8', { fatal: true }).decode(bytes) : null, revision };
}

export async function writeShipBackup(text: string, revision: string): Promise<void> {
  if (new TextEncoder().encode(text).length > maxBackupBytes)
    throw new Error('Wallet backup is too large.');
  const response = await fetch(endpoint, {
    method: 'PUT',
    credentials: 'same-origin',
    redirect: 'error',
    headers: {
      'Content-Type': 'application/octet-stream',
      'If-Match': revision,
      'X-Nockster-Backup': '1'
    },
    body: text
  });
  if (response.status === 409)
    throw new Error('The ship backup changed. Review it before trying again.');
  if (!response.ok) throw new Error(`Could not save ship backup (${response.status}).`);
}
