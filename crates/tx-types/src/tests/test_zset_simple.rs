#[cfg(test)]
mod tests {
    use noun_serde::NounEncode;
    use crate::collections::zset::{ZSet, DorTip};
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
    fn test_zset_simple_1_and_2() {
        println!("\n=== Testing ZSet with just numbers 1 and 2 ===\n");
        
        // First compute the hashes of 1 and 2
        let key1 = NumKey(1);
        let key2 = NumKey(2);
        
        let hash1 = key1.tip_hash();
        let hash2 = key2.tip_hash();
        
        println!("Hash of 1: {:016x?}", hash1.values);
        println!("Hash of 2: {:016x?}", hash2.values);
        
        // Compare the hashes to see ordering
        let cmp = hash1.cmp(&hash2);
        println!("\nHash comparison (1 vs 2): {:?}", cmp);
        println!("This means hash(1) {} hash(2)", 
            match cmp {
                std::cmp::Ordering::Less => "<",
                std::cmp::Ordering::Greater => ">",
                std::cmp::Ordering::Equal => "==",
            }
        );
        
        // Now create the ZSet and add 1 and 2 in order
        let mut set = ZSet::new();
        
        println!("\n=== Inserting 1 ===");
        set.put_with_debug(NumKey(1));
        
        println!("\n=== Inserting 2 ===");
        set.put_with_debug(NumKey(2));
        
        // Check what's in the set
        println!("\n=== Checking membership ===");
        println!("Has 1: {}", set.has(&NumKey(1)));
        println!("Has 2: {}", set.has(&NumKey(2)));
        
        // Get the structure as a string
        println!("\n=== Tree Structure ===");
        let structure = set.debug_structure();
        println!("{}", structure);
        
        // Compute the hash of the whole set
        println!("\n=== ZSet Hash ===");
        let hashable = set.to_hashable(
            |key| {
                let hash = key.tip_hash();
                Hashable::Hash(hash)
            }
        );
        
        let set_hash = hash_hashable(&hashable);
        println!("Final ZSet hash: {:016x?}", set_hash.values);
        
        // Verify we have 2 elements
        assert_eq!(set.wyt(), 2, "Should have 2 elements in the set");
    }
    
    #[test]
    fn test_zset_simple_1_2_3() {
        println!("\n=== Testing ZSet with numbers 1, 2, and 3 ===\n");
        
        // First compute the hashes of 1, 2, and 3
        let key1 = NumKey(1);
        let key2 = NumKey(2);
        let key3 = NumKey(3);
        
        let hash1 = key1.tip_hash();
        let hash2 = key2.tip_hash();
        let hash3 = key3.tip_hash();
        
        println!("Hash of 1: {:016x?}", hash1.values);
        println!("Hash of 2: {:016x?}", hash2.values);
        println!("Hash of 3: {:016x?}", hash3.values);
        
        // Compare the hashes to see ordering
        println!("\nHash comparisons:");
        println!("hash(1) vs hash(2): {:?}", hash1.cmp(&hash2));
        println!("hash(2) vs hash(3): {:?}", hash2.cmp(&hash3));
        println!("hash(1) vs hash(3): {:?}", hash1.cmp(&hash3));
        
        // Sort to see the gor-tip order
        let mut sorted = vec![(1, hash1), (2, hash2), (3, hash3)];
        sorted.sort_by(|a, b| a.1.cmp(&b.1));
        println!("\nSorted by hash (gor-tip order):");
        for (num, _) in &sorted {
            println!("  {}", num);
        }
        
        // Now create the ZSet and add 1, 2, 3 in order
        let mut set = ZSet::new();
        
        println!("\n=== Inserting 1 ===");
        set.put_with_debug(NumKey(1));
        println!("\n--- Tree after inserting 1 ---");
        println!("{}", set.debug_structure());
        
        println!("\n=== Inserting 2 ===");
        set.put_with_debug(NumKey(2));
        println!("\n--- Tree after inserting 2 ---");
        println!("{}", set.debug_structure());
        
        println!("\n=== Inserting 3 ===");
        set.put_with_debug(NumKey(3));
        println!("\n--- Tree after inserting 3 ---");
        println!("{}", set.debug_structure());
        
        // Check what's in the set
        println!("\n=== Checking membership ===");
        println!("Has 1: {}", set.has(&NumKey(1)));
        println!("Has 2: {}", set.has(&NumKey(2)));
        println!("Has 3: {}", set.has(&NumKey(3)));
        
        // Get the final structure
        println!("\n=== Final Tree Structure ===");
        let structure = set.debug_structure();
        println!("{}", structure);
        
        // Compute the hash of the whole set
        println!("\n=== ZSet Hash ===");
        let hashable = set.to_hashable(
            |key| {
                let hash = key.tip_hash();
                Hashable::Hash(hash)
            }
        );
        
        let set_hash = hash_hashable(&hashable);
        println!("Final ZSet hash: {:016x?}", set_hash.values);
        
        // Verify we have 3 elements
        assert_eq!(set.wyt(), 3, "Should have 3 elements in the set");
    }
}