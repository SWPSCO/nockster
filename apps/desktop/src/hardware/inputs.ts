import { validateMnemonic, mnemonicToSeed } from '@scure/bip39';
import { wordlist } from '@scure/bip39/wordlists/english.js';

export async function seedFromPhrase(input: string, passphrase: string): Promise<Uint8Array> {
  const phrase = input.trim().normalize('NFKD').split(/\s+/).join(' ');
  if (phrase.split(' ').length !== 24 || !validateMnemonic(phrase, wordlist)) {
    throw new Error('Enter a valid 24-word recovery phrase');
  }
  return mnemonicToSeed(phrase, passphrase);
}

export function asciiLabel(input: string, maximum = 32): string {
  const label = input.trim();
  if (!label || label.length > maximum || !/^[\x20-\x7e]+$/.test(label)) {
    throw new Error(`Use a label of 1–${maximum} printable ASCII characters`);
  }
  return label;
}

export function parseHex(input: string): Uint8Array {
  const hex = input.trim().replace(/^0x/, '').replace(/\s+/g, '');
  if (!hex || hex.length % 2 !== 0 || !/^[0-9a-fA-F]+$/.test(hex)) {
    throw new Error('Enter complete hexadecimal bytes, such as a1b2c3');
  }
  return Uint8Array.from(hex.match(/../g)!, byte => parseInt(byte, 16));
}

export async function readFile(file: File, maximum = 1024 * 1024) {
  if (file.size > maximum)
    throw new Error(`Choose a file smaller than ${maximum.toLocaleString()} bytes`);
  return new Uint8Array(await file.arrayBuffer());
}
