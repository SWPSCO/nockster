/// Transaction ID computation
/// Implements the compute-id logic from Hoon's raw-tx

use super::hashable::Hashable;
use super::hasher::{hash_hashable, digest_to_base58};
use crate::collections::ZMap;
use crate::transaction_types::{Hash, NName, Input, TimelockRange, to_hashable_timelock_intent};

/// Compute the transaction ID from components
/// Mirrors Hoon's compute-id function:
/// ```hoon
/// ++  compute-id
///   |=  raw=form
///   ^-  tx-id
///   %-  hash-hashable:tip5
///   :+  (hashable:inputs inputs.raw)
///     (hashable:timelock-range timelock-range.raw)
///   leaf+total-fees.raw
/// ```
pub fn compute_tx_id(
    inputs: &ZMap<NName, Input>,
    timelock_range: &TimelockRange,
    total_fees: u64,
) -> Hash {
    // Build the hashable structure as a triple
    let hashable = Hashable::triple(
        // Convert inputs z-map to hashable
        inputs.to_hashable(
            |nname| nname.to_hashable(),
            |input| input.to_hashable(),
        ),
        // Convert timelock-range to hashable
        timelock_range.to_hashable(),
        // Total fees as leaf
        Hashable::leaf_u64(total_fees),
    );
    
    // Hash the structure
    hash_hashable(&hashable)
}

