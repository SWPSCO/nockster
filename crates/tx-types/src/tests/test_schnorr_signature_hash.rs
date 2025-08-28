#[cfg(test)]
mod test_schnorr_signature_hash {
    use crate::transaction_types::*;
    
    #[test]
    fn test_schnorr_signature_hash_matches_hoon() {
        println!("\n=== Testing SchnorrSignature Hash Against Hoon ===\n");
        
        // Create the SchnorrSignature from the Hoon data
        let signature = SchnorrSignature {
            chal: Chal { values: T8 { values: [
                0x1111, 0x2222, 0x3333, 0x4444,
                0x5555, 0x6666, 0x7777, 0x8888,
            ]}},
            sig: Sig { values: T8 { values: [
                0xaaaa, 0xbbbb, 0xcccc, 0xdddd,
                0xeeee, 0xffff, 0x1234, 0x5678,
            ]}},
        };
        
        // Hash the signature
        let computed_hash = signature.to_hash();
        
        // Expected hash from Hoon
        let expected_hash = Hash { values: [
            0x39b6_b9c8_c0d4_cc87,
            0xda0f_159f_c638_ab5f,
            0x4e4a_0f6f_872d_d3a1,
            0x5527_81a2_b92a_21ca,
            0x0e61_9c04_01cc_3ccb,  // Note: leading 0 is important
        ]};
        
        println!("Computed signature hash: {:016x?}", computed_hash.values);
        println!("Expected signature hash: {:016x?}", expected_hash.values);
        
        if computed_hash != expected_hash {
            println!("\n❌ SchnorrSignature hash MISMATCH!");
            
            // Let's also check what the to_hashable returns
            let hashable = signature.to_hashable();
            println!("\nDebug: signature.to_hashable() = {:?}", hashable);
            
            // Check the raw bytes
            if let crate::hashing::hashable::Hashable::Leaf(ref bytes) = hashable {
                println!("\nSignature bytes (len={}):", bytes.len());
                println!("  Chal bytes: {:02x?}", &bytes[0..64]);
                println!("  Sig bytes:  {:02x?}", &bytes[64..128]);
                
                // Also show as u64 values
                println!("\nAs u64 values:");
                for i in 0..8 {
                    let chal_val = u64::from_le_bytes([
                        bytes[i*8], bytes[i*8+1], bytes[i*8+2], bytes[i*8+3],
                        bytes[i*8+4], bytes[i*8+5], bytes[i*8+6], bytes[i*8+7],
                    ]);
                    let sig_val = u64::from_le_bytes([
                        bytes[64+i*8], bytes[64+i*8+1], bytes[64+i*8+2], bytes[64+i*8+3],
                        bytes[64+i*8+4], bytes[64+i*8+5], bytes[64+i*8+6], bytes[64+i*8+7],
                    ]);
                    println!("  [{}]: chal=0x{:04x}, sig=0x{:04x}", i, chal_val, sig_val);
                }
            }
            
            // Try to understand the difference
            println!("\nDifference analysis:");
            for i in 0..5 {
                if computed_hash.values[i] != expected_hash.values[i] {
                    println!("  values[{}]: computed={:016x} expected={:016x}", 
                        i, computed_hash.values[i], expected_hash.values[i]);
                }
            }
        } else {
            println!("\n✓ SchnorrSignature hash matches Hoon exactly!");
        }
        
        assert_eq!(computed_hash, expected_hash, "SchnorrSignature hash should match Hoon!");
    }
}