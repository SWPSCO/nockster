/// Hashable enum mirroring Hoon's hashable type
/// This is an intermediate representation for hashing complex structures

use std::fmt::Debug;
use crate::transaction_types::Hash;

/// Hashable representation matching Hoon's hashable type
#[derive(Debug, Clone)]
pub enum Hashable {
    /// Raw data to be hashed (equivalent to [%leaf p=*])
    Leaf(Vec<u8>),
    
    /// Pre-computed hash (equivalent to [%hash p=noun-digest])
    Hash(Hash),
    
    /// Binary cell structure (equivalent to [p=hashable q=hashable])
    Cell(Box<Hashable>, Box<Hashable>),
    
    /// List of hashables (equivalent to [%list p=(list hashable)])
    List(Vec<Hashable>),
}

impl Hashable {
    /// Create a leaf from any byte slice
    pub fn leaf(data: impl AsRef<[u8]>) -> Self {
        Hashable::Leaf(data.as_ref().to_vec())
    }
    
    /// Create a leaf representing null/empty
    pub fn null() -> Self {
        // In Hoon, null (0) is represented as leaf+0
        // This should be a leaf containing the bytes for 0
        Hashable::Leaf(0u64.to_le_bytes().to_vec())
    }
    
    /// Create a leaf from a u64 value
    pub fn leaf_u64(value: u64) -> Self {
        Hashable::Leaf(value.to_le_bytes().to_vec())
    }
    
    /// Create a pre-computed hash node
    pub fn hash(digest: Hash) -> Self {
        Hashable::Hash(digest)
    }
    
    /// Create a cell from two hashables
    pub fn cell(left: Hashable, right: Hashable) -> Self {
        Hashable::Cell(Box::new(left), Box::new(right))
    }
    
    /// Create a triple (syntactic sugar for nested cells)
    /// Equivalent to Hoon's :+ operator
    pub fn triple(first: Hashable, second: Hashable, third: Hashable) -> Self {
        Hashable::Cell(
            Box::new(first),
            Box::new(Hashable::Cell(Box::new(second), Box::new(third)))
        )
    }
    
    /// Create a list of hashables
    pub fn list(items: Vec<Hashable>) -> Self {
        Hashable::List(items)
    }
}