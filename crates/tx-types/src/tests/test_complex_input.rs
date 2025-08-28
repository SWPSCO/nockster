#[cfg(test)]
mod test_complex_input {
    use crate::transaction_types::*;
    use crate::collections::{ZSet, ZMap};
    
    #[test]
    fn test_complex_input_hash_matches_hoon() {
        println!("\n=== Testing Complex Input Hash Against Hoon ===\n");
        
        // Create the three pubkeys that appear in the lock and seeds
        let pubkey1 = SchnorrPubkey {
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
        
        let pubkey2 = SchnorrPubkey {
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
        
        // Create the NNote
        let name = NName {
            p: vec![
                Hash { values: [
                    0x39a2_c47c_d74a_c703,
                    0xe694_ea41_e5b6_3636,
                    0x247e_1bbd_ba16_aa3b,
                    0xb417_3248_d3ba_8e89,
                    0xfbd2_68f1_cbe1_b8c8,
                ]},
                Hash { values: [
                    0x9df8_2628_da0d_c29b,
                    0x38ce_ea66_61fb_346a,
                    0xcec5_3e77_ec5c_ab9e,
                    0xd60f_e1a8_b1e5_9ce1,
                    0xa7aa_2076_1737_1ec2,
                ]},
            ],
        };
        
        // Create the lock with 3 pubkeys
        let mut lock_pubkeys = ZSet::new();
        lock_pubkeys.put(pubkey1.clone());
        lock_pubkeys.put(pubkey2.clone());
        lock_pubkeys.put(pubkey3.clone());
        
        let lock = Lock {
            m: 1,
            pubkeys: lock_pubkeys,
        };
        
        let source = Source {
            p: Hash { values: [
                0x0805_c2cf_766b_24c7,
                0xec56_5e7a_ad63_dcb8,
                0x3284_90ab_bda9_e422,
                0x8e0a_deb4_e57c_29d9,
                0x7f6e_c2a2_e9b2_3141,
            ]},
            is_coinbase: false,
        };
        
        let meta = NNoteHead {
            version: 0,
            origin_page: PageNumber { value: 999 },
            timelock: Timelock {
                intent: Some((
                    TimelockRange {
                        min: Some(PageNumber { value: 1_000 }),
                        max: Some(PageNumber { value: 10_000 }),
                    },
                    TimelockRange {
                        min: Some(PageNumber { value: 100 }),
                        max: Some(PageNumber { value: 500 }),
                    },
                )),
            },
        };
        
        let nnote = NNote {
            meta,
            name,
            lock,
            source,
            assets: Coins { value: 50_000 },
        };
        
        // Create the signature
        let mut sig_map = ZMap::new();
        sig_map.put(
            pubkey3.clone(),
            SchnorrSignature {
                chal: Chal { values: T8 { values: [
                    0x9967_8da9,
                    0x80ac_aacd,
                    0x6288_f325,
                    0x3541_baa6,
                    0xf414_aea5,
                    0xc115_6f69,
                    0xcc45_480d,
                    0x44db_20c3,
                ]}},
                sig: Sig { values: T8 { values: [
                    0x9c95_5d1e,
                    0xa5b0_199f,
                    0x8e9f_9858,
                    0x82dc_90c2,
                    0x1d96_d032,
                    0xe7aa_f7fd,
                    0x925c_40c0,
                    0x6e6d_6155,
                ]}},
            },
        );
        
        let signature = Some(Signature { map: sig_map });
        
        // Create the seeds
        // The parent hash that appears in all seeds
        let parent_hash = Hash { values: [
            0x2be3_176d_4980_f2c3,
            0x5ac8_b1b4_eeed_cb3b,
            0x3eb4_1422_1733_40d9,
            0xfcbc_e735_4ce9_3c62,
            0x0bf4_4b9c_2ed8_af11,  // Note: leading 0 is important
        ]};
        
        // Seed 1 (the root node)
        let seed1 = Seed {
            output_source: None,
            recipient: Lock {
                m: 1,
                pubkeys: {
                    let mut pks = ZSet::new();
                    pks.put(pubkey1.clone());
                    pks.put(pubkey3.clone());
                    pks
                },
            },
            timelock_intent: Some((
                TimelockRange {
                    min: Some(PageNumber { value: 2_000 }),
                    max: Some(PageNumber { value: 20_000 }),
                },
                TimelockRange {
                    min: Some(PageNumber { value: 50 }),
                    max: Some(PageNumber { value: 200 }),
                },
            )),
            gift: Coins { value: 15_000 },
            parent_hash: parent_hash.clone(),
        };
        
        // Seed 2 (in right subtree)
        let seed2 = Seed {
            output_source: None,
            recipient: Lock {
                m: 1,
                pubkeys: {
                    let mut pks = ZSet::new();
                    pks.put(pubkey1.clone());
                    pks.put(pubkey2.clone());
                    pks.put(pubkey3.clone());
                    pks
                },
            },
            timelock_intent: Some((
                TimelockRange {
                    min: Some(PageNumber { value: 500 }),
                    max: Some(PageNumber { value: 1_500 }),
                },
                TimelockRange {
                    min: None,
                    max: None,
                },
            )),
            gift: Coins { value: 20_000 },
            parent_hash: parent_hash.clone(),
        };
        
        // Seed 3 (in right subtree)
        let seed3 = Seed {
            output_source: None,
            recipient: Lock {
                m: 1,
                pubkeys: {
                    let mut pks = ZSet::new();
                    pks.put(pubkey1.clone());
                    pks.put(pubkey3.clone());
                    pks
                },
            },
            timelock_intent: None,
            gift: Coins { value: 14_990 },
            parent_hash: parent_hash.clone(),
        };
        
        // Create the seeds ZSet
        // The Hoon shows a specific tree structure:
        // - root: seed1
        // - left: empty
        // - right: {seed2, seed3}
        let mut seed_set = ZSet::new();
        seed_set.put(seed1);
        seed_set.put(seed2);
        seed_set.put(seed3);
        
        let spend = Spend {
            signature,
            seeds: Seeds { set: seed_set },
            fee: Coins { value: 10 },
        };
        
        // Create the Input
        let input = Input {
            note: nnote.clone(),
            spend: spend.clone(),
        };
        
        // Debug: Hash the individual components
        println!("\nDebug: Component hashes:");
        let note_hash = nnote.to_hash();
        println!("  NNote hash: {:x?}", note_hash.values);
        
        // Expected NNote hash from Hoon
        let expected_nnote_hash = Hash { values: [
            0x2be3_176d_4980_f2c3,
            0x5ac8_b1b4_eeed_cb3b,
            0x3eb4_1422_1733_40d9,
            0xfcbc_e735_4ce9_3c62,
            0x0bf4_4b9c_2ed8_af11,  // Note: leading 0 is important
        ]};
        println!("  Expected NNote hash: {:x?}", expected_nnote_hash.values);
        
        if note_hash != expected_nnote_hash {
            println!("  ❌ NNote hash MISMATCH!");
        } else {
            println!("  ✓ NNote hash matches!");
        }
        
        let spend_hash = spend.to_hash();
        println!("  Spend hash: {:x?}", spend_hash.values);
        
        // Expected Spend hash from Hoon
        let expected_spend_hash = Hash { values: [
            0x2f4a_96c0_a13c_1990,
            0xf1a7_3c8c_a20b_e0c8,
            0x80da_d4cd_ec5b_4864,
            0xa4eb_04ba_79de_1a41,
            0x2f46_3850_6132_dd89,
        ]};
        println!("  Expected Spend hash: {:x?}", expected_spend_hash.values);
        
        if spend_hash != expected_spend_hash {
            println!("  ❌ Spend hash MISMATCH!");
        } else {
            println!("  ✓ Spend hash matches!");
        }
        
        // Debug: Check the seeds
        println!("\nDebug: Seeds in ZSet (count={}):", spend.seeds.set.wyt());
        for (i, seed) in spend.seeds.set.iter().enumerate() {
            println!("  Seed {}: gift={}, parent_hash={:016x?}", 
                i, seed.gift.value, &seed.parent_hash.values);
            let seed_hash = seed.to_hash();
            println!("    Seed {} hash: {:016x?}", i, seed_hash.values);
        }
        
        // Debug the Spend components individually
        println!("\nDebug: Spend components:");
        if let Some(ref sig) = spend.signature {
            let sig_hash = sig.to_hash();
            println!("  Signature hash: {:016x?}", sig_hash.values);
        } else {
            println!("  Signature: None");
        }
        let seeds_hash = spend.seeds.to_hash();
        println!("  Seeds hash: {:016x?}", seeds_hash.values);
        
        // Expected Seeds hash from Hoon
        let expected_seeds_hash = Hash { values: [
            0xdcea_1fd8_053f_9755,
            0x0ea7_2e01_e9ca_2acd,
            0xaa16_6cc7_bec2_de8d,
            0xf042_374f_dbf9_e5ab,
            0x5d8a_2b92_5e9f_8b73,
        ]};
        println!("  Expected Seeds hash: {:016x?}", expected_seeds_hash.values);
        
        if seeds_hash != expected_seeds_hash {
            println!("  ❌ Seeds hash MISMATCH!");
        } else {
            println!("  ✓ Seeds hash matches!");
        }
        
        println!("  Fee: {}", spend.fee.value);
        
        // Hash the Input
        let computed_hash = input.to_hash();
        
        // Expected hash from Hoon
        let expected_hash = Hash { values: [
            0xea33_22a3_7c15_9ef5,
            0xa1a9_0fc6_1530_359c,
            0x0fd4_844f_5b58_9168,
            0xa2d9_a0cc_ffca_15e9,
            0xe316_afb5_e8ed_377b,
        ]};
        
        println!("Computed Input hash: {:x?}", computed_hash.values);
        println!("Expected Input hash: {:x?}", expected_hash.values);
        
        assert_eq!(computed_hash, expected_hash, "Input hash should match Hoon!");
        println!("\n✓ Complex Input hash matches Hoon exactly!");
    }
}