/// Compute tx-id and return as base58 string
pub fn compute_tx_id_base58(
    inputs: &ZMap<NName, Input>,
    timelock_range: &TimelockRange,
    total_fees: u64,
) -> String {
    let digest = compute_tx_id(inputs, timelock_range, total_fees);
    digest_to_base58(&digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compute_tx_id() {
        // Create a simple z-map with one input
        use crate::transaction_types::{NNote, NNoteHead, Source, Lock, Coins, Spend, PageNumber, Timelock, TimelockIntent, Seeds};
        use crate::collections::ZSet;
        
        let mut inputs = ZMap::new();
        let nname = NName {
            p: vec![
                Hash { values: [1, 2, 3, 4, 5] },
                Hash { values: [6, 7, 8, 9, 10] },
            ],
        };
        // Create a proper input for testing
        let input = Input {
            note: NNote {
                meta: NNoteHead {
                    version: 1,
                    origin_page: PageNumber { value: 100 },
                    timelock: Timelock {
                        intent: None,
                    },
                },
                name: NName { p: vec![] },
                lock: Lock { m: 1, pubkeys: ZSet::new() },
                source: Source { p: Hash { values: [11, 12, 13, 14, 15] }, is_coinbase: false },
                assets: Coins { value: 100 },
            },
            spend: Spend {
                signature: None,
                seeds: Seeds { set: ZSet::new() },
                fee: Coins { value: 10 },
            },
        };
        inputs.put(nname, input);
        
        let timelock_range = TimelockRange {
            min: None,
            max: None,
        };
        
        let total_fees = 100;
        
        // Compute tx-id
        let tx_id = compute_tx_id(&inputs, &timelock_range, total_fees);
        
        // Check we got a digest
        assert_eq!(tx_id.values.len(), 5);
        
        // Get base58 representation
        let base58 = compute_tx_id_base58(&inputs, &timelock_range, total_fees);
        assert!(!base58.is_empty());
        
        println!("TX-ID: {:?}", tx_id);
        println!("TX-ID (base58): {}", base58);
    }
    
    // Comprehensive tests for all the structures
    
    #[test]
    fn test_nnote_to_hashable_matches_hoon() {
        use crate::transaction_types::*;
        use crate::collections::ZSet;
        
        // Create the exact NNote from Hoon data
        // name: two hashes plus ~
        let name = NName {
            p: vec![
                Hash { values: [
                    0x1823f2b17cba6a60,
                    0xf21d6e6241adb7c2,
                    0xcc5a55974af38483,
                    0x95524fbf2e34cb94,
                    0xfd998aff51844889,
                ]},
                Hash { values: [
                    0x9df82628da0dc29b,
                    0x38ceea6661fb346a,
                    0xcec53e77ec5cab9e,
                    0xd60fe1a8b1e59ce1,
                    0xa7aa207617371ec2,
                ]},
            ],
        };
        
        // lock: m=1 with single pubkey
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
        
        let mut pubkeys = ZSet::new();
        pubkeys.put(pubkey);
        let lock = Lock { m: 1, pubkeys };
        
        // source: hash with is_coinbase=false
        let source = Source {
            p: Hash { values: [
                0x0805c2cf766b24c7,
                0xec565e7aad63dcb8,
                0x328490abbda9e422,
                0x8e0adeb4e57c29d9,
                0x7f6ec2a2e9b23141,
            ]},
            is_coinbase: false,
        };
        
        // meta: version=0, origin-page=42, timelock with ranges
        let meta = NNoteHead {
            version: 0,
            origin_page: PageNumber { value: 42 },
            timelock: Timelock {
                intent: Some((
                    TimelockRange {
                        min: Some(PageNumber { value: 100 }),
                        max: Some(PageNumber { value: 110 }),
                    },
                    TimelockRange {
                        min: Some(PageNumber { value: 10 }),
                        max: Some(PageNumber { value: 20 }),
                    },
                )),
            },
        };
        
        // assets: 1337
        let assets = Coins { value: 1337 };
        
        // Create the NNote
        let nnote = NNote {
            meta,
            name,
            lock,
            source,
            assets,
        };
        
        // Convert to hashable
        let hashable = nnote.to_hashable();
        
        // Also test the to_hash() method
        let hash_via_method = nnote.to_hash();
        
        // Expected hash from Hoon
        let expected_hash = Hash { values: [
            0x65ec8739eb1ca738,
            0x3ea446350267137e,
            0xc7ce451397619567,
            0x54d603f1c9f3520f,
            0xb1aa6c5ef3f23c9c,
        ]};
        
        let computed_hash = hash_hashable(&hashable);
        
        println!("\nNNote hash:");
        println!("Expected hash from Hoon: {:x?}", expected_hash.values);
        println!("Computed hash from Rust: {:x?}", computed_hash.values);
        println!("Hash via to_hash() method: {:x?}", hash_via_method.values);
        
        assert_eq!(computed_hash, expected_hash, "NNote hash should match Hoon");
        assert_eq!(hash_via_method, expected_hash, "NNote.to_hash() should match Hoon");
        
        println!("NNote to Hashable conversion matches Hoon! ✓");
    }
    
    #[test]
    fn test_spend_to_hashable_matches_hoon() {
        use crate::transaction_types::*;
        use crate::collections::ZSet;
        
        // Create test Spend
        let mut seed_set = ZSet::new();
        let seed = Seed {
            output_source: Some(Source {
                p: Hash { values: [1, 2, 3, 4, 5] },
                is_coinbase: false,
            }),
            recipient: Lock { m: 1, pubkeys: ZSet::new() },
            timelock_intent: None,
            gift: Coins { value: 100 },
            parent_hash: Hash { values: [5, 4, 3, 2, 1] },
        };
        seed_set.put(seed);
        
        let spend = Spend {
            signature: None,
            seeds: Seeds { set: seed_set },
            fee: Coins { value: 10 },
        };
        
        let hashable = spend.to_hashable();
        
        // Check structure
        match hashable {
            Hashable::Cell(sig_part, rest) => {
                // signature should be null (None)
                assert!(matches!(*sig_part, Hashable::Leaf(_)));
                match *rest {
                    Hashable::Cell(_seeds_part, fee_part) => {
                        // seeds_part should be the seeds hashable
                        // fee_part should be a cell with the fee
                        match *fee_part {
                            Hashable::Leaf(ref data) => {
                                let fee_val = u64::from_le_bytes(data[..8].try_into().unwrap());
                                assert_eq!(fee_val, 10);
                            }
                            _ => panic!("Expected fee to be a leaf"),
                        }
                    }
                    _ => panic!("Expected cell"),
                }
            }
            _ => panic!("Expected cell"),
        }
        
        println!("Spend to_hashable test passed!");
    }
    
    #[test]
    fn test_nnote_999_to_hashable_matches_hoon() {
        use crate::transaction_types::*;
        use crate::collections::ZSet;
        
        // Create the exact NNote from Hoon data (origin-page=999 version)
        // name: two hashes plus ~
        let name = NName {
            p: vec![
                Hash { values: [
                    0x6982_9223_e9f0_3ecc,
                    0xd0a6_5a96_1645_75aa,
                    0xae64_244f_4f6b_e0cd,
                    0xce5d_39f8_001d_bff8,
                    0xa2cf_fe02_fa6f_9206,
                ]},
                Hash { values: [
                    0x77a5_4d8a_71ff_ff5e,
                    0x3cf2_51a4_28ec_2921,
                    0xcb23_dc5f_2973_f677,
                    0x4ae8_83f7_9101_658b,
                    0x4abb_bb29_2f31_fc42,
                ]},
            ],
        };
        
        // lock: m=2 with two pubkeys
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
        
        let mut pubkeys = ZSet::new();
        
        // Debug: Let's compute the hashes used for ordering
        use nockapp::noun::slab::NounSlab;
        use noun_serde::NounEncode;
        use crate::hashing::tip5::Tip5Hasher;
        
        let mut slab1: NounSlab = NounSlab::new();
        let pubkey1_noun = pubkey1.to_noun(&mut slab1);
        let pubkey1_hash = Tip5Hasher::hash_noun(pubkey1_noun).unwrap();
        println!("\npubkey1 (x[0]={}): hash={:x?}", pubkey1.x.values[0], pubkey1_hash.values);
        
        let mut slab2: NounSlab = NounSlab::new();
        let pubkey2_noun = pubkey2.to_noun(&mut slab2);
        let pubkey2_hash = Tip5Hasher::hash_noun(pubkey2_noun).unwrap();
        println!("pubkey2 (x[0]={}): hash={:x?}", pubkey2.x.values[0], pubkey2_hash.values);
        
        // Convert hashes to UBig for comparison (same as gor_tip does)
        use ibig::UBig;
        let hash1_ubig = UBig::from_le_bytes(&{
            let mut bytes = Vec::new();
            for &val in pubkey1_hash.values.iter() {
                bytes.extend_from_slice(&val.to_le_bytes());
            }
            bytes
        });
        let hash2_ubig = UBig::from_le_bytes(&{
            let mut bytes = Vec::new();
            for &val in pubkey2_hash.values.iter() {
                bytes.extend_from_slice(&val.to_le_bytes());
            }
            bytes
        });
        
        println!("\nHash comparison (gor_tip ordering):");
        println!("pubkey1 hash as UBig: {}", hash1_ubig);
        println!("pubkey2 hash as UBig: {}", hash2_ubig);
        match hash1_ubig.cmp(&hash2_ubig) {
            std::cmp::Ordering::Less => println!("pubkey1 < pubkey2 (pubkey1 should come first)"),
            std::cmp::Ordering::Greater => println!("pubkey1 > pubkey2 (pubkey2 should come first)"),
            std::cmp::Ordering::Equal => println!("pubkey1 == pubkey2 (would use dor_tip fallback)"),
        }
        
        pubkeys.put(pubkey1.clone());
        pubkeys.put(pubkey2.clone());
        
        // Debug: Print the order of pubkeys in the tree
        println!("\nPubkeys in tree order:");
        for (i, pk) in pubkeys.iter().enumerate() {
            println!("  {}: x[0]={}, y[0]={}", i, pk.x.values[0], pk.y.values[0]);
        }
        
        let lock = Lock { m: 2, pubkeys };
        
        // Debug: Print the lock hashable
        let lock_hashable = lock.to_hashable();
        println!("\nLock hashable structure (partial):");
        match &lock_hashable {
            Hashable::Cell(m_part, pubkeys_part) => {
                println!("  m: {:?}", m_part);
                println!("  pubkeys structure type: {:?}", std::mem::discriminant(pubkeys_part.as_ref()));
            }
            _ => println!("  Unexpected lock hashable structure"),
        }
        
        // source: hash with is_coinbase=true (%.y)
        let source = Source {
            p: Hash { values: [
                0xfea1_d1f4_0d9f_7418,
                0x8d17_0651_a786_4595,
                0xb612_76f7_ac3c_5d66,
                0x57ab_26f9_081c_a3e4,
                0x89d0_b1cf_6da2_e3be,
            ]},
            is_coinbase: true,  // %.y = true
        };
        
        // meta: version=0, origin-page=999, timelock=~
        let meta = NNoteHead {
            version: 0,
            origin_page: PageNumber { value: 999 },
            timelock: Timelock {
                intent: None,  // Fixed: changed from old structure
            },
        };
        
        // assets: 50000
        let assets = Coins { value: 50_000 };
        
        // Create the NNote
        let nnote = NNote {
            meta,
            name,
            lock,
            source,
            assets,
        };
        
        // Convert to hashable
        let hashable = nnote.to_hashable();
        
        // Expected intermediate hashes from Hoon
        let expected_timelock_hash = Hash { values: [
            1_730_770_831_742_798_981,
            2_676_322_185_709_933_211,
            8_329_210_750_824_781_744,
            16_756_092_452_590_401_876,
            3_547_445_316_740_171_466,
        ]};
        
        let expected_name_hash = Hash { values: [
            14_782_930_557_674_113_914,
            9_669_739_630_935_817_120,
            3_762_528_132_326_415_848,
            6_425_292_481_844_412_528,
            6_322_876_085_907_856_569,
        ]};
        
        let expected_lock_hash = Hash { values: [
            15_008_028_953_498_235_592,
            1_256_070_566_009_709_546,
            14_335_510_939_895_011_282,
            6_449_775_178_652_276_721,
            14_374_030_633_419_849_529,
        ]};
        
        let expected_source_hash = Hash { values: [
            3_647_931_446_410_251_362,
            18_318_120_786_284_226_387,
            8_369_968_361_425_459_235,
            3_886_639_426_538_736_762,
            9_204_395_879_304_833_490,
        ]};
        
        // Verify intermediate hashes
        match &hashable {
            Hashable::Cell(meta_part, rest_part) => {
                // Check timelock hash in meta
                if let Hashable::Cell(_, origin_timelock) = meta_part.as_ref() {
                    if let Hashable::Cell(_, timelock_hash) = origin_timelock.as_ref() {
                        if let Hashable::Hash(hash) = timelock_hash.as_ref() {
                            assert_eq!(hash, &expected_timelock_hash, "Timelock hash mismatch");
                        }
                    }
                }
                
                // Check name, lock, source hashes
                if let Hashable::Cell(name_hash, lock_source_assets) = rest_part.as_ref() {
                    if let Hashable::Hash(hash) = name_hash.as_ref() {
                        assert_eq!(hash, &expected_name_hash, "Name hash mismatch");
                    }
                    
                    if let Hashable::Cell(lock_hash, source_assets) = lock_source_assets.as_ref() {
                        if let Hashable::Hash(hash) = lock_hash.as_ref() {
                            assert_eq!(hash, &expected_lock_hash, "Lock hash mismatch");
                        }
                        
                        if let Hashable::Cell(source_hash, _) = source_assets.as_ref() {
                            if let Hashable::Hash(hash) = source_hash.as_ref() {
                                assert_eq!(hash, &expected_source_hash, "Source hash mismatch");
                            }
                        }
                    }
                }
            }
            _ => panic!("Unexpected hashable structure"),
        }
        
        // Now verify the final hash
        let computed_hash = hash_hashable(&hashable);
        let expected_hash = Hash { values: [
            0x7546_e958_54fa_40c0,
            0x50ca_6bfd_a56f_073b,
            0xc4df_f03b_64e4_2be9,
            0xdf93_1d8a_ba6f_efc0,
            0x61a0_4c77_128b_1bab,
        ]};
        
        println!("\nNNote (origin-page=999) hash:");
        println!("Expected hash from Hoon: {:x?}", expected_hash.values);
        println!("Computed hash from Rust: {:x?}", computed_hash.values);
        
        assert_eq!(computed_hash, expected_hash, "NNote (origin-page=999) hash should match Hoon");
        
        println!("NNote (origin-page=999) test passes! ✓");
    }
    
    #[test]
    fn test_to_hash_methods() {
        use crate::transaction_types::*;
        use crate::collections::ZSet;
        
        // Test NName
        let nname = NName {
            p: vec![
                Hash { values: [1, 2, 3, 4, 5] },
                Hash { values: [6, 7, 8, 9, 10] },
            ],
        };
        let hash_from_method = nname.to_hash();
        let hash_from_manual = hash_hashable(&nname.to_hashable());
        assert_eq!(hash_from_method, hash_from_manual, "NName to_hash should match manual approach");
        
        // Test TimelockRange
        let timelock_range = TimelockRange {
            min: Some(PageNumber { value: 100 }),
            max: Some(PageNumber { value: 200 }),
        };
        let hash_from_method = timelock_range.to_hash();
        let hash_from_manual = hash_hashable(&timelock_range.to_hashable());
        assert_eq!(hash_from_method, hash_from_manual, "TimelockRange to_hash should match manual approach");
        
        // Test Timelock
        let timelock = Timelock {
            intent: Some((
                TimelockRange { min: None, max: None },
                TimelockRange { min: Some(PageNumber { value: 10 }), max: None },
            )),
        };
        let hash_from_method = timelock.to_hash();
        let hash_from_manual = hash_hashable(&timelock.to_hashable());
        assert_eq!(hash_from_method, hash_from_manual, "Timelock to_hash should match manual approach");
        
        // Test Source
        let source = Source {
            p: Hash { values: [11, 12, 13, 14, 15] },
            is_coinbase: false,
        };
        let hash_from_method = source.to_hash();
        let hash_from_manual = hash_hashable(&source.to_hashable());
        assert_eq!(hash_from_method, hash_from_manual, "Source to_hash should match manual approach");
        
        // Test Lock
        let mut pubkeys = ZSet::new();
        let pubkey = SchnorrPubkey {
            x: F6LT { values: [1, 2, 3, 4, 5, 6] },
            y: F6LT { values: [7, 8, 9, 10, 11, 12] },
            inf: false,
        };
        pubkeys.put(pubkey);
        let lock = Lock { m: 1, pubkeys };
        let hash_from_method = lock.to_hash();
        let hash_from_manual = hash_hashable(&lock.to_hashable());
        assert_eq!(hash_from_method, hash_from_manual, "Lock to_hash should match manual approach");
        
        // Test Seed
        let seed = Seed {
            output_source: None,
            recipient: Lock { m: 1, pubkeys: ZSet::new() },
            timelock_intent: None,
            gift: Coins { value: 100 },
            parent_hash: Hash { values: [20, 21, 22, 23, 24] },
        };
        let hash_from_method = seed.to_hash();
        let hash_from_manual = hash_hashable(&seed.to_hashable());
        assert_eq!(hash_from_method, hash_from_manual, "Seed to_hash should match manual approach");
        
        // Test Seeds
        let mut seed_set = ZSet::new();
        seed_set.put(seed);
        let seeds = Seeds { set: seed_set };
        let hash_from_method = seeds.to_hash();
        let hash_from_manual = hash_hashable(&seeds.to_hashable());
        assert_eq!(hash_from_method, hash_from_manual, "Seeds to_hash should match manual approach");
        
        // Test Spend
        let spend = Spend {
            signature: None,
            seeds: Seeds { set: ZSet::new() },
            fee: Coins { value: 10 },
        };
        let hash_from_method = spend.to_hash();
        let hash_from_manual = hash_hashable(&spend.to_hashable());
        assert_eq!(hash_from_method, hash_from_manual, "Spend to_hash should match manual approach");
        
        // Test Input
        let nnote = NNote {
            meta: NNoteHead {
                version: 0,
                origin_page: PageNumber { value: 42 },
                timelock: Timelock { intent: None },
            },
            name: NName { p: vec![] },
            lock: Lock { m: 1, pubkeys: ZSet::new() },
            source: Source {
                p: Hash { values: [30, 31, 32, 33, 34] },
                is_coinbase: false,
            },
            assets: Coins { value: 1000 },
        };
        
        let input = Input {
            note: nnote.clone(),
            spend: spend.clone(),
        };
        let hash_from_method = input.to_hash();
        let hash_from_manual = hash_hashable(&input.to_hashable());
        assert_eq!(hash_from_method, hash_from_manual, "Input to_hash should match manual approach");
        
        // Test NNote's to_hash (which already existed)
        let hash_from_method = nnote.to_hash();
        let hash_from_manual = hash_hashable(&nnote.to_hashable());
        assert_eq!(hash_from_method, hash_from_manual, "NNote to_hash should match manual approach");
        
        println!("All to_hash() methods work correctly!");
    }
}