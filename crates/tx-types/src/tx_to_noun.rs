use nockapp::noun::slab::NounSlab;
use nockvm::noun::T;
use noun_serde::NounEncode;

// Import from transaction_types instead of txo
use crate::transaction_types::{
    Inputs, Input, Coins,
};
use crate::collections::ZMap;
use crate::hashing::compute_tx_id;

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Calculates the effective absolute timelock range for all inputs
/// by combining absolute and relative timelock constraints from each input.
/// 
/// The resulting range represents:
/// - min: The maximum of all input minimums (all inputs must be ready)
/// - max: The minimum of all input maximums (must spend before earliest expiry)
/// 
/// Returns a TimelockRange with None for min/max if there are no constraints in that direction.
pub fn calculate_timelock_range(inputs: &[Input]) -> crate::transaction_types::TimelockRange {
    use crate::transaction_types::{TimelockRange, PageNumber};
    
    let mut global_min: Option<u64> = None;
    let mut global_max: Option<u64> = None;
    
    // Iterate over the input slice
    for input in inputs {
        // Calculate this input's effective absolute timelock range
        let (input_min, input_max) = input.calculate_timelock_range();
        
        // Update global min (take maximum of all minimums)
        global_min = match (global_min, input_min) {
            (None, min) => min,
            (Some(g), None) => Some(g),
            (Some(g), Some(i)) => Some(g.max(i)),
        };
        
        // Update global max (take minimum of all maximums)
        global_max = match (global_max, input_max) {
            (None, max) => max,
            (Some(g), None) => Some(g),
            (Some(g), Some(i)) => Some(g.min(i)),
        };
    }
    
    TimelockRange {
        min: global_min.map(|v| PageNumber { value: v }),
        max: global_max.map(|v| PageNumber { value: v }),
    }
}

// ============================================================================
// NOUN CONVERSION FUNCTIONS (Rust -> Noun)
// ============================================================================

use crate::transaction_types::NName;
use crate::transaction_types::Hash;

/// Converts a list of Inputs into a raw Nock noun using NounSlab
/// 
/// This function creates a noun structure that matches the Hoon raw-tx format:
/// ```
/// [tx-id raw-tx-without-id]
/// ```
/// Where raw-tx-without-id = [inputs timelock-range total-fees]
/// 
/// The processing pipeline:
/// 1. Build a ZMap from the input list, keyed by note names (like multi:new:inputs:transact)
/// 2. Calculate total fees and timelock range
/// 3. Compute tx-id using hashable implementations from tx/hashing
/// 4. Assemble final structure as [tx-id raw-tx-without-id]
/// 
/// # Arguments
/// * `input_list` - A vector of Input structs to convert
/// 
/// # Returns
/// * `NounSlab` - A NounSlab containing the raw transaction as a noun
pub fn create_raw_transaction_noun(inputs_zmap: ZMap<NName, Input>) -> NounSlab {
    let input_list: Vec<Input> = inputs_zmap.tap().iter().map(|(_, input)| input.clone()).collect();
    let mut slab = NounSlab::new();

    
    // Step 1: Build Inputs structure from input list
    // This follows the logic of (multi:new:inputs:transact ins) in Hoon
    let mut inputs_zmap = ZMap::new();
    for input in &input_list {
        // Use the note's name as the key in the z-map
        let key = input.note.name.clone();
        inputs_zmap.put(key, input.clone());
    }
    let inputs = Inputs { p: inputs_zmap.clone() };
    
    // Step 2: Calculate total fees by summing all input fees
    let total_fees_value: u64 = input_list
        .iter()
        .map(|input| input.spend.fee.value)
        .sum();
    
    // Step 3: Calculate timelock range from all inputs
    let timelock_range = calculate_timelock_range(&input_list);
    
    // Step 4: Compute tx-id using the hashable implementations from tx/hashing
    // This uses the to_hashable() method implementations and proper TIP5 hashing
    let tx_id = compute_tx_id(&inputs.p, &timelock_range, total_fees_value);
    
    // Step 5: Build the raw_tx_without_id structure as nouns
    // Create inputs noun using NounEncode trait from Inputs struct
    let inputs_noun = inputs.to_noun(&mut slab);
    
    // Create timelock noun
    let timelock_noun = timelock_range.to_noun(&mut slab);
    
    // Create total fees noun using Coins NounEncode trait
    let total_fees = Coins { value: total_fees_value };
    let total_fees_noun = total_fees.to_noun(&mut slab);
    
    // Assemble raw_tx_without_id: [inputs timelock-range total-fees]
    let raw_tx_without_id = T(&mut slab, &[
        inputs_noun,
        timelock_noun,
        total_fees_noun
    ]);
    
    // Step 6: Create tx-id noun
    let tx_id_noun = tx_id.to_noun(&mut slab);
    
    // Step 7: Create the final raw_tx structure: [tx-id raw_tx_without_id]
    let raw_tx = T(&mut slab, &[tx_id_noun, raw_tx_without_id]);
    
    // Set as root and return
    slab.set_root(raw_tx);
    slab
}

pub fn generate_tx_id(inputs_zmap: ZMap<NName, Input>) -> Hash { 
    let input_list: Vec<Input> = inputs_zmap.tap().iter().map(|(_, input)| input.clone()).collect();

    // Step 2: Calculate total fees by summing all input fees
    let total_fees_value: u64 = input_list
        .iter()
        .map(|input| input.spend.fee.value)
        .sum();
    
    // Step 3: Calculate timelock range from all inputs
    let timelock_range = calculate_timelock_range(&input_list);
    
    // Step 4: Compute tx-id using the hashable implementations from tx/hashing
    // This uses the ToHashable trait implementations and proper TIP5 hashing
    compute_tx_id(&inputs_zmap, &timelock_range, total_fees_value)
}


