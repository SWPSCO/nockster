#[cfg(test)]
mod test_schnorr_pubkey_hash {
    use crate::transaction_types::*;
    
    #[test]
    fn test_schnorr_pubkey_hash_matches_hoon() {
        println!("\n=== Testing SchnorrPubkey Hash Against Hoon ===\n");
        
        // Create the SchnorrPubkey from the Hoon data
        let pubkey = SchnorrPubkey {
            x: F6LT { values: [
                9_323_455_886_065_152_710,
                8_604_621_052_628_066_076,
                8_724_446_291_889_705_637,
                15_913_798_201_200_938_686,
                6_871_293_856_171_770_838,
                11_532_431_931_696_133_539,
            ]},
            y: F6LT { values: [
                10_242_415_564_008_566_488,
                10_485_181_329_625_226_048,
                8_639_946_714_446_054_618,
                4_053_240_175_695_272_783,
                11_730_999_058_788_639_792,
                14_820_844_833_610_271_254,
            ]},
            inf: false,  // %.n means false
        };
        
        // Hash the pubkey
        let computed_hash = pubkey.to_hash();
        
        // Expected hash from Hoon
        let expected_hash = Hash { values: [
            0x9f6b_9852_a2ca_6ef0,
            0x7885_9c43_f58e_b268,
            0xb77f_75f3_fe27_9fc5,
            0x0004_4dd0_c6cc_6166,  // Note: leading zeros are important
            0xf8d7_889d_bf3d_eb9f,
        ]};
        
        println!("Computed pubkey hash: {:016x?}", computed_hash.values);
        println!("Expected pubkey hash: {:016x?}", expected_hash.values);
        
        if computed_hash != expected_hash {
            println!("\n❌ SchnorrPubkey hash MISMATCH!");
            
            // Let's also check what the to_hashable returns
            let hashable = pubkey.to_hashable();
            println!("\nDebug: pubkey.to_hashable() = {:?}", hashable);
            
            // Try to understand the difference
            println!("\nDifference analysis:");
            for i in 0..5 {
                if computed_hash.values[i] != expected_hash.values[i] {
                    println!("  values[{}]: computed={:016x} expected={:016x}", 
                        i, computed_hash.values[i], expected_hash.values[i]);
                }
            }
        } else {
            println!("\n✓ SchnorrPubkey hash matches Hoon exactly!");
        }
        
        assert_eq!(computed_hash, expected_hash, "SchnorrPubkey hash should match Hoon!");
    }
}