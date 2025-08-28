use nockapp::Noun;
use nockchain_libp2p_io::tip5_util;

// Re-export for use by other modules in this crate
pub use nockchain_libp2p_io::tip5_util::tip5_hash_to_base58;
use nockvm::noun::{Atom, D, T};
use nockvm::jets::JetErr;
use nockvm::jets::util::test::init_context;
use zkvm_jetpack::jets::tip5_jets::{hash_noun_varlen_jet, hash_10_jet};

use crate::transaction_types::Hash;

/// Errors that can occur during TIP5 hashing
#[derive(Debug, Clone)]
pub enum Tip5Error {
    /// Failed to serialize the noun for hashing
    SerializationError(String),
    /// TIP5 hashing operation failed
    HashingError(String),
    /// Invalid noun structure
    InvalidNoun(String),
}

impl std::fmt::Display for Tip5Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tip5Error::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            Tip5Error::HashingError(msg) => write!(f, "Hashing error: {}", msg),
            Tip5Error::InvalidNoun(msg) => write!(f, "Invalid noun: {}", msg),
        }
    }
}

impl std::error::Error for Tip5Error {}

impl From<JetErr> for Tip5Error {
    fn from(err: JetErr) -> Self {
        Tip5Error::HashingError(format!("Jet error: {:?}", err))
    }
}

/// TIP5 hasher for Nock nouns
pub struct Tip5Hasher;


impl Tip5Hasher {
    /// Hash a noun using the TIP5 algorithm and return as a Hash struct
    /// 
    /// This is the main interface for hashing nouns. It takes any noun
    /// and returns its TIP5 hash as a Hash struct containing the 5-element belt.
    /// 
    /// # Arguments
    /// * `noun` - The noun to hash
    /// 
    /// # Returns
    /// * `Result<Hash, Tip5Error>` - The hash as a Hash struct or an error
    pub fn hash_noun(noun: Noun) -> Result<Hash, Tip5Error> {
        // Create a test context (includes NockStack and other needed components)
        let context = &mut init_context();
        
        // Create the proper subject structure for jets: [formula sample payload]
        // The jet expects the sample at slot 6, so we need [0 sample 0]
        let subject = T(&mut context.stack, &[D(0), noun, D(0)]);
        
        // Call the TIP5 hash jet - returns a 5-element tuple (belt)
        let hash_noun = hash_noun_varlen_jet(context, subject)
            .map_err(|e| Tip5Error::HashingError(format!("TIP5 hashing failed: {:?}", e)))?;
        
        // Extract the 5 u64 values from the belt
        // The belt is a nested cell structure: [a [b [c [d e]]]]
        let mut values = [0u64; 5];
        let mut current = hash_noun;
        
        for i in 0..4 {
            let cell = current.as_cell()
                .map_err(|_| Tip5Error::HashingError(format!("Expected cell at position {}", i)))?;
            
            // Get the atom
            let atom = cell.head().as_atom()
                .map_err(|_| Tip5Error::HashingError(format!("Expected atom at position {}", i)))?;
            
            // Handle the conversion properly
            values[i] = if atom.is_direct() {
                atom.as_u64()
                    .map_err(|_| Tip5Error::HashingError(format!("Atom too large at position {}", i)))?
            } else {
                // For indirect atoms, we need to handle the bytes properly
                let bytes = atom.as_ne_bytes();
                if bytes.len() > 8 {
                    return Err(Tip5Error::HashingError(format!("Atom too large at position {}", i)));
                }
                // Reconstruct u64 from little-endian bytes
                let mut result = 0u64;
                for (j, &byte) in bytes.iter().enumerate() {
                    result |= (byte as u64) << (j * 8);
                }
                result
            };
            
            current = cell.tail();
        }
        
        // The last element
        let atom = current.as_atom()
            .map_err(|_| Tip5Error::HashingError("Expected atom at position 4".to_string()))?;
        
        values[4] = if atom.is_direct() {
            atom.as_u64()
                .map_err(|_| Tip5Error::HashingError("Atom too large at position 4".to_string()))?
        } else {
            // For indirect atoms, we need to handle the bytes properly
            let bytes = atom.as_ne_bytes();
            if bytes.len() > 8 {
                return Err(Tip5Error::HashingError("Atom too large at position 4".to_string()));
            }
            // Reconstruct u64 from little-endian bytes
            let mut result = 0u64;
            for (j, &byte) in bytes.iter().enumerate() {
                result |= (byte as u64) << (j * 8);
            }
            result
        };
        
        Ok(Hash { values })
    }
    
