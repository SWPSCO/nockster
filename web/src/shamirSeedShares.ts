import { entropyToMnemonic, mnemonicToEntropy, validateMnemonic } from '@scure/bip39';
import { wordlist } from '@scure/bip39/wordlists/english.js';

export type SeedPhraseShare = {
  index: number;
  threshold: number;
  total: number;
  digestHex: string;
  phrase: string;
  copyText: string;
};

export type RecoveredSeedPhrase = {
  mnemonic: string;
  threshold: number;
  total: number;
  used: number;
};

const SHARE_MARKER = 'nockster-seed-share-v1';
const SECRET_LEN = 32;
const MIN_THRESHOLD = 2;
const MAX_SHARES = 16;
const wordSet = new Set(wordlist);

function checkParams(k: number, n: number) {
  if (!Number.isInteger(k) || !Number.isInteger(n) || k < MIN_THRESHOLD || n < k || n > MAX_SHARES) {
    throw new Error('Need 2 <= threshold <= shares <= 16');
  }
}

function normalizeMnemonic(input: string): string {
  return input.trim().toLowerCase().split(/\s+/).filter(Boolean).join(' ');
}

function gfMul(a: number, b: number): number {
  let p = 0;
  let aa = a & 0xff;
  let bb = b & 0xff;
  for (let i = 0; i < 8; i += 1) {
    if ((bb & 1) !== 0) p ^= aa;
    const hi = aa & 0x80;
    aa = (aa << 1) & 0xff;
    if (hi !== 0) aa ^= 0x1b;
    bb >>= 1;
  }
  return p & 0xff;
}

function gfInv(a: number): number {
  if (a === 0) throw new Error('bad share coordinates');
  let result = 1;
  let base = a & 0xff;
  for (let bit = 0; bit < 8; bit += 1) {
    if (((254 >> bit) & 1) !== 0) {
      result = gfMul(result, base);
    }
    base = gfMul(base, base);
  }
  return result;
}

function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

async function digest4Hex(bytes: Uint8Array): Promise<string> {
  if (!globalThis.crypto?.subtle) {
    throw new Error('Web Crypto API unavailable');
  }
  const material = new Uint8Array(bytes);
  try {
    const digest = new Uint8Array(
      await globalThis.crypto.subtle.digest('SHA-256', material.buffer as ArrayBuffer),
    );
    return bytesToHex(digest.slice(0, 4));
  } finally {
    material.fill(0);
  }
}

