// Cheetah pubkey encoding to match Rust ser_a_pt function
// Based on tx-types/tx-types/src/crypto/cheetah.rs

import bs58 from 'bs58';

/**
 * Serialize affine point limbs (x,y) into 97 bytes in Hoon parity:
 * 0x01 sentinel followed by Y then X limbs, most-significant limb first.
 *
 * Input arrays are LSW-first (index 0 = least significant word)
 * Output is MSW-first for serialization
 */
export function serializeCheetahPublicKey(x: bigint[], y: bigint[]): Uint8Array {
  const out = new Uint8Array(97);
  out[0] = 0x01; // Sentinel byte

  let offset = 1;

  // Write Y coordinate (MSW first, so reverse the array)
  for (let i = y.length - 1; i >= 0; i--) {
    const limb = y[i];
    // Write u64 as big-endian bytes
    const bytes = new Uint8Array(8);
    for (let j = 0; j < 8; j++) {
      bytes[7 - j] = Number((limb >> BigInt(8 * j)) & 0xFFn);
    }
    out.set(bytes, offset);
    offset += 8;
  }

  // Write X coordinate (MSW first, so reverse the array)
  for (let i = x.length - 1; i >= 0; i--) {
    const limb = x[i];
    // Write u64 as big-endian bytes
    const bytes = new Uint8Array(8);
    for (let j = 0; j < 8; j++) {
      bytes[7 - j] = Number((limb >> BigInt(8 * j)) & 0xFFn);
    }
    out.set(bytes, offset);
    offset += 8;
  }

  return out;
}

/**
 * Base58 encode bytes (Bitcoin-style alphabet)
 * Using bs58 library to match Rust implementation
 */
export function base58Encode(bytes: Uint8Array): string {
  return bs58.encode(bytes);
}

/**
 * Format Cheetah pubkey from (x, y) coordinates to base58
 * Matches the Rust pubkey_to_b58 function
 */
export function formatCheetahPubkey(x: bigint[], y: bigint[]): string {
  const serialized = serializeCheetahPublicKey(x, y);
  return base58Encode(serialized);
}