    /// Hash a 10-tuple using the TIP5 hash_10 algorithm
    /// 
    /// This is specifically for hashing 10-element tuples, as used in hash_ten_cell.
    /// It wraps the hash_10_jet function which implements the Hoon hash-10 function.
    /// 
    /// # Arguments
    /// * `ten_list` - A noun that should be a 10-element list
    /// 
    /// # Returns
    /// * `Result<Hash, Tip5Error>` - The hash as a Hash struct or an error
    pub fn hash_10(ten_list: Noun) -> Result<Hash, Tip5Error> {
        // Create a test context (includes NockStack and other needed components)
        let context = &mut init_context();
        
        // Create the proper subject structure for jets: [formula sample payload]
        // The jet expects the sample at slot 6, so we need [0 sample 0]
        let subject = T(&mut context.stack, &[D(0), ten_list, D(0)]);
        
        // Call the TIP5 hash_10 jet - returns a list (not a tuple)
        let hash_noun = hash_10_jet(context, subject)
            .map_err(|e| Tip5Error::HashingError(format!("TIP5 hash_10 failed: {:?}", e)))?;
        
        // Extract the 5 u64 values from the list
        // The result is a list structure: [a [b [c [d [e 0]]]]]
        let mut values = [0u64; 5];
        let mut current = hash_noun;
        
        for i in 0..5 {
            let cell = current.as_cell()
                .map_err(|_| Tip5Error::HashingError(format!("Expected cell at position {}", i)))?;
            
            // Get the atom
            let atom = cell.head().as_atom()
                .map_err(|_| Tip5Error::HashingError(format!("Expected atom at position {}", i)))?;
            
            // Handle the conversion properly
            values[i] = if atom.is_direct() {
                atom.as_u64()
                    .map_err(|_| Tip5Error::HashingError(format!("Atom too large at position {}", i)))?
            } else {
                // For indirect atoms, we need to handle the bytes properly
                let bytes = atom.as_ne_bytes();
                if bytes.len() > 8 {
                    return Err(Tip5Error::HashingError(format!("Atom too large at position {}", i)));
                }
                // Reconstruct u64 from little-endian bytes
                let mut result = 0u64;
                for (j, &byte) in bytes.iter().enumerate() {
                    result |= (byte as u64) << (j * 8);
                }
                result
            };
            
            current = cell.tail();
        }
        
        // current should now be 0 (end of list)
        if current.as_atom().is_ok() && current.as_atom().unwrap().as_u64().unwrap_or(1) != 0 {
            return Err(Tip5Error::HashingError("Expected list to end with 0".to_string()));
        }
        
        Ok(Hash { values })
    }
    
    /// Hash a noun and convert to base58 string
    /// 
    /// Convenience function that hashes and converts to base58.
    /// 
    /// # Arguments
    /// * `noun` - The noun to hash
    /// 
    /// # Returns
    /// * `Result<String, Tip5Error>` - The base58-encoded hash or an error
    pub fn hash_noun_to_base58(noun: Noun) -> Result<String, Tip5Error> {
        // Create a test context (includes NockStack and other needed components)
        let context = &mut init_context();
        
        // Create the proper subject structure for jets: [formula sample payload]
        // The jet expects the sample at slot 6, so we need [0 sample 0]
        let subject = T(&mut context.stack, &[D(0), noun, D(0)]);
        
        // Call the TIP5 hash jet
        let hash_noun = hash_noun_varlen_jet(context, subject)
            .map_err(|e| Tip5Error::HashingError(format!("TIP5 hashing failed: {:?}", e)))?;
        
        // Convert the 5-tuple hash to base58 string
        tip5_util::tip5_hash_to_base58(hash_noun)
            .map_err(|e| Tip5Error::HashingError(format!("Base58 conversion failed: {:?}", e)))
    }
}

// Tests for the TIP5Hasher struct

#[cfg(test)]
mod tests {
    use super::*;
    use nockvm::noun::D;

    #[test]
    fn test_hash_simple_atom() {
        let noun = D(42);
        let hash = Tip5Hasher::hash_noun(noun).expect("Should hash successfully");
        
        // Hash should have 5 u64 values
        assert_eq!(hash.values.len(), 5);
        
        // All values should be non-zero (very likely for a hash)
        assert!(hash.values.iter().any(|&v| v != 0), "Hash should have non-zero values");
        
        // Test the base58 conversion function
        let hash_str = Tip5Hasher::hash_noun_to_base58(noun).expect("Should convert to base58");
        assert!(!hash_str.is_empty());
        assert!(hash_str.chars().all(|c| "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(c)));
    }

    #[test]
    fn test_hash_consistency() {
        let noun = D(123);
        let hash1 = Tip5Hasher::hash_noun(noun).expect("Should hash successfully");
        let hash2 = Tip5Hasher::hash_noun(noun).expect("Should hash successfully");
        
        // Same input should produce same hash
        assert_eq!(hash1.values, hash2.values);
    }

    #[test] 
    fn test_hash_different_values() {
        let hash1 = Tip5Hasher::hash_noun(D(42)).expect("Should hash successfully");
        let hash2 = Tip5Hasher::hash_noun(D(43)).expect("Should hash successfully");
        
        // Different inputs should produce different hashes
        assert_ne!(hash1.values, hash2.values);
    }
}