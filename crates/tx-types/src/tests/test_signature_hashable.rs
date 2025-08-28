#[cfg(test)]
mod test_signature_hashable {
    use crate::transaction_types::*;
    use crate::collections::ZMap;
    use crate::hashing::hashable::Hashable;
    use crate::hashing::hasher::hash_hashable;
    
    #[test]
    fn test_signature_hashable_matches_hoon() {
        println!("\n=== Testing Signature Hashable Against Hoon ===\n");
        
        // First, let's create the signature structure from the previous test
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
        
        let mut sig_map = ZMap::new();
        sig_map.put(pubkey1.clone(), sig1);
        sig_map.put(pubkey2.clone(), sig2);
        sig_map.put(pubkey3.clone(), sig3);
        
        let signature = Signature { map: sig_map };
        
        // Get the hashable from our Rust implementation
        let rust_hashable = signature.to_hashable();
        
        // Now let's manually construct the expected hashable from Hoon
        // The structure is a tree with root node and left/right subtrees
        
        // Root node hash (pubkey1)
        let root_hash = Hash { values: [
            1_550_560_774_221_189_564,
            9_450_039_748_918_830_565,
            934_651_045_440_982_446,
            3_604_049_111_199_163_374,
            12_236_041_076_923_494_295,
        ]};
        
        // Root node signature data as bytes
        // chal=[8738, 13107, 17476, 21845, 26214, 30583, 34952, 39321]
        // sig=[48059, 52428, 56797, 61166, 65535, 4369, 9029, 26505]
        // Note: These are the decimal values from Hoon, not hex
        let root_sig_bytes = {
            let mut bytes = Vec::new();
            // Chal values (8 u64s)
            for val in [8738u64, 13107, 17476, 21845, 26214, 30583, 34952, 39321] {
                bytes.extend_from_slice(&val.to_le_bytes());
            }
            // Sig values (8 u64s)
            for val in [48059u64, 52428, 56797, 61166, 65535, 4369, 9029, 26505] {
                bytes.extend_from_slice(&val.to_le_bytes());
            }
            bytes
        };
        
        // Left subtree hash (pubkey2)
        let left_hash = Hash { values: [
            159_408_762_462_931_994,
            5_415_112_519_136_719_218,
            842_518_352_812_128_067,
            10_891_317_961_964_658_774,
            4_484_425_932_328_644_647,
        ]};
        
        // Left subtree signature data
        // chal=[13107, 17476, 21845, 26214, 30583, 34952, 39321, 43690]
        // sig=[52428, 56797, 61166, 65535, 4369, 8738, 13398, 30874]
        let left_sig_bytes = {
            let mut bytes = Vec::new();
            for val in [13107u64, 17476, 21845, 26214, 30583, 34952, 39321, 43690] {
                bytes.extend_from_slice(&val.to_le_bytes());
            }
            for val in [52428u64, 56797, 61166, 65535, 4369, 8738, 13398, 30874] {
                bytes.extend_from_slice(&val.to_le_bytes());
            }
            bytes
        };
        
        // Right subtree hash (pubkey3)
        let right_hash = Hash { values: [
            11_487_442_755_224_497_904,
            8_684_519_272_150_381_160,
            13_222_416_721_784_577_989,
            1_211_458_990_661_990,
            17_930_950_652_498_668_447,
        ]};
        
        // Right subtree signature data
        // chal=[4369, 8738, 13107, 17476, 21845, 26214, 30583, 34952]
        // sig=[43690, 48059, 52428, 56797, 61166, 65535, 4660, 22136]
        let right_sig_bytes = {
            let mut bytes = Vec::new();
            for val in [4369u64, 8738, 13107, 17476, 21845, 26214, 30583, 34952] {
                bytes.extend_from_slice(&val.to_le_bytes());
            }
            for val in [43690u64, 48059, 52428, 56797, 61166, 65535, 4660, 22136] {
                bytes.extend_from_slice(&val.to_le_bytes());
            }
            bytes
        };
        
        // Construct the expected hashable structure
        // The tree structure is:
        // [root_entry [left_tree right_tree]]
        // where each entry is [hash signature_data]
        // and empty subtrees are [leaf+0 leaf+0]
        
        let expected_hashable = Hashable::cell(
            // Root node: [hash, signature_data]
            Hashable::cell(
                Hashable::Hash(root_hash),
                Hashable::leaf(root_sig_bytes),
            ),
            // Subtrees
            Hashable::cell(
                // Left subtree
                Hashable::cell(
                    // Left node: [hash, signature_data]
                    Hashable::cell(
                        Hashable::Hash(left_hash),
                        Hashable::leaf(left_sig_bytes),
                    ),
                    // Left subtree's children (both empty)
                    Hashable::cell(
                        Hashable::cell(Hashable::null(), Hashable::null()),
                        Hashable::cell(Hashable::null(), Hashable::null()),
                    ),
                ),
                // Right subtree
                Hashable::cell(
                    // Right node: [hash, signature_data]
                    Hashable::cell(
                        Hashable::Hash(right_hash),
                        Hashable::leaf(right_sig_bytes),
                    ),
                    // Right subtree's children (both empty)
                    Hashable::cell(
                        Hashable::cell(Hashable::null(), Hashable::null()),
                        Hashable::cell(Hashable::null(), Hashable::null()),
                    ),
                ),
            ),
        );
        
        // Debug print both hashables
        println!("Rust hashable: {:?}", rust_hashable);
        println!("\nExpected hashable structure created");
        
        // Hash both and compare
        let rust_hash = match rust_hashable {
            Hashable::Hash(h) => h,
            _ => hash_hashable(&rust_hashable),
        };
        
        let expected_hash_from_hashable = hash_hashable(&expected_hashable);
        
        println!("\nRust Signature hash: {:016x?}", rust_hash.values);
        println!("Expected hash from manually constructed hashable: {:016x?}", expected_hash_from_hashable.values);
        
        // The expected final hash from Hoon
        let expected_final_hash = Hash { values: [
            0x8381_562f_eba1_fcbe,
            0xe398_01d5_5f0f_8b4a,
            0x5de0_0a11_9370_e112,
            0xd294_aed6_6d83_7564,
            0x3e39_8166_ceb1_2ab0,
        ]};
        
        println!("Expected final hash from Hoon: {:016x?}", expected_final_hash.values);
        
        if expected_hash_from_hashable != expected_final_hash {
            println!("\n❌ Manually constructed hashable doesn't produce expected hash!");
            println!("This means our understanding of the Hoon structure is incorrect.");
        } else {
            println!("\n✓ Manually constructed hashable produces correct hash!");
        }
        
        if rust_hash != expected_final_hash {
            println!("\n❌ Rust implementation doesn't match expected hash!");
            println!("The issue is in how Signature is converted to hashable.");
        } else {
            println!("\n✓ Rust implementation matches expected hash!");
        }
    }
}