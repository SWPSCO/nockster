const PBKDF2_ITERATIONS = 2048;

const VALID_WORD_COUNTS = new Set([12, 15, 18, 21, 24]);

export function isValidMnemonicWordCount(count: number): boolean {
  return VALID_WORD_COUNTS.has(count);
}

function ensureWebCrypto() {
  if (typeof globalThis === 'undefined' || !globalThis.crypto || !globalThis.crypto.subtle) {
    throw new Error('Web Crypto API unavailable');
  }
  return globalThis.crypto.subtle;
}

export function validateMnemonicWords(input: string): void {
  const words = input.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) {
    throw new Error('Provide your seed words');
  }
  if (!isValidMnemonicWordCount(words.length)) {
    throw new Error(`Seed words should be 12, 15, 18, 21, or 24 words (received ${words.length})`);
  }
}

export async function mnemonicToSeed(mnemonic: string, passphrase: string): Promise<Uint8Array> {
  const subtle = ensureWebCrypto();
  const encoder = new TextEncoder();

  const normalizedMnemonic = mnemonic.normalize('NFKD');
  const normalizedPassphrase = passphrase.normalize('NFKD');

  const keyMaterial = await subtle.importKey(
    'raw',
    encoder.encode(normalizedMnemonic),
    'PBKDF2',
    false,
    ['deriveBits'],
  );

  const salt = encoder.encode(`mnemonic${normalizedPassphrase}`);
  const derivedBits = await subtle.deriveBits(
    {
      name: 'PBKDF2',
      salt,
      iterations: PBKDF2_ITERATIONS,
      hash: 'SHA-512',
    },
    keyMaterial,
    512,
  );

  return new Uint8Array(derivedBits);
}
