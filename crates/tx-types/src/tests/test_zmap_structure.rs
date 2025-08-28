#[cfg(test)]
mod tests {
    use noun_serde::NounEncode;
    use crate::collections::zmap::ZMap;
    use crate::collections::zset::DorTip;
    use crate::transaction_types::Hash;
    use crate::hashing::hasher::hash_hashable;
    use crate::hashing::hashable::Hashable;
    
    // Simple wrapper for u64 to implement required traits
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct NumKey(u64);
    
    impl noun_serde::NounEncode for NumKey {
        fn to_noun<A: nockvm::noun::NounAllocator>(&self, alloc: &mut A) -> nockvm::noun::Noun {
            use nockvm::noun::Atom;
            Atom::new(alloc, self.0).as_noun()
        }
    }
    
    // NumKey already implements DorTip via Ord (blanket implementation)
    
    impl NumKey {
        fn tip_hash(&self) -> Hash {
            // Hash the number as a noun
            use nockapp::noun::slab::NounSlab;
            use crate::hashing::tip5::Tip5Hasher;
            
            let mut slab: NounSlab = NounSlab::new();
            let noun = self.to_noun(&mut slab);
            Tip5Hasher::hash_noun(noun).unwrap_or_else(|_| Hash { values: [0; 5] })
        }
    }
    
    #[test]
    fn test_numbers_1_to_10_zmap_structure() {
        println!("\n=== Testing ZMap Structure with Numbers 1-10 ===\n");
        
        let mut map = ZMap::new();
        
        // Insert numbers 1 through 10 with their string representation as values
        for i in 1..=10 {
            println!("Inserting key {}", i);
            map.put(NumKey(i), format!("value_{}", i));
        }
        
        println!("\n=== ZMap Contents ===");
        // Check each key to see what's in the map
        for i in 1..=10 {
            if let Some(value) = map.get(&NumKey(i)) {
                println!("  Key: {}, Value: {}", i, value);
            }
        }
        
        // We can't directly access the tree structure since root is private
        println!("\n=== ZMap Size ===");
        println!("Number of elements: {}", map.wyt());
        
        // Check the hash ordering by computing hashes for each key
        println!("\n=== Hash Values for Keys ===");
        let mut hash_pairs: Vec<(u64, Hash)> = Vec::new();
        for i in 1..=10 {
            let key = NumKey(i);
            let hash = key.tip_hash();
            println!("  Key {}: hash = {:016x?}", i, &hash.values[0..2]);
            hash_pairs.push((i, hash));
        }
        
        // Sort by hash to see the gor-tip ordering
        hash_pairs.sort_by(|a, b| {
            // Implement gor-tip comparison (single hash comparison)
            a.1.cmp(&b.1)
        });
        
        println!("\n=== Keys Sorted by Hash (gor-tip order) ===");
        for (num, hash) in &hash_pairs {
            println!("  Key {}: hash = {:016x?}", num, &hash.values[0..2]);
        }
        
        // Test the hashable representation
        println!("\n=== ZMap Hashable Structure ===");
        let hashable = map.to_hashable(
            |key| {
                // For keys, we'll hash them
                let hash = key.tip_hash();
                Hashable::Hash(hash)
            },
            |val| {
                // For values, we'll use them as leaf data
                Hashable::Leaf(val.as_bytes().to_vec())
            }
        );
        
        // Compute the final hash
        let map_hash = hash_hashable(&hashable);
        println!("Final ZMap hash: {:016x?}", map_hash.values);
        
        // Verify we have all 10 elements
        assert_eq!(map.wyt(), 10, "Should have 10 elements in the map");
    }
}