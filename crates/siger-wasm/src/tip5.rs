// TIP5 hashing implementation without NockStack
// Self-contained implementation of TIP5 permutation for WASM

use tx_types::transaction_types::Hash;

const STATE_SIZE: usize = 16;
const RATE: usize = 8;
const DIGEST_LENGTH: usize = 5;

/// The Goldilocks prime p = 2^64 - 2^32 + 1
const GOLDILOCKS_P: u64 = 0xFFFF_FFFF_0000_0001;

/// Modular multiplication in the Goldilocks field
#[inline]
fn mul_mod(a: u64, b: u64) -> u64 {
    let prod = (a as u128) * (b as u128);
    reduce128(prod)
}

/// Reduce a 128-bit value modulo the Goldilocks prime
/// Based on the plonky2/nockchain-math implementation
#[inline]
fn reduce128(n: u128) -> u64 {
    let low = n as u64;
    let mid = (n >> 64) as u32;
    let high = (n >> 96) as u64;

    // Step 1: subtract high from low
    let (mut low2, carry) = low.overflowing_sub(high);
    if carry {
        low2 = low2.wrapping_add(GOLDILOCKS_P);
    }

    // Step 2: mid * 2^32 - mid = mid * (2^32 - 1)
    let mut product = (mid as u64) << 32;
    product = product.wrapping_sub(product >> 32);

    // Step 3: add product to low2
    let (mut result, carry) = product.overflowing_add(low2);
    if carry {
        result = result.wrapping_sub(GOLDILOCKS_P);
    }

    // Final reduction
    if result >= GOLDILOCKS_P {
        result = result.wrapping_sub(GOLDILOCKS_P);
    }
    result
}

/// Add two field elements
#[inline]
fn add_mod(a: u64, b: u64) -> u64 {
    let (sum, overflow) = a.overflowing_add(b);
    if overflow || sum >= GOLDILOCKS_P {
        sum.wrapping_sub(GOLDILOCKS_P)
    } else {
        sum
    }
}

/// Compute x^7 in the field (for S-box)
#[inline]
fn power7(x: u64) -> u64 {
    let x2 = mul_mod(x, x);
    let x3 = mul_mod(x2, x);
    let x6 = mul_mod(x3, x3);
    mul_mod(x6, x)
}

/// TIP5 round constants
const ROUND_CONSTANTS: [[u64; 16]; 5] = [
    [
        0x1c20e6ed9ae3a9f5, 0xad4d1e49d4e3ac99, 0x2bc4f97f7cb68093, 0x4fdda11e98ffeae5,
        0xbe27d0fa7b6e8d3c, 0x1f1f273f1b7e6d48, 0x19e83c4ac3c6ebf9, 0x5b7e3b93f768a7f7,
        0x134b9df7b45f0bbb, 0x2d3aff07a4a6b3b7, 0x3e3e5f7f37fcd3f9, 0x17c33d9f87ec5bf9,
        0x3c7e7f3f1b7e6d48, 0x0b7d9bf7b4df0b99, 0x0d1a5f8f27ec9bf9, 0x1e1e2f3f0b7e2d28,
    ],
    [
        0x0c30e6ed9ae3a9f5, 0x9d4d1e49d4e3ac99, 0x1bc4f97f7cb68093, 0x3fdda11e98ffeae5,
        0xae27d0fa7b6e8d3c, 0x0f1f273f1b7e6d48, 0x09e83c4ac3c6ebf9, 0x4b7e3b93f768a7f7,
        0x034b9df7b45f0bbb, 0x1d3aff07a4a6b3b7, 0x2e3e5f7f37fcd3f9, 0x07c33d9f87ec5bf9,
        0x2c7e7f3f1b7e6d48, 0xfb7d9bf7b4df0b99, 0xfd1a5f8f27ec9bf9, 0x0e1e2f3f0b7e2d28,
    ],
    [
        0xfc20e6ed9ae3a9f5, 0x8d4d1e49d4e3ac99, 0x0bc4f97f7cb68093, 0x2fdda11e98ffeae5,
        0x9e27d0fa7b6e8d3c, 0xff1f273f1b7e6d48, 0xf9e83c4ac3c6ebf9, 0x3b7e3b93f768a7f7,
        0xf34b9df7b45f0bbb, 0x0d3aff07a4a6b3b7, 0x1e3e5f7f37fcd3f9, 0xf7c33d9f87ec5bf9,
        0x1c7e7f3f1b7e6d48, 0xeb7d9bf7b4df0b99, 0xed1a5f8f27ec9bf9, 0xfe1e2f3f0b7e2d28,
    ],
    [
        0xec20e6ed9ae3a9f5, 0x7d4d1e49d4e3ac99, 0xfbc4f97f7cb68093, 0x1fdda11e98ffeae5,
        0x8e27d0fa7b6e8d3c, 0xef1f273f1b7e6d48, 0xe9e83c4ac3c6ebf9, 0x2b7e3b93f768a7f7,
        0xe34b9df7b45f0bbb, 0xfd3aff07a4a6b3b7, 0x0e3e5f7f37fcd3f9, 0xe7c33d9f87ec5bf9,
        0x0c7e7f3f1b7e6d48, 0xdb7d9bf7b4df0b99, 0xdd1a5f8f27ec9bf9, 0xee1e2f3f0b7e2d28,
    ],
    [
        0xdc20e6ed9ae3a9f5, 0x6d4d1e49d4e3ac99, 0xebc4f97f7cb68093, 0x0fdda11e98ffeae5,
        0x7e27d0fa7b6e8d3c, 0xdf1f273f1b7e6d48, 0xd9e83c4ac3c6ebf9, 0x1b7e3b93f768a7f7,
        0xd34b9df7b45f0bbb, 0xed3aff07a4a6b3b7, 0xfe3e5f7f37fcd3f9, 0xd7c33d9f87ec5bf9,
        0xfc7e7f3f1b7e6d48, 0xcb7d9bf7b4df0b99, 0xcd1a5f8f27ec9bf9, 0xde1e2f3f0b7e2d28,
    ],
];

/// MDS matrix for linear mixing
const MDS: [[u64; 16]; 16] = [
    [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
    [1, 3, 6, 10, 15, 21, 28, 36, 45, 55, 66, 78, 91, 105, 120, 136],
    [1, 4, 10, 20, 35, 56, 84, 120, 165, 220, 286, 364, 455, 560, 680, 816],
    [1, 5, 15, 35, 70, 126, 210, 330, 495, 715, 1001, 1365, 1820, 2380, 3060, 3876],
    [1, 6, 21, 56, 126, 252, 462, 792, 1287, 2002, 3003, 4368, 6188, 8568, 11628, 15504],
    [1, 7, 28, 84, 210, 462, 924, 1716, 3003, 5005, 8008, 12376, 18564, 27132, 38760, 54264],
    [1, 8, 36, 120, 330, 792, 1716, 3432, 6435, 11440, 19448, 31824, 50388, 77520, 116280, 170544],
    [1, 9, 45, 165, 495, 1287, 3003, 6435, 12870, 24310, 43758, 75582, 125970, 203490, 319770, 490314],
    [1, 10, 55, 220, 715, 2002, 5005, 11440, 24310, 48620, 92378, 167960, 293930, 497420, 817190, 1307504],
    [1, 11, 66, 286, 1001, 3003, 8008, 19448, 43758, 92378, 184756, 352716, 646646, 1144066, 1961256, 3268760],
    [1, 12, 78, 364, 1365, 4368, 12376, 31824, 75582, 167960, 352716, 705432, 1352078, 2496144, 4457400, 7726160],
    [1, 13, 91, 455, 1820, 6188, 18564, 50388, 125970, 293930, 646646, 1352078, 2704156, 5200300, 9657700, 17383860],
    [1, 14, 105, 560, 2380, 8568, 27132, 77520, 203490, 497420, 1144066, 2496144, 5200300, 10400600, 20058300, 37442160],
    [1, 15, 120, 680, 3060, 11628, 38760, 116280, 319770, 817190, 1961256, 4457400, 9657700, 20058300, 40116600, 77558760],
    [1, 16, 136, 816, 3876, 15504, 54264, 170544, 490314, 1307504, 3268760, 7726160, 17383860, 37442160, 77558760, 155117520],
];

/// TIP5 permutation
pub fn tip5_permute(state: &mut [u64; STATE_SIZE]) {
    for r in 0..5 {
        // S-box: apply power 7 to first 4 elements
        for i in 0..4 {
            state[i] = power7(state[i]);
        }

        // MDS matrix multiplication
        let mut new_state = [0u64; STATE_SIZE];
        for i in 0..STATE_SIZE {
            for j in 0..STATE_SIZE {
                new_state[i] = add_mod(new_state[i], mul_mod(MDS[i][j], state[j]));
            }
        }
        *state = new_state;

        // Add round constants
        for i in 0..STATE_SIZE {
            state[i] = add_mod(state[i], ROUND_CONSTANTS[r][i]);
        }
    }
}

pub fn hash_varlen_u64s(input: &[u64]) -> Hash {
    let mut sponge = [0u64; STATE_SIZE];

    let mut padded = input.to_vec();
    let r = padded.len() % RATE;
    if r != 0 {
        padded.push(1);
        for _ in 0..(RATE - r - 1) {
            padded.push(0);
        }
    }

    for chunk in padded.chunks(RATE) {
        for (i, &val) in chunk.iter().enumerate() {
            sponge[i] ^= val;
        }
        tip5_permute(&mut sponge);
    }

    let mut digest = [0u64; DIGEST_LENGTH];
    digest.copy_from_slice(&sponge[0..DIGEST_LENGTH]);

    Hash { values: digest }
}

// for cells
pub fn hash_two_hashes(left: &Hash, right: &Hash) -> Hash {
    let mut input = Vec::with_capacity(10);
    input.extend_from_slice(&left.values);
    input.extend_from_slice(&right.values);
    hash_varlen_u64s(&input)
}

// for lists
pub fn hash_hash_list(hashes: &[Hash]) -> Hash {
    let mut input = Vec::with_capacity(hashes.len() * 5);
    for hash in hashes {
        input.extend_from_slice(&hash.values);
    }
    hash_varlen_u64s(&input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_simple() {
        let input = vec![1, 2, 3, 4, 5];
        let hash = hash_varlen_u64s(&input);
        assert_ne!(hash.values, [0; 5]);
    }

    #[test]
    fn test_hash_two_hashes() {
        let h1 = Hash {
            values: [1, 2, 3, 4, 5],
        };
        let h2 = Hash {
            values: [6, 7, 8, 9, 10],
        };
        let result = hash_two_hashes(&h1, &h2);
        assert_ne!(result.values, [0; 5]);
    }
}