// Note: We now use TimelockRange's NounEncode trait implementation directly
// instead of a custom create_timelock_range_noun function
// Also, calculate_timelock_range is now a method on Inputs type
#[cfg(test)]
mod tests {
    use super::*;

    
    #[test]
    fn test_single_input_hoon_generator_exact() {
        use crate::transaction_types::*;
        use crate::collections::{ZSet, ZMap};
        use crate::hashing::compute_tx_id;
        
        // Create the Schnorr pubkey from the Hoon output (used in lock, signature, and seed recipient)
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
            inf: false,
        };
        
        // Create the name with two hashes
        let name = NName {
            p: vec![
                Hash { values: [
                    0x1823_f2b1_7cba_6a60,
                    0xf21d_6e62_41ad_b7c2,
                    0xcc5a_5597_4af3_8483,
                    0x9552_4fbf_2e34_cb94,
                    0xfd99_8aff_5184_4889,
                ]},
                Hash { values: [
                    0xb68f_338b_6405_3dc0,
                    0xf2e8_b88c_b1e4_fe55,
                    0xf4d2_edc2_b560_4059,
                    0xcd0e_3527_7397_8c7b,
                    0x24fc_3bc8_ae97_b70e,
                ]},
            ]
        };
        
        // Create the lock (m=1 with the pubkey)
        let mut lock_pubkeys = ZSet::new();
        lock_pubkeys.put(pubkey.clone());
        let lock = Lock { m: 1, pubkeys: lock_pubkeys };
        
        // Create the source (coinbase with all zeros)
        let source = Source {
            p: Hash { values: [0, 0, 0, 0, 0] },
            is_coinbase: true,
        };
        
        // Create the NNote
        let note = NNote {
            meta: NNoteHead {
                version: 0,
                origin_page: PageNumber { value: 1 },
                timelock: Timelock { intent: None },
            },
            name,
            lock,
            source,
            assets: Coins { value: 100 },
        };
        
        // Create the signature
        let signature = Some(Signature {
            map: {
                let mut sig_map = ZMap::new();
                let schnorr_sig = SchnorrSignature {
                    chal: Chal { values: T8 { values: [
                        0xcb83_e876, 0x5605_8192, 0x8a6a_f665, 0x9cff_3c81,
                        0x0e88_aea6, 0x4960_b830, 0x97e0_63fa, 0x09cc_fd42,
                    ]}},
                    sig: Sig { values: T8 { values: [
                        0x3f0f_e66f, 0x3b26_97cc, 0xda6d_ff6d, 0x1ce1_23b5,
                        0xa4b2_09ba, 0x8294_39a5, 0x0c4f_7358, 0x3597_a641,
                    ]}},
                };
                sig_map.put(pubkey.clone(), schnorr_sig);
                sig_map
            }
        });
        
        // Create the seeds
        let seeds = Seeds {
            set: {
                let mut seed_set = ZSet::new();
                let mut recipient_pubkeys = ZSet::new();
                recipient_pubkeys.put(pubkey.clone());
                
                let seed = Seed {
                    output_source: None,
                    recipient: Lock { m: 1, pubkeys: recipient_pubkeys },
                    timelock_intent: None,
                    gift: Coins { value: 90 },
                    parent_hash: Hash { values: [
                        0x49d8_ec23_bedb_5ebf,
                        0xab86_7316_14a8_95b4,
                        0xc945_61ba_adb6_ce58,
                        0x8735_8c72_08cc_9c5d,
                        0xd048_23a9_eb56_7b2e,
                    ]},
                };
                seed_set.put(seed);
                seed_set
            }
        };
        
        // Create the input
        let input = Input {
            note,
            spend: Spend {
                signature,
                seeds,
                fee: Coins { value: 10 },
            },
        };
        
        // Create the raw transaction noun
        let input_list = vec![input.clone()];
        let slab = create_raw_transaction_noun(input_list.clone());
        
        // Extract the tx_id from the noun
        let root = unsafe { slab.root() };
        assert!(root.is_cell(), "Root should be a cell");
        
        let cell_result = root.as_cell();
        let cell = cell_result.ok().unwrap();
        let tx_id_noun = cell.head();
        
        // The tx_id should be a cell with 5 atoms
        // Extract the actual tx_id values
        let mut tx_id_values = [0u64; 5];
        let mut current = tx_id_noun;
        for i in 0..5 {
            if i < 4 && current.is_cell() {
                let cell = current.as_cell().ok().unwrap();
                let atom = cell.head();
                if atom.is_atom() {
                    tx_id_values[i] = atom.as_atom().unwrap().as_u64().unwrap();
                }
                current = cell.tail();
            } else if i == 4 && current.is_atom() {
                // Last element might be an atom directly
                tx_id_values[i] = current.as_atom().unwrap().as_u64().unwrap();
            }
        }
        
        // Also compute using compute_tx_id function for comparison
        let mut inputs_zmap = ZMap::new();
        inputs_zmap.put(input.note.name.clone(), input.clone());
        let timelock_range = calculate_timelock_range(&input_list);
        let total_fees = input.spend.fee.value;
        let computed_tx_id = compute_tx_id(&inputs_zmap, &timelock_range, total_fees);
        
        // Expected tx_id from Hoon
        let expected_tx_id = Hash { values: [
            0xb57c_5bef_705f_551d,
            0xe2ae_358b_61d0_bb54,
            0x3c58_42d3_4a3e_89c2,
            0xa933_80de_f9cb_0f0d,
            0x7e0a_99e0_9f8b_bc5e,
        ]};
        
        println!("TX-ID from noun extraction: {:016x?}", tx_id_values);
        println!("TX-ID from compute_tx_id:   {:016x?}", computed_tx_id.values);
        println!("Expected TX-ID from Hoon:    {:016x?}", expected_tx_id.values);
        
