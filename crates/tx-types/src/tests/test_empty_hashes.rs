#[cfg(test)]
mod test_empty {
    use crate::transaction_types::*;
    use crate::hashing::hasher::hash_hashable;
    
    #[test]
    fn test_empty_structure_hashes() {
        // Test empty NName
        let empty_name = NName { p: vec![] };
        let name_hashable = empty_name.to_hashable();
        let name_hash = hash_hashable(&name_hashable);
        println!("Empty NName hash: {:x?}", name_hash.values);
        
        // Test empty timelock
        let empty_timelock = Timelock { intent: None };
        let timelock_hashable = empty_timelock.to_hashable();
        let timelock_hash = hash_hashable(&timelock_hashable);
        println!("Empty Timelock hash: {:x?}", timelock_hash.values);
        
        // Both should be the same (null/0 hashes)
        assert_eq!(name_hash, timelock_hash, "Empty structures should hash the same");
    }
}
