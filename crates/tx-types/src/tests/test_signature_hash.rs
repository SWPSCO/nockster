#[cfg(test)]
mod test_signature_hash {
    use crate::transaction_types::*;
    use crate::collections::ZMap;
    
    #[test]
    fn test_signature_hash_matches_hoon() {
        println!("\n=== Testing Signature Hash Against Hoon ===\n");
        
        // Create the three pubkeys
        // Pubkey 1 (middle/root node based on hash ordering)
        let pubkey1 = SchnorrPubkey {
            x: F6LT { values: [
                12_487_265_248_076_809_436,
                17_319_685_696_360_017_128,
                376_096_759_924_622_883,
                12_357_718_953_192_005_415,
                1_813_709_243_440_642_035,
                13_183_585_707_273_158_019,
            ]},
            y: F6LT { values: [
                1_221_896_489_402_813_317,
                4_702_927_297_152_298_800,
                15_400_301_787_023_911_298,
                5_131_927_075_907_562_501,
                13_208_991_022_157_055_578,
                10_078_243_364_526_679_113,
            ]},
            inf: false,
        };
        
        // Pubkey 2 (left subtree)
        let pubkey2 = SchnorrPubkey {
            x: F6LT { values: [
                566_273_053_357_821_052,
                4_788_914_443_680_986_537,
                8_415_538_053_559_789_354,
                6_139_886_333_872_072_363,
                5_982_840_667_074_872_231,
                3_576_629_195_875_272_167,
            ]},
            y: F6LT { values: [
                15_011_107_681_872_349_543,
                13_458_149_730_927_283_597,
                4_493_098_844_657_385_094,
                16_216_728_320_903_752_444,
                4_842_233_851_133_808_121,
                15_566_351_388_284_388_351,
            ]},
            inf: false,
        };
        
        // Pubkey 3 (right subtree)
        let pubkey3 = SchnorrPubkey {
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
            inf: false,
        };
        
        // Create the signatures for each pubkey
        let sig1 = SchnorrSignature {
            chal: Chal { values: T8 { values: [
                0x2222, 0x3333, 0x4444, 0x5555,
                0x6666, 0x7777, 0x8888, 0x9999,
            ]}},
            sig: Sig { values: T8 { values: [
                0xbbbb, 0xcccc, 0xdddd, 0xeeee,
                0xffff, 0x1111, 0x2345, 0x6789,
            ]}},
        };
        
        let sig2 = SchnorrSignature {
            chal: Chal { values: T8 { values: [
                0x3333, 0x4444, 0x5555, 0x6666,
                0x7777, 0x8888, 0x9999, 0xaaaa,
            ]}},
            sig: Sig { values: T8 { values: [
                0xcccc, 0xdddd, 0xeeee, 0xffff,
                0x1111, 0x2222, 0x3456, 0x789a,
            ]}},
        };
        
        let sig3 = SchnorrSignature {
            chal: Chal { values: T8 { values: [
                0x1111, 0x2222, 0x3333, 0x4444,
                0x5555, 0x6666, 0x7777, 0x8888,
            ]}},
            sig: Sig { values: T8 { values: [
                0xaaaa, 0xbbbb, 0xcccc, 0xdddd,
                0xeeee, 0xffff, 0x1234, 0x5678,
            ]}},
        };
        
        // Create the signature map
        let mut sig_map = ZMap::new();
        sig_map.put(pubkey1.clone(), sig1);
        sig_map.put(pubkey2.clone(), sig2);
        sig_map.put(pubkey3.clone(), sig3);
        
        let signature = Signature { map: sig_map };
        
        // Debug: Check the tree structure
        println!("Debug: Signature map has {} entries", signature.map.wyt());
        
        // Hash the signature
        let computed_hash = signature.to_hash();
        
        // Expected hash from Hoon
        let expected_hash = Hash { values: [
            0x8381_562f_eba1_fcbe,
            0xe398_01d5_5f0f_8b4a,
            0x5de0_0a11_9370_e112,
            0xd294_aed6_6d83_7564,
            0x3e39_8166_ceb1_2ab0,
        ]};
        
        println!("Computed Signature hash: {:x?}", computed_hash.values);
        println!("Expected Signature hash: {:x?}", expected_hash.values);
        
        assert_eq!(computed_hash, expected_hash, "Signature hash should match Hoon!");
        println!("\n✓ Signature hash matches Hoon exactly!");
    }
}