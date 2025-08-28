/// Transaction processing module
/// 
/// This module contains all transaction-related functionality organized into:
/// - Core transaction types (transaction_types.rs)
/// - Noun encoding/decoding (to_noun.rs)
/// - Hashing algorithms and transaction ID computation (hashing/)
/// - Data structures like Z-maps (collections/)
/// - Transaction validation logic (validation/)

// Core modules in root
pub mod transaction_types;
pub mod tx_to_noun;

// Submodules
pub mod hashing;
pub mod collections;
pub mod validation;
pub mod u320;


// Re-export commonly used types from submodules
pub use hashing::{
    hashable::Hashable,
    hasher::{hash_hashable, hash_noun_varlen, hash_ten_cell},
    tip5::{Tip5Hasher, Tip5Error},
    tx_id::{compute_tx_id, compute_tx_id_base58},
};

pub use collections::{zmap::ZMap, zset::{ZSet, DorTip}};

pub use validation::{validator::{TransactionValidator, TransactionValidationError}};

// Re-export main transaction types
pub use transaction_types::*;

// Test modules
#[cfg(test)]
mod tests;