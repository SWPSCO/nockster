#[cfg(test)]
mod tests {
    use crate::transaction_types::{Hash, SchnorrPubkey, Lock, F6LT};
    use crate::collections::ZSet;
    
    #[test]
    fn test_hash_base58_roundtrip() {
        // Test with a known hash
        let hash = Hash {
            values: [
                0x1823f2b17cba6a60,
                0xf21d6e6241adb7c2,
                0xcc5a55974af38483,
                0x95524fbf2e34cb94,
                0xfd998aff51844889,
            ]
        };
        
        // Convert to base58
        let base58 = hash.to_b58();
        println!("Hash base58: {}", base58);
        
        // Convert back from base58
        let decoded = Hash::from_b58(&base58).expect("Should decode");
        
        // Should match original
        assert_eq!(hash, decoded, "Hash should roundtrip through base58");
    }
    
    #[test]
    fn test_schnorr_pubkey_base58_roundtrip() {
        // Test with a known pubkey
        let pubkey = SchnorrPubkey {
            x: F6LT { values: [
                9323455886065152710,
                8604621052628066076,
                8724446291889705637,
                15913798201200938686,
                6871293856171770838,
                11532431931696133539,
            ]},
            y: F6LT { values: [
                10242415564008566488,
                10485181329625226048,
                8639946714446054618,
                4053240175695272783,
                11730999058788639792,
                14820844833610271254,
            ]},
            inf: false,
        };
        
        // Convert to base58
        let base58 = pubkey.to_b58();
        println!("Pubkey base58: {}", base58);
        
        // Convert back from base58
        let decoded = SchnorrPubkey::from_b58(&base58).expect("Should decode");
        
        // Should match original
        assert_eq!(pubkey, decoded, "Pubkey should roundtrip through base58");
    }
    
    #[test]
    fn test_lock_base58_roundtrip() {
        // Create a 2-of-3 multisig lock
        let pubkey1 = SchnorrPubkey {
            x: F6LT { values: [1, 2, 3, 4, 5, 6] },
            y: F6LT { values: [7, 8, 9, 10, 11, 12] },
            inf: false,
        };
        
        let pubkey2 = SchnorrPubkey {
            x: F6LT { values: [13, 14, 15, 16, 17, 18] },
            y: F6LT { values: [19, 20, 21, 22, 23, 24] },
            inf: false,
        };
        
        let pubkey3 = SchnorrPubkey {
            x: F6LT { values: [25, 26, 27, 28, 29, 30] },
            y: F6LT { values: [31, 32, 33, 34, 35, 36] },
            inf: false,
        };
        
        let mut pubkeys = ZSet::new();
        pubkeys.put(pubkey1.clone());
        pubkeys.put(pubkey2.clone());
        pubkeys.put(pubkey3.clone());
        
        let lock = Lock { m: 2, pubkeys };
        
        // Convert to base58
        let (m, pks_b58) = lock.to_b58();
        println!("Lock m: {}, pubkeys: {:?}", m, pks_b58);
        
        // Convert back from base58
        let decoded = Lock::from_b58(m, pks_b58).expect("Should decode");
        
        // Should match original (note: ordering might differ in ZSet)
        assert_eq!(lock.m, decoded.m, "m value should match");
        assert_eq!(lock.pubkeys.len(), decoded.pubkeys.len(), "Should have same number of pubkeys");
        
        // Check each pubkey is present
        for pk in lock.pubkeys.iter() {
            assert!(decoded.pubkeys.iter().any(|dpk| dpk == pk), "Pubkey should be present in decoded lock");
        }
    }
    
    #[test]
    fn test_hash_from_known_base58() {
        // Test with a known base58 string from RPC data
        // This is a placeholder test - in production you'd use actual RPC data
        let base58 = "7svA4MFCaQojDx8tUBsPCQYqBNzh15Z7n5C989pb2FZGHR9MbcykZpe";
        
        // Should decode without error
        let hash = Hash::from_b58(base58).expect("Should decode known base58");
        
        // Convert back to base58
        let encoded = hash.to_b58();
        
        // Should match original
        assert_eq!(base58, encoded, "Should match original base58");
    }
    
    #[test]
    fn test_invalid_base58() {
        // Test with invalid base58 strings
        assert!(Hash::from_b58("not-valid-base58!@#").is_err());
        assert!(SchnorrPubkey::from_b58("invalid!@#$").is_err());
    }
    
    #[test]
    fn test_lock_validation() {
        // Test that Lock validates m value correctly
        let pubkey = SchnorrPubkey {
            x: F6LT { values: [1, 2, 3, 4, 5, 6] },
            y: F6LT { values: [7, 8, 9, 10, 11, 12] },
            inf: false,
        };
        
        let pk_b58 = vec![pubkey.to_b58()];
        
        // m = 0 should fail
        assert!(Lock::from_b58(0, pk_b58.clone()).is_err());
        
        // m > n should fail
        assert!(Lock::from_b58(2, pk_b58.clone()).is_err());
        
        // m = 1, n = 1 should succeed
        assert!(Lock::from_b58(1, pk_b58).is_ok());
    }
}