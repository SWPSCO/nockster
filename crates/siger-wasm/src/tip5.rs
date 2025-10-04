// TIP5 hashing implementation without NockStack
use tx_types::crypto::goldilocks::tip5_permute;
use tx_types::Hash;

const STATE_SIZE: usize = 16;
const RATE: usize = 8;
const DIGEST_LENGTH: usize = 5;

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