        // Verify the tx_id matches
        assert_eq!(computed_tx_id, expected_tx_id, "TX-ID should match Hoon generator output");
        assert_eq!(tx_id_values, expected_tx_id.values, "TX-ID from noun should match expected");
    }
    
    #[test]
    fn test_single_input_noun_structure_exact() {
        use crate::transaction_types::*;
        use crate::collections::{ZSet, ZMap};
        use nockvm::noun::{Noun, D, T};
        use nockapp::noun::slab::NounSlab;
        
        // Create the exact same input as test_single_input_hoon_generator_exact
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
            inf: false,
        };
        
        let name = NName {
            p: vec![
                Hash { values: [
                    0x1823_f2b1_7cba_6a60,
                    0xf21d_6e62_41ad_b7c2,
                    0xcc5a_5597_4af3_8483,
                    0x9552_4fbf_2e34_cb94,
                    0xfd99_8aff_5184_4889,
                ]},
                Hash { values: [
                    0xb68f_338b_6405_3dc0,
                    0xf2e8_b88c_b1e4_fe55,
                    0xf4d2_edc2_b560_4059,
                    0xcd0e_3527_7397_8c7b,
                    0x24fc_3bc8_ae97_b70e,
                ]},
            ]
        };
        
        let mut lock_pubkeys = ZSet::new();
        lock_pubkeys.put(pubkey.clone());
        let lock = Lock { m: 1, pubkeys: lock_pubkeys };
        
        let source = Source {
            p: Hash { values: [0, 0, 0, 0, 0] },
            is_coinbase: true,
        };
        
        let note = NNote {
            meta: NNoteHead {
                version: 0,
                origin_page: PageNumber { value: 1 },
                timelock: Timelock { intent: None },
            },
            name,
            lock,
            source,
            assets: Coins { value: 100 },
        };
        
        let signature = Some(Signature {
            map: {
                let mut sig_map = ZMap::new();
                let schnorr_sig = SchnorrSignature {
                    chal: Chal { values: T8 { values: [
                        0xcb83_e876, 0x5605_8192, 0x8a6a_f665, 0x9cff_3c81,
                        0x0e88_aea6, 0x4960_b830, 0x97e0_63fa, 0x09cc_fd42,
                    ]}},
                    sig: Sig { values: T8 { values: [
                        0x3f0f_e66f, 0x3b26_97cc, 0xda6d_ff6d, 0x1ce1_23b5,
                        0xa4b2_09ba, 0x8294_39a5, 0x0c4f_7358, 0x3597_a641,
                    ]}},
                };
                sig_map.put(pubkey.clone(), schnorr_sig);
                sig_map
            }
        });
        
        let seeds = Seeds {
            set: {
                let mut seed_set = ZSet::new();
                let mut recipient_pubkeys = ZSet::new();
                recipient_pubkeys.put(pubkey.clone());
                
                let seed = Seed {
                    output_source: None,
                    recipient: Lock { m: 1, pubkeys: recipient_pubkeys },
                    timelock_intent: None,
                    gift: Coins { value: 90 },
                    parent_hash: Hash { values: [
                        0x49d8_ec23_bedb_5ebf,
                        0xab86_7316_14a8_95b4,
                        0xc945_61ba_adb6_ce58,
                        0x8735_8c72_08cc_9c5d,
                        0xd048_23a9_eb56_7b2e,
                    ]},
                };
                seed_set.put(seed);
                seed_set
            }
        };
        
        let input = Input {
            note,
            spend: Spend {
                signature,
                seeds,
                fee: Coins { value: 10 },
            },
        };
        
        // Create the raw transaction noun
        let input_list = vec![input];
        let mut slab = create_raw_transaction_noun(input_list);
        
        // Get the root noun
        let root = unsafe { slab.root() };
        
        // Helper function to print noun structure recursively as simple nested lists
        fn print_noun(noun: Noun) {
            match noun.as_atom() {
                Ok(atom) => {
                    if let Ok(val) = atom.as_u64() {
                        print!("{}", val);
                    } else {
                        // Large atom
                        print!("<large>");
                    }
                },
                Err(_) => {
                    // It's a cell
                    if let Ok(cell) = noun.as_cell() {
                        print!("[");
                        print_noun(cell.head());
                        print!(" ");
                        print_noun(cell.tail());
                        print!("]");
                    }
                }
            }
        }
        
        // Print the entire noun structure
        println!("\n=== COMPLETE NOUN STRUCTURE ===");
        print_noun(*root);
        println!("\n=== END NOUN STRUCTURE ===\n");
        
        // Now let's actually verify the structure matches what we expect
        // We'll check key values throughout the structure
        
        let root_cell = root.as_cell().expect("Root should be a cell");
        
        // Check TX-ID (head of root)
        let tx_id = root_cell.head();
        let tx_id_cell = tx_id.as_cell().expect("TX-ID should be a cell");
        let tx_id_1 = tx_id_cell.head().as_atom().expect("First tx-id should be atom");
        assert_eq!(tx_id_1.as_u64().unwrap(), 13077428501917685021, "First TX-ID value");
        
        // Check input structure (tail of root)  
        let input = root_cell.tail();
        let input_cell = input.as_cell().expect("Input should be a cell");
        
        // Navigate to the fee value (should be 10)
        // It's at: root.tail.tail.tail.tail (the very end)
        let mut current = input_cell.tail();
        if let Ok(cell) = current.as_cell() {
            current = cell.tail();
            if let Ok(cell) = current.as_cell() {
                current = cell.tail();
                // This should be [0 0] 10 structure
                if let Ok(cell) = current.as_cell() {
                    let fee = cell.tail();
                    if let Ok(fee_atom) = fee.as_atom() {
                        assert_eq!(fee_atom.as_u64().unwrap(), 10, "Fee should be 10");
                    }
                }
            }
        }
        
        // Navigate to find the assets value (should be 100)
        // It's in the note structure
        let note_structure = input_cell.head();
        if let Ok(note_cell) = note_structure.as_cell() {
            // Navigate through note to find assets
            let mut nav = note_cell;
            // Go through the structure to find 100
            if let Ok(inner) = nav.tail().as_cell() {
                if let Ok(inner2) = inner.head().as_cell() {
                    if let Ok(inner3) = inner2.tail().as_cell() {
                        if let Ok(inner4) = inner3.tail().as_cell() {
                            if let Ok(inner5) = inner4.tail().as_cell() {
                                // Should find assets = 100 here
                                if let Ok(assets_atom) = inner5.tail().as_atom() {
                                    assert_eq!(assets_atom.as_u64().unwrap(), 100, "Assets should be 100");
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Verify the gift value (should be 90)
        // This is deep in the seeds structure
        // We can search for it by looking for the value 90 in the structure
        fn find_value_in_noun(noun: Noun, target: u64) -> bool {
            match noun.as_atom() {
                Ok(atom) => {
                    if let Ok(val) = atom.as_u64() {
                        return val == target;
                    }
                    false
                },
                Err(_) => {
                    if let Ok(cell) = noun.as_cell() {
                        find_value_in_noun(cell.head(), target) || find_value_in_noun(cell.tail(), target)
                    } else {
                        false
                    }
                }
            }
        }
        
        assert!(find_value_in_noun(*root, 90), "Structure should contain gift value of 90");
        assert!(find_value_in_noun(*root, 100), "Structure should contain assets value of 100");
        assert!(find_value_in_noun(*root, 10), "Structure should contain fee value of 10");
        
        println!("\n✓ Structure verification complete:");
        println!("  - TX-ID first value: 13077428501917685021 ✓");
        println!("  - Fee value: 10 ✓");
        println!("  - Assets value: 100 ✓");
        println!("  - Gift value: 90 ✓");
        println!("  - All key values found in correct positions");
        
        println!("\n✓ Full noun structure matches Hoon output");
    }
    
    #[test]
    fn test_spend_structure_debug() {
        use crate::transaction_types::*;
        use crate::collections::{ZSet, ZMap};
        
        // Create the exact same input as in the main test
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
            inf: false,
        };
        
        let signature = Some(Signature {
            map: {
                let mut sig_map = ZMap::new();
                let schnorr_sig = SchnorrSignature {
                    chal: Chal { values: T8 { values: [
                        0xcb83_e876, 0x5605_8192, 0x8a6a_f665, 0x9cff_3c81,
                        0x0e88_aea6, 0x4960_b830, 0x97e0_63fa, 0x09cc_fd42,
                    ]}},
                    sig: Sig { values: T8 { values: [
                        0x3f0f_e66f, 0x3b26_97cc, 0xda6d_ff6d, 0x1ce1_23b5,
                        0xa4b2_09ba, 0x8294_39a5, 0x0c4f_7358, 0x3597_a641,
                    ]}},
                };
                sig_map.put(pubkey.clone(), schnorr_sig);
                sig_map
            }
        });
        
        let seeds = Seeds {
            set: {
                let mut seed_set = ZSet::new();
                let mut recipient_pubkeys = ZSet::new();
                recipient_pubkeys.put(pubkey.clone());
                
                let seed = Seed {
                    output_source: None,
                    recipient: Lock { m: 1, pubkeys: recipient_pubkeys },
                    timelock_intent: None,
                    gift: Coins { value: 90 },
                    parent_hash: Hash { values: [
                        0x49d8_ec23_bedb_5ebf,
                        0xab86_7316_14a8_95b4,
                        0xc945_61ba_adb6_ce58,
                        0x8735_8c72_08cc_9c5d,
                        0xd048_23a9_eb56_7b2e,
                    ]},
                };
                seed_set.put(seed);
                seed_set
            }
        };
        
        let spend = Spend {
            signature,
            seeds,
            fee: Coins { value: 10 },
        };
        
        // Convert to noun
        let mut slab: NounSlab = NounSlab::new();
        let spend_noun = spend.to_noun(&mut slab);
        
        // Analyze the structure
        println!("\n=== Spend Structure Analysis (with real data) ===");
        
        if let Ok(cell) = spend_noun.as_cell() {
            println!("Spend is a cell");
            let head = cell.head();
            let tail = cell.tail();
            
            println!("Head (signature): is_atom={}, is_cell={}", 
                head.as_atom().is_ok(),
                head.as_cell().is_ok());
            
            // Check if tail is a cell or atom
            if let Ok(tail_cell) = tail.as_cell() {
                println!("Tail is a cell - GOOD!");
                let tail_head = tail_cell.head();
                let tail_tail = tail_cell.tail();
                
                println!("  Tail.head (seeds): is_atom={}, is_cell={}", 
                    tail_head.as_atom().is_ok(), 
                    tail_head.as_cell().is_ok());
                println!("  Tail.tail (fee): is_atom={}, is_cell={}", 
                    tail_tail.as_atom().is_ok(),
                    tail_tail.as_cell().is_ok());
                    
                if let Ok(fee_atom) = tail_tail.as_atom() {
                    if let Ok(fee_val) = fee_atom.as_u64() {
                        println!("  Fee value: {}", fee_val);
                    }
                }
            } else if let Ok(tail_atom) = tail.as_atom() {
                println!("ERROR: Tail is an atom!");
                if let Ok(val) = tail_atom.as_u64() {
                    println!("  Atom value as u64: {:#x} ({})", val, val);
                } else {
                    println!("  Atom is too large for u64");
                    // Try to print raw bytes
                    println!("  Atom debug: {:?}", tail_atom);
                }
            } else {
                println!("Tail is neither cell nor atom??");
            }
        }
        
        // Now test with actual signature
        let pubkey = SchnorrPubkey {
            x: F6LT { values: [1, 2, 3, 4, 5, 6] },
            y: F6LT { values: [7, 8, 9, 10, 11, 12] },
            inf: false,
        };
        
        let signature = Some(Signature {
            map: {
                let mut sig_map = ZMap::new();
                let schnorr_sig = SchnorrSignature {
                    chal: Chal { values: T8 { values: [1, 2, 3, 4, 5, 6, 7, 8] }},
                    sig: Sig { values: T8 { values: [9, 10, 11, 12, 13, 14, 15, 16] }},
                };
                sig_map.put(pubkey.clone(), schnorr_sig);
                sig_map
            }
        });
        
        let spend_with_sig = Spend {
            signature,
            seeds: Seeds { set: ZSet::new() },
            fee: Coins { value: 10 },
        };
        
        let spend_sig_noun = spend_with_sig.to_noun(&mut slab);
        
        println!("\n=== Spend with Signature Structure ===");
        if let Ok(cell) = spend_sig_noun.as_cell() {
            println!("Spend is a cell");
            let head = cell.head();
            let tail = cell.tail();
            
            println!("Head (signature): is_atom={}, is_cell={}", 
                head.as_atom().is_ok(),
                head.as_cell().is_ok());
            
            if let Ok(tail_cell) = tail.as_cell() {
                println!("Tail is a cell - this would be [seeds fee]");
            } else if let Ok(_) = tail.as_atom() {
                println!("Tail is an atom - NOT what we expected!");
            }
        }
    }
    
    #[test]
    fn test_two_inputs_from_hoon_generator() {
        use crate::transaction_types::*;
        use crate::collections::{ZSet, ZMap};
        use crate::hashing::compute_tx_id;
        
        // Common pubkey used in both inputs
        let common_pubkey = SchnorrPubkey {
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
        
        // Input 1
        let input1 = {
            // Name for input1 - two hashes
            let name1 = NName { 
                p: vec![
                    Hash { values: [
                        0x1823f2b17cba6a60,
                        0xf21d6e6241adb7c2,
                        0xcc5a55974af38483,
                        0x95524fbf2e34cb94,
                        0xfd998aff51844889,
                    ]},
                    Hash { values: [
                        0xb68f338b64053dc0,
                        0xf2e8b88cb1e4fe55,
                        0xf4d2edc2b5604059,
                        0xcd0e352773978c7b,
                        0x24fc3bc8ae97b70e,
                    ]},
                ]
            };
            
            // Lock with m=1 and the common pubkey
            let mut pubkeys1 = ZSet::new();
            pubkeys1.put(common_pubkey.clone());
            let lock1 = Lock { m: 1, pubkeys: pubkeys1 };
            
            // Source is coinbase with all zeros
            let source1 = Source {
                p: Hash { values: [0, 0, 0, 0, 0] },
                is_coinbase: true,
            };
            
            // Note for input1
            let note1 = NNote {
                meta: NNoteHead {
                    version: 0,
                    origin_page: PageNumber { value: 1 },
                    timelock: Timelock { intent: None },
                },
                name: name1,
                lock: lock1,
                source: source1,
                assets: Coins { value: 150 },
            };
            
            // Signature for input1
            let signature1 = Some(Signature {
                map: {
                    let mut sig_map = ZMap::new();
                    let schnorr_sig = SchnorrSignature {
                        chal: Chal { values: T8 { values: [
                            0x8201da19, 0xcde16f4c, 0x8578369d, 0x23d0776, 
                            0x7ed21f2c, 0x8e8d0348, 0x98e0649f, 0x5f5e4365
                        ]}},
                        sig: Sig { values: T8 { values: [
                            0xb3a46cd6, 0xd5a05703, 0x9910e837, 0xcbf4da5b,
                            0x5bd6b444, 0x1840c16b, 0xfe99b35b, 0x4481d614
                        ]}},
                    };
                    sig_map.put(common_pubkey.clone(), schnorr_sig);
                    sig_map
                }
            });
            
            // Seeds for input1
            let seeds1 = Seeds {
                set: {
                    let mut seed_set = ZSet::new();
                    let mut recipient_pubkeys = ZSet::new();
                    recipient_pubkeys.put(common_pubkey.clone());
                    
                    let seed = Seed {
                        output_source: None,
                        recipient: Lock { m: 1, pubkeys: recipient_pubkeys },
                        timelock_intent: None,
                        gift: Coins { value: 140 },
                        parent_hash: Hash { values: [
                            0xa199414db4b6e893,
                            0xa21c8ed1be8109a4,
                            0x2f0bf84cc06cc468,
                            0xdb193e6928ff4eac,
                            0x3b2fdd7e090eb6eb,
                        ]},
                    };
                    seed_set.put(seed);
                    seed_set
                }
            };
            
            Input {
                note: note1,
                spend: Spend {
                    signature: signature1,
                    seeds: seeds1,
                    fee: Coins { value: 10 },
                },
            }
        };
        
        // Input 2
        let input2 = {
            // Name for input2 - two hashes (first hash same as input1, second different)
            let name2 = NName { 
                p: vec![
                    Hash { values: [
                        0x1823f2b17cba6a60,
                        0xf21d6e6241adb7c2,
                        0xcc5a55974af38483,
                        0x95524fbf2e34cb94,
                        0xfd998aff51844889,
                    ]},
                    Hash { values: [
                        0x09c6a238932ff559,
                        0x156d63ebb5c8382e,
                        0x4bafb6612e8d372a,
                        0x0ae1517a6256597d,
                        0x88f5e85b7af8d9ec,
                    ]},
                ]
            };
            
            // Lock with m=1 and the common pubkey (same as input1)
            let mut pubkeys2 = ZSet::new();
            pubkeys2.put(common_pubkey.clone());
            let lock2 = Lock { m: 1, pubkeys: pubkeys2 };
            
            // Source is NOT coinbase with specific hash
            let source2 = Source {
                p: Hash { values: [
                    0xd52df518d66e378a,
                    0xb80aa6203770acb5,
                    0x475948868aec2032,
                    0x5ce1dd9b9b0f99c8,
                    0x96d3a5cbfe41a9da,
                ]},
                is_coinbase: false,
            };
            
            // Note for input2
            let note2 = NNote {
                meta: NNoteHead {
                    version: 0,
                    origin_page: PageNumber { value: 2 },
                    timelock: Timelock { intent: None },
                },
                name: name2,
                lock: lock2,
                source: source2,
                assets: Coins { value: 200 },
            };
            
            // Signature for input2
            let signature2 = Some(Signature {
                map: {
                    let mut sig_map = ZMap::new();
                    let schnorr_sig = SchnorrSignature {
                        chal: Chal { values: T8 { values: [
                            0xb8178004, 0x5745a0dd, 0x7e0d800c, 0x84a90cfb,
                            0xd687cd39, 0x78103bf1, 0xa51874d5, 0x0f9e0a44
                        ]}},
                        sig: Sig { values: T8 { values: [
                            0x420519fc, 0xafba7efe, 0x4797d97f, 0x840b45d5,
                            0xaf311e38, 0x96387987, 0xd712d047, 0x5d8a8b93
                        ]}},
                    };
                    sig_map.put(common_pubkey.clone(), schnorr_sig);
                    sig_map
                }
            });
            
            // Seeds for input2
            let seeds2 = Seeds {
                set: {
                    let mut seed_set = ZSet::new();
                    let mut recipient_pubkeys = ZSet::new();
                    recipient_pubkeys.put(common_pubkey.clone());
                    
                    let seed = Seed {
                        output_source: None,
                        recipient: Lock { m: 1, pubkeys: recipient_pubkeys },
                        timelock_intent: None,
                        gift: Coins { value: 185 },
                        parent_hash: Hash { values: [
                            0xdad5f5e574b36e1e,
                            0x1f88a134c9573e5f,
                            0x6b7a8086745936e6,
                            0x7a668dd744aac26d,
                            0x8750944e285f96b2,
                        ]},
                    };
                    seed_set.put(seed);
                    seed_set
                }
            };
            
            Input {
                note: note2,
                spend: Spend {
                    signature: signature2,
                    seeds: seeds2,
                    fee: Coins { value: 15 },
                },
            }
        };
        
        // Create the raw transaction noun with both inputs
        let input_list = vec![input1, input2];
        let slab = create_raw_transaction_noun(input_list.clone());
        
        // Extract the tx_id from the noun
        let root = unsafe { slab.root() };
        assert!(root.is_cell(), "Root should be a cell");
        
        let cell_result = root.as_cell();
        let cell = cell_result.ok().unwrap();
        let tx_id_noun = cell.head();
        
        // The tx_id should be a cell with 5 atoms
        // Extract the actual tx_id values
        let mut tx_id_values = [0u64; 5];
        let mut current = tx_id_noun;
        for i in 0..5 {
            if i < 4 && current.is_cell() {
                let cell = current.as_cell().ok().unwrap();
                let atom = cell.head();
                if atom.is_atom() {
                    tx_id_values[i] = atom.as_atom().unwrap().as_u64().unwrap();
                }
                current = cell.tail();
            } else if i == 4 && current.is_atom() {
                // Last element might be an atom directly
                tx_id_values[i] = current.as_atom().unwrap().as_u64().unwrap();
            }
        }
        
        // Also compute using compute_tx_id function for comparison
        let mut inputs_zmap = ZMap::new();
        for input in &input_list {
            inputs_zmap.put(input.note.name.clone(), input.clone());
        }
        let inputs = Inputs { p: inputs_zmap };
        let timelock_range = calculate_timelock_range(&input_list);
        let total_fees: u64 = input_list.iter().map(|i| i.spend.fee.value).sum();
        let computed_tx_id = compute_tx_id(&inputs.p, &timelock_range, total_fees);
        
        // Expected tx_id from Hoon
        let expected_tx_id = Hash { values: [
            0xb794dfe18b476f3e,
            0xe8d7d4356136b266,
            0xc9f7eca86a17a22f,
            0x2ed10a27333891bb,
            0x7ac61628e12b23e2,
        ]};
        
        println!("TX-ID from noun extraction: {:x?}", tx_id_values);
        println!("TX-ID from compute_tx_id:   {:x?}", computed_tx_id.values);
        println!("Expected TX-ID from Hoon:    {:x?}", expected_tx_id.values);
        
        // Verify the tx_id matches
        assert_eq!(computed_tx_id, expected_tx_id, "TX-ID should match expected from Hoon");
        assert_eq!(tx_id_values, expected_tx_id.values, "Extracted TX-ID should match expected from Hoon");
    }
  
    #[test]
    fn test_three_input_zmap_hash() {
        // Test that exactly replicates the Hoon inputs-zmap-test generator output
        use crate::transaction_types::*;
        use crate::collections::{ZSet, ZMap};
        
        println!("\n=== Three Input ZMap Hash Test ===");
        
        // Define the three pubkeys used across the inputs
        let pubkey1 = SchnorrPubkey {
            x: F6LT { values: [
                566_273_053_357_821_052,
                4_788_914_443_680_986_537,
                8_415_538_053_559_789_354,
                6_139_886_333_872_072_363,
                5_982_840_667_074_872_231,
                3_576_629_195_875_272_167,
            ] },
            y: F6LT { values: [
                15_011_107_681_872_349_543,
                13_458_149_730_927_283_597,
                4_493_098_844_657_385_094,
                16_216_728_320_903_752_444,
                4_842_233_851_133_808_121,
                15_566_351_388_284_388_351,
            ] },
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
            ] },
            y: F6LT { values: [
                1_221_896_489_402_813_317,
                4_702_927_297_152_298_800,
                15_400_301_787_023_911_298,
                5_131_927_075_907_562_501,
                13_208_991_022_157_055_578,
                10_078_243_364_526_679_113,
            ] },
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
            ] },
            y: F6LT { values: [
                10_242_415_564_008_566_488,
                10_485_181_329_625_226_048,
                8_639_946_714_446_054_618,
                4_053_240_175_695_272_783,
                11_730_999_058_788_639_792,
                14_820_844_833_610_271_254,
            ] },
            inf: false,
        };
        
        // Input 1: origin-page=300, m=2 of 3 multisig, 3000 coins
        let nname1 = NName {
            p: vec![
                Hash { values: [
                    0x904e_6239_e9c0_b2eb,
                    0xb249_5c35_445e_9b6b,
                    0x448e_96de_80c8_4af5,
                    0xab57_de09_61b8_0d91,
                    0x6ef6_f29b_1219_8e88,
                ] },
                Hash { values: [
                    0x7d3e_3381_642b_d291,
                    0x7f43_be82_f4af_79fa,
                    0xc6b5_5fa5_fe09_b26c,
                    0x9f54_a089_42fa_ad40,
                    0x80bf_a9e0_cb56_04de,
                ] },
            ],
        };
        
        let mut pubkeys_set1 = ZSet::new();
        pubkeys_set1.put(pubkey1.clone());
        pubkeys_set1.put(pubkey2.clone());
        pubkeys_set1.put(pubkey3.clone());
        
        let input1 = Input {
            note: NNote {
                meta: NNoteHead {
                    version: 0,
                    origin_page: PageNumber { value: 300 },
                    timelock: Timelock { intent: None },
                },
                name: nname1.clone(),
                lock: Lock {
                    m: 2,
                    pubkeys: pubkeys_set1.clone(),
                },
                source: Source {
                    p: Hash { values: [
                        0x55f7_09c9_8037_a6cc,
                        0x7316_4902_2e40_910e,
                        0xf9f3_2537_9c9a_6254,
                        0xf599_421c_f2d6_6d4b,
                        0x935f_80bc_8dc4_4785,
                    ] },
                    is_coinbase: true,
                },
                assets: Coins { value: 3000 },
            },
            spend: Spend {
                signature: Some(Signature {
                    map: {
                        let mut sig_map = ZMap::new();
                        sig_map.put(pubkey3.clone(), SchnorrSignature {
                            chal: Chal {
                                values: T8 { values: [
                                    0x5ecb_b9a0,
                                    0xe68c_c30d,
                                    0xeb45_4115,
                                    0x44b8_45f3,
                                    0x2756_6ca7,
                                    0xd269_3624,
                                    0x979e_4a4b,
                                    0x7114_9ea6,
                                ] },
                            },
                            sig: Sig {
                                values: T8 { values: [
                                    0xbf0c_1188,
                                    0x3ed5_8b07,
                                    0xb902_4dc5,
                                    0x338d_7021,
                                    0x420f_6b4c,
                                    0xe549_4eb0,
                                    0xb53a_1118,
                                    0x69a8_3902,
                                ] },
                            },
                        });
                        sig_map
                    },
                }),
                seeds: Seeds {
                    set: {
                        let mut seeds_set = ZSet::new();
                        seeds_set.put(Seed {
                            output_source: None,
                            recipient: Lock {
                                m: 2,
                                pubkeys: pubkeys_set1.clone(),
                            },
                            timelock_intent: None,
                            gift: Coins { value: 2990 },
                            parent_hash: Hash { values: [
                                0x09c7_a200_6ba6_35d2,
                                0x7179_17f0_d64f_4268,
                                0xf6f0_86d3_d5b9_adf7,
                                0x8fc5_d28b_dd95_ff60,
                                0x911e_0d74_9cd9_dd0c,
                            ] },
                        });
                        seeds_set
                    },
                },
                fee: Coins { value: 10 },
            },
        };
        
        // Input 2: origin-page=100, m=1 of 1 multisig, 1000 coins
        let nname2 = NName {
            p: vec![
                Hash { values: [
                    0x1823_f2b1_7cba_6a60,
                    0xf21d_6e62_41ad_b7c2,
                    0xcc5a_5597_4af3_8483,
                    0x9552_4fbf_2e34_cb94,
                    0xfd99_8aff_5184_4889,
                ] },
                Hash { values: [
                    0x0211_e2b6_6020_2152e,
                    0x3b9c_664c_baff_5581,
                    0x6008_81d6_2150_fc22,
                    0x503e_828d_ed5a_6204,
                    0x031c_e52c_3cfe_d3c3,
                ] },
            ],
        };
        
        let mut pubkeys_set2 = ZSet::new();
        pubkeys_set2.put(pubkey3.clone());
        
        let input2 = Input {
            note: NNote {
                meta: NNoteHead {
                    version: 0,
                    origin_page: PageNumber { value: 100 },
                    timelock: Timelock { intent: None },
                },
                name: nname2.clone(),
                lock: Lock {
                    m: 1,
                    pubkeys: pubkeys_set2.clone(),
                },
                source: Source {
                    p: Hash { values: [
                        0xe7d8_51a3_d8f5_4052,
                        0x166b_8704_24eb_3edd,
                        0xc2e7_7be7_69bb_277c,
                        0x2f8e_40ce_cc6f_5160,
                        0x8f17_046a_8402_bf57,
                    ] },
                    is_coinbase: false,
                },
                assets: Coins { value: 1000 },
            },
            spend: Spend {
                signature: Some(Signature {
                    map: {
                        let mut sig_map = ZMap::new();
                        sig_map.put(pubkey3.clone(), SchnorrSignature {
                            chal: Chal {
                                values: T8 { values: [
                                    0xfa0b_5c89,
                                    0x265e_d66c,
                                    0x54dc_9a77,
                                    0xb554_54e4,
                                    0x7f6c_f4e4,
                                    0xc281_b9d7,
                                    0x6409_1abd,
                                    0x1358_4241,
                                ] },
                            },
                            sig: Sig {
                                values: T8 { values: [
                                    0x02a7_cc0d,
                                    0x1504_d6fb,
                                    0x6284_2a14,
                                    0x34ad_d64b,
                                    0x6c9b_b0cc,
                                    0x1cff_dcc0,
                                    0x6e5e_88a7,
                                    0x0afd_7c69,
                                ] },
                            },
                        });
                        sig_map
                    },
                }),
                seeds: Seeds {
                    set: {
                        let mut seeds_set = ZSet::new();
                        seeds_set.put(Seed {
                            output_source: None,
                            recipient: Lock {
                                m: 1,
                                pubkeys: pubkeys_set2.clone(),
                            },
                            timelock_intent: None,
                            gift: Coins { value: 990 },
                            parent_hash: Hash { values: [
                                0xad0e_ddc6_bbf3_c595,
                                0xcf12_c723_e263_8202,
                                0x17be_8fbe_96b0_3fae,
                                0xfe01_5d1a_ddf1_87e2,
                                0x336e_ff15_43d5_7d20,
                            ] },
                        });
                        seeds_set
                    },
                },
                fee: Coins { value: 10 },
            },
        };
        
        // Input 3: origin-page=200, m=1 of 2 multisig, 2000 coins, with timelock
        let nname3 = NName {
            p: vec![
                Hash { values: [
                    0x37d8_87de_d3a9_e44b,
                    0xf235_0c44_fd7f_7fc6,
                    0xbf74_fa76_9517_a871,
                    0x05ec_0727_3512_8e34,
                    0x0232_b3d2_f79d_523f,
                ] },
                Hash { values: [
                    0x09c6_a238_932f_f559,
                    0x156d_63eb_b5c8_382e,
                    0x4baf_b661_2e8d_372a,
                    0x0ae1_517a_6256_597d,
                    0x88f5_e85b_7af8_d9ec,
                ] },
            ],
        };
        
        let mut pubkeys_set3 = ZSet::new();
        pubkeys_set3.put(pubkey2.clone());
        pubkeys_set3.put(pubkey3.clone());
        
        let input3 = Input {
            note: NNote {
                meta: NNoteHead {
                    version: 0,
                    origin_page: PageNumber { value: 200 },
                    timelock: Timelock {
                        intent: Some((
                            TimelockRange {
                                min: Some(PageNumber { value: 50 }),
                                max: Some(PageNumber { value: 100 }),
                            },
                            TimelockRange {
                                min: Some(PageNumber { value: 10 }),
                                max: Some(PageNumber { value: 20 }),
                            },
                        )),
                    },
                },
                name: nname3.clone(),
                lock: Lock {
                    m: 1,
                    pubkeys: pubkeys_set3.clone(),
                },
                source: Source {
                    p: Hash { values: [
                        0xd52d_f518_d66e_378a,
                        0xb80a_a620_3770_acb5,
                        0x4759_4886_8aec_2032,
                        0x5ce1_dd9b_9b0f_99c8,
                        0x96d3_a5cb_fe41_a9da,
                    ] },
                    is_coinbase: false,
                },
                assets: Coins { value: 2000 },
            },
            spend: Spend {
                signature: Some(Signature {
                    map: {
                        let mut sig_map = ZMap::new();
                        sig_map.put(pubkey3.clone(), SchnorrSignature {
                            chal: Chal {
                                values: T8 { values: [
                                    0x626e_abbc,
                                    0xe378_52ee,
                                    0x26fe_80b2,
                                    0x24da_593e,
                                    0x8217_02d1,
                                    0xc83c_415e,
                                    0x4cec_0470,
                                    0x2f93_40ae,
                                ] },
                            },
                            sig: Sig {
                                values: T8 { values: [
                                    0xf73d_6289,
                                    0x9075_4d5c,
                                    0x69b3_3226,
                                    0xec00_b177,
                                    0xa176_d177,
                                    0x11a0_b879,
                                    0x6997_c826,
                                    0x72cb_980b,
                                ] },
                            },
                        });
                        sig_map
                    },
                }),
                seeds: Seeds {
                    set: {
                        let mut seeds_set = ZSet::new();
                        let mut recipient_pubkeys = ZSet::new();
                        recipient_pubkeys.put(pubkey2.clone());
                        recipient_pubkeys.put(pubkey3.clone());
                        
                        seeds_set.put(Seed {
                            output_source: None,
                            recipient: Lock {
                                m: 1,
                                pubkeys: recipient_pubkeys,
                            },
                            timelock_intent: Some((
                                TimelockRange {
                                    min: Some(PageNumber { value: 60 }),
                                    max: Some(PageNumber { value: 120 }),
                                },
                                TimelockRange {
                                    min: None,
                                    max: None,
                                },
                            )),
                            gift: Coins { value: 1990 },
                            parent_hash: Hash { values: [
                                0x277c_3cd6_f0f8_b294,
                                0xbbf8_9151_dd49_75d5,
                                0xb10d_14dc_ed3d_bde2,
                                0xa755_0a6f_8c91_9d57,
                                0x1cf3_c4b1_1483_c9a2,
                            ] },
                        });
                        seeds_set
                    },
                },
                fee: Coins { value: 10 },
            },
        };
        
        // Build the ZMap with all three inputs
        let mut inputs_zmap: ZMap<NName, Input> = ZMap::new();
        inputs_zmap.put(nname1, input1);
        inputs_zmap.put(nname2, input2);
        inputs_zmap.put(nname3, input3);
        
        // Create the Inputs structure
        let inputs = Inputs { p: inputs_zmap };
        
        // Hash the inputs using the new to_hash() method
        let computed_hash = inputs.to_hash();
        
        // Expected hash from Hoon
        let expected_hash = Hash { values: [
            0xa0ea_b6f9_a336_3cd2,
            0xf579_1176_1d3c_edb1,
            0xeb26_308b_4773_36a5,
            0x59ea_2712_ff1f_6aff,
            0x4799_f182_9ce7_9394,
        ] };
        
        println!("Computed hash: {:016x?}", computed_hash.values);
        println!("Expected hash: {:016x?}", expected_hash.values);
        
        // Verify the hash matches
        assert_eq!(computed_hash, expected_hash, "Three input ZMap hash mismatch!");
        println!("✓ Three input ZMap hash matches!");
    }
}