/// Hashing module for transaction processing
/// 
/// This module contains all hashing-related functionality including:
/// - The Hashable trait and enum for creating hashable structures
/// - TIP5 hashing algorithm implementation
/// - Transaction ID computation
/// - General hashing utilities

pub mod hashable;
pub mod hasher;
pub mod tip5;
pub mod tx_id;

// Re-export commonly used items
pub use hashable::Hashable;
pub use hasher::{hash_hashable, hash_noun_varlen, hash_ten_cell};
pub use tip5::{Tip5Hasher, Tip5Error};
pub use tx_id::{compute_tx_id, compute_tx_id_base58};