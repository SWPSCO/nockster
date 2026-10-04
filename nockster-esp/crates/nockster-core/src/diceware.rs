//! Deterministic entropy extraction for physical six-sided dice.

use sha2::{Digest, Sha256};

/// 100 fair d6 rolls provide about 258.5 bits of source entropy.
pub const DICE_ROLLS_FOR_256_BITS: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiceEntropyError {
    WrongRollCount,
    InvalidFace,
}

/// Convert exactly 100 physical d6 results into domain-separated 256-bit
/// entropy suitable for a 24-word BIP-39 mnemonic.
///
/// This deliberately does not mix in device randomness: dice mode remains an
/// independently reproducible, user-supplied entropy path.
pub fn dice_entropy_256(rolls: &[u8]) -> Result<[u8; 32], DiceEntropyError> {
    if rolls.len() != DICE_ROLLS_FOR_256_BITS {
        return Err(DiceEntropyError::WrongRollCount);
    }
    if rolls.iter().any(|roll| !(1..=6).contains(roll)) {
        return Err(DiceEntropyError::InvalidFace);
    }

    let mut hasher = Sha256::new();
    hasher.update(b"nockster/diceware/v1\0");
    hasher.update([DICE_ROLLS_FOR_256_BITS as u8]);
    hasher.update(rolls);
    Ok(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dice_entropy_has_stable_domain_separated_vector() {
        let mut rolls = [0u8; DICE_ROLLS_FOR_256_BITS];
        for (index, roll) in rolls.iter_mut().enumerate() {
            *roll = (index % 6 + 1) as u8;
        }
        let entropy = dice_entropy_256(&rolls).unwrap();
        assert_eq!(
            entropy,
            [
                0xce, 0xfd, 0xa6, 0xbb, 0x10, 0xc4, 0x84, 0xf2, 0x9d, 0x47, 0x4a, 0xa1, 0x08, 0x87,
                0x91, 0x68, 0x1a, 0x05, 0xa1, 0xca, 0x93, 0xbd, 0x10, 0x14, 0xc7, 0x08, 0xbd, 0x97,
                0x9b, 0x7e, 0x2c, 0x11,
            ]
        );
        assert_eq!(
            bip39::Mnemonic::from_entropy(&entropy).unwrap().to_string(),
            "solve unfold put canoe embark junk insect true patient during tone south park special clean jeans avoid plate season kite keen sample raccoon craft"
        );
    }

    #[test]
    fn dice_entropy_rejects_bad_count_and_faces() {
        assert_eq!(
            dice_entropy_256(&[1; DICE_ROLLS_FOR_256_BITS - 1]),
            Err(DiceEntropyError::WrongRollCount)
        );
        let mut rolls = [1u8; DICE_ROLLS_FOR_256_BITS];
        rolls[42] = 7;
        assert_eq!(dice_entropy_256(&rolls), Err(DiceEntropyError::InvalidFace));
    }
}