function parseMetadataToken(token: string) {
  const clean = token.trim().replace(/[,:;]+$/g, '').toLowerCase();
  let match = clean.match(/^k=(\d+)$/);
  if (match) return { key: 'threshold' as const, value: Number(match[1]) };
  match = clean.match(/^n=(\d+)$/);
  if (match) return { key: 'total' as const, value: Number(match[1]) };
  match = clean.match(/^x=(\d+)$/);
  if (match) return { key: 'index' as const, value: Number(match[1]) };
  match = clean.match(/^#(\d+)$/);
  if (match) return { key: 'index' as const, value: Number(match[1]) };
  match = clean.match(/^d=([0-9a-f]{8})$/);
  if (match) return { key: 'digestHex' as const, value: match[1] };
  match = clean.match(/^(\d+)-of-(\d+)$/);
  if (match) {
    return {
      key: 'thresholdTotal' as const,
      value: [Number(match[1]), Number(match[2])] as const,
    };
  }
  return null;
}

function cleanWordToken(token: string): string {
  return token.toLowerCase().replace(/^[^a-z]+|[^a-z]+$/g, '');
}

type ParsedSeedShare = {
  index: number;
  threshold: number;
  total: number;
  digestHex: string;
  y: Uint8Array;
};

function parseMarkedSeedShares(input: string): ParsedSeedShare[] {
  const tokens = input.trim().split(/\s+/).filter(Boolean);
  const shares: ParsedSeedShare[] = [];

  for (let i = 0; i < tokens.length; i += 1) {
    if (tokens[i] !== SHARE_MARKER) continue;

    let threshold: number | null = null;
    let total: number | null = null;
    let index: number | null = null;
    let digestHex: string | null = null;
    i += 1;

    while (i < tokens.length) {
      const meta = parseMetadataToken(tokens[i]);
      if (!meta) break;
      if (meta.key === 'threshold') threshold = meta.value;
      if (meta.key === 'total') total = meta.value;
      if (meta.key === 'index') index = meta.value;
      if (meta.key === 'digestHex') digestHex = meta.value;
      if (meta.key === 'thresholdTotal') {
        threshold = meta.value[0];
        total = meta.value[1];
      }
      i += 1;
    }

    const words: string[] = [];
    while (i < tokens.length && words.length < 24) {
      if (tokens[i] === SHARE_MARKER) break;
      const word = cleanWordToken(tokens[i]);
      if (!wordSet.has(word)) {
        throw new Error(`Seed share #${index ?? '?'} contains a non-BIP39 word`);
      }
      words.push(word);
      i += 1;
    }
    i -= 1;

    if (threshold === null || total === null || index === null || digestHex === null) {
      throw new Error('Seed shares must include k, n, share number, and digest');
    }
    if (words.length !== 24) {
      throw new Error(`Seed share #${index} must contain exactly 24 words`);
    }
    checkParams(threshold, total);
    if (index < 1 || index > total) {
      throw new Error(`Seed share #${index} is outside 1..${total}`);
    }
    const phrase = words.join(' ');
    if (!validateMnemonic(phrase, wordlist)) {
      throw new Error(`Seed share #${index} has a bad BIP39 checksum`);
    }
    const y = mnemonicToEntropy(phrase, wordlist);
    if (y.length !== SECRET_LEN) {
      throw new Error(`Seed share #${index} must be a 24-word BIP39 phrase`);
    }
    shares.push({ index, threshold, total, digestHex, y });
  }

  return shares;
}

function zeroizeParsedShares(shares: ParsedSeedShare[]) {
  shares.forEach((share) => share.y.fill(0));
}

export async function splitMnemonicToSeedShares(
  mnemonicInput: string,
  threshold: number,
  total: number,
): Promise<SeedPhraseShare[]> {
  checkParams(threshold, total);
  const mnemonic = normalizeMnemonic(mnemonicInput);
  const words = mnemonic.split(/\s+/).filter(Boolean);
  if (words.length !== 24) {
    throw new Error(`Seed backup needs the original 24-word phrase (received ${words.length} words)`);
  }
  if (!validateMnemonic(mnemonic, wordlist)) {
    throw new Error('Invalid seed phrase: check spelling and word order');
  }

  const entropy = mnemonicToEntropy(mnemonic, wordlist);
  if (entropy.length !== SECRET_LEN) {
    entropy.fill(0);
    throw new Error('Seed backup only supports 24-word BIP39 phrases');
  }

  const digestHex = await digest4Hex(entropy);
  const random = new Uint8Array(SECRET_LEN * (threshold - 1));
  globalThis.crypto.getRandomValues(random);

  const shares: SeedPhraseShare[] = [];
  try {
    for (let x = 1; x <= total; x += 1) {
      const y = new Uint8Array(SECRET_LEN);
      for (let byteIdx = 0; byteIdx < SECRET_LEN; byteIdx += 1) {
        let acc = 0;
        for (let j = threshold - 1; j >= 0; j -= 1) {
          const coeff = j === 0 ? entropy[byteIdx] : random[byteIdx * (threshold - 1) + (j - 1)];
          acc = gfMul(acc, x) ^ coeff;
        }
        y[byteIdx] = acc;
      }
      const phrase = entropyToMnemonic(y, wordlist);
      y.fill(0);
      shares.push({
        index: x,
        threshold,
        total,
        digestHex,
        phrase,
        copyText: `${SHARE_MARKER} k=${threshold} n=${total} x=${x} d=${digestHex}\n${phrase}`,
      });
    }
    return shares;
  } finally {
    entropy.fill(0);
    random.fill(0);
  }
}

export async function combineSeedPhraseShares(input: string): Promise<RecoveredSeedPhrase> {
  const shares = parseMarkedSeedShares(input);
  if (shares.length === 0) {
    throw new Error(`No ${SHARE_MARKER} seed shares found`);
  }

  const first = shares[0];
  for (const share of shares) {
    if (
      share.threshold !== first.threshold ||
      share.total !== first.total ||
      share.digestHex !== first.digestHex
    ) {
      zeroizeParsedShares(shares);
      throw new Error('Seed shares are from different backups');
    }
  }

  const seen = new Set<number>();
  for (const share of shares) {
    if (seen.has(share.index)) {
      zeroizeParsedShares(shares);
      throw new Error(`Duplicate seed share #${share.index}`);
    }
    seen.add(share.index);
  }

  if (shares.length < first.threshold) {
    zeroizeParsedShares(shares);
    throw new Error(`Need ${first.threshold} seed shares; got ${shares.length}`);
  }

  const xs = shares.map((share) => share.index);
  const secret = new Uint8Array(SECRET_LEN);
  try {
    for (let byteIdx = 0; byteIdx < SECRET_LEN; byteIdx += 1) {
      let acc = 0;
      for (let i = 0; i < shares.length; i += 1) {
        let num = 1;
        let den = 1;
        for (let m = 0; m < xs.length; m += 1) {
          if (m === i) continue;
          num = gfMul(num, xs[m]);
          den = gfMul(den, xs[m] ^ xs[i]);
        }
        const basis = gfMul(num, gfInv(den));
        acc ^= gfMul(shares[i].y[byteIdx], basis);
      }
      secret[byteIdx] = acc;
    }

    const digestHex = await digest4Hex(secret);
    if (digestHex !== first.digestHex) {
      throw new Error('Seed shares did not reconstruct the expected seed');
    }

    return {
      mnemonic: entropyToMnemonic(secret, wordlist),
      threshold: first.threshold,
      total: first.total,
      used: shares.length,
    };
  } finally {
    secret.fill(0);
    zeroizeParsedShares(shares);
  }
}
