/// Tests for calculate_timelock_range function
/// Based on Hoon's roll-timelocks:inputs test cases

#[cfg(test)]
mod tests {
    use crate::transaction_types::*;
    use crate::collections::ZSet;
    use crate::tx_to_noun::calculate_timelock_range;

    // Helper function to create a test pubkey
    fn test_pubkey() -> SchnorrPubkey {
        SchnorrPubkey {
            x: F6LT { values: [
                9323455886065152710,
                8604621052628066076,
                8724446291889705637,
                15913798201200938686,
                6871293856171770838,
                11532431931696133539,
            ] },
            y: F6LT { values: [
                10242415564008566488,
                10485181329625226048,
                8639946714446054618,
                4053240175695272783,
                11730999058788639792,
                14820844833610271254,
            ] },
            inf: false,
        }
    }

    // Helper function to create a test lock
    fn test_lock() -> Lock {
        let mut pubkeys = ZSet::new();
        pubkeys.put(test_pubkey());
        Lock { m: 1, pubkeys }
    }

    // Helper function to create test inputs
    fn create_test_inputs(inputs_data: Vec<(u64, Option<TimelockRange>, Option<TimelockRange>)>) -> Vec<Input> {
        let mut inputs = Vec::new();
        
        for (i, (origin_page, absolute, relative)) in inputs_data.into_iter().enumerate() {
            // Create unique name for each input
            let name = NName {
                p: vec![
                    Hash { values: [0x1823f2b17cba6a60, 0xf21d6e6241adb7c2, 0xcc5a55974af38483, 0x95524fbf2e34cb94, 0xfd998aff51844889] },
                    Hash { values: [i as u64, 0, 0, 0, 0] },  // Unique second hash
                ],
            };
            
            let note = NNote {
                meta: NNoteHead {
                    version: 0,
                    origin_page: PageNumber { value: origin_page },
                    timelock: Timelock {
                        intent: match (absolute, relative) {
                            (None, None) => None,
                            (abs, rel) => Some((
                                abs.unwrap_or(TimelockRange { min: None, max: None }),
                                rel.unwrap_or(TimelockRange { min: None, max: None }),
                            )),
                        },
                    },
                },
                name: name.clone(),
                lock: test_lock(),
                source: Source {
                    p: Hash { values: [0, 0, 0, 0, 0] },
                    is_coinbase: false,
                },
                assets: Coins { value: 100 },
            };
            
            let input = Input {
                note,
                spend: Spend {
                    signature: None,
                    seeds: Seeds { set: ZSet::new() },
                    fee: Coins { value: 10 },
                },
            };
            
            inputs.push(input);
        }
        
        inputs
    }

    #[test]
    fn test1_seven_inputs() {
        // Test 1: Seven inputs with various timelock combinations
        // Expected result: [min=[~ 50] max=[~ 58]]
        
        let inputs_data = vec![
            // Input 1: origin-page=25, absolute=[min=[~ 45] max=[~ 65]], relative=empty
            (25, Some(TimelockRange { 
                min: Some(PageNumber { value: 45 }), 
                max: Some(PageNumber { value: 65 }) 
            }), None),
            
            // Input 2: origin-page=20, absolute=[min=[~ 48] max=~], relative=[min=~ max=[~ 40]]
            (20, Some(TimelockRange { 
                min: Some(PageNumber { value: 48 }), 
                max: None 
            }), Some(TimelockRange { 
                min: None, 
                max: Some(PageNumber { value: 40 }) 
            })),
            
            // Input 3: origin-page=20, absolute=[min=[~ 50] max=~], relative=empty
            (20, Some(TimelockRange { 
                min: Some(PageNumber { value: 50 }), 
                max: None 
            }), None),
            
            // Input 4: origin-page=28, absolute=[min=~ max=[~ 58]], relative=[min=[~ 20] max=~]
            (28, Some(TimelockRange { 
                min: None, 
                max: Some(PageNumber { value: 58 }) 
            }), Some(TimelockRange { 
                min: Some(PageNumber { value: 20 }), 
                max: None 
            })),
            
            // Input 5: origin-page=10, no timelock
            (10, None, None),
            
            // Input 6: origin-page=35, relative=[min=[~ 15] max=[~ 30]]
            (35, None, Some(TimelockRange { 
                min: Some(PageNumber { value: 15 }), 
                max: Some(PageNumber { value: 30 }) 
            })),
            
            // Input 7: origin-page=15, relative=[min=[~ 30] max=~]
            (15, None, Some(TimelockRange { 
                min: Some(PageNumber { value: 30 }), 
                max: None 
            })),
        ];
        
        let inputs = create_test_inputs(inputs_data);
        let result = calculate_timelock_range(&inputs);
        
        assert_eq!(result.min, Some(PageNumber { value: 50 }));
        assert_eq!(result.max, Some(PageNumber { value: 58 }));
    }

    #[test]
    fn test2_single_no_timelock() {
        // Test 2: Single input with no timelock
        // Expected result: [min=~ max=~]
        
        let inputs_data = vec![
            // Input: origin-page=100, no timelock
            (100, None, None),
        ];
        
        let inputs = create_test_inputs(inputs_data);
        let result = calculate_timelock_range(&inputs);
        
        assert_eq!(result.min, None);
        assert_eq!(result.max, None);
    }

    #[test]
    fn test3_two_absolute() {
        // Test 3: Two inputs with absolute timelocks
        // Expected result: [min=[~ 40] max=[~ 70]]
        
        let inputs_data = vec![
            // Input 1: origin-page=55, absolute=[min=[~ 40] max=[~ 80]]
            (55, Some(TimelockRange { 
                min: Some(PageNumber { value: 40 }), 
                max: Some(PageNumber { value: 80 }) 
            }), None),
            
            // Input 2: origin-page=50, absolute=[min=[~ 30] max=[~ 70]]
            (50, Some(TimelockRange { 
                min: Some(PageNumber { value: 30 }), 
                max: Some(PageNumber { value: 70 }) 
            }), None),
        ];
        
        let inputs = create_test_inputs(inputs_data);
        let result = calculate_timelock_range(&inputs);
        
        assert_eq!(result.min, Some(PageNumber { value: 40 }));
        assert_eq!(result.max, Some(PageNumber { value: 70 }));
    }

    #[test]
    fn test4_two_relative() {
        // Test 4: Two inputs with relative timelocks
        // Expected result: [min=[~ 40] max=[~ 70]]
        
        let inputs_data = vec![
            // Input 1: origin-page=25, relative=[min=[~ 15] max=[~ 45]]
            (25, None, Some(TimelockRange { 
                min: Some(PageNumber { value: 15 }), 
                max: Some(PageNumber { value: 45 }) 
            })),
            
            // Input 2: origin-page=20, relative=[min=[~ 10] max=[~ 50]]
            (20, None, Some(TimelockRange { 
                min: Some(PageNumber { value: 10 }), 
                max: Some(PageNumber { value: 50 }) 
            })),
        ];
        
        let inputs = create_test_inputs(inputs_data);
        let result = calculate_timelock_range(&inputs);
        
        // Input 1: absolute becomes [40, 70] (25+15, 25+45)
        // Input 2: absolute becomes [30, 70] (20+10, 20+50)
        // Intersection: [40, 70]
        assert_eq!(result.min, Some(PageNumber { value: 40 }));
        assert_eq!(result.max, Some(PageNumber { value: 70 }));
    }

    #[test]
    fn test5_mixed_abs_rel() {
        // Test 5: Mixed absolute and relative timelocks
        // Expected result: [min=[~ 43] max=[~ 45]]
        
        let inputs_data = vec![
            // Input 1: origin-page=18, absolute=[min=~ max=[~ 60]], relative=[min=[~ 25] max=~]
            (18, Some(TimelockRange { 
                min: None, 
                max: Some(PageNumber { value: 60 }) 
            }), Some(TimelockRange { 
                min: Some(PageNumber { value: 25 }), 
                max: None 
            })),
            
            // Input 2: origin-page=15, absolute=[min=[~ 35] max=~], relative=[min=~ max=[~ 30]]
            (15, Some(TimelockRange { 
                min: Some(PageNumber { value: 35 }), 
                max: None 
            }), Some(TimelockRange { 
                min: None, 
                max: Some(PageNumber { value: 30 }) 
            })),
        ];
        
        let inputs = create_test_inputs(inputs_data);
        let result = calculate_timelock_range(&inputs);
        
        // Input 1: relative becomes [43, ∞] (18+25, ∞), merged with absolute [∞, 60] = [43, 60]
        // Input 2: relative becomes [∞, 45] (∞, 15+30), merged with absolute [35, ∞] = [35, 45]
        // Intersection: [43, 45]
        assert_eq!(result.min, Some(PageNumber { value: 43 }));
        assert_eq!(result.max, Some(PageNumber { value: 45 }));
    }

    #[test]
    fn test6_both_abs_and_rel() {
        // Test 6: Both absolute and relative constraints
        // Expected result: [min=[~ 40] max=[~ 70]]
        
        let inputs_data = vec![
            // Input 1: origin-page=30, absolute=[min=[~ 38] max=[~ 75]], relative=[min=[~ 10] max=[~ 40]]
            (30, Some(TimelockRange { 
                min: Some(PageNumber { value: 38 }), 
                max: Some(PageNumber { value: 75 }) 
            }), Some(TimelockRange { 
                min: Some(PageNumber { value: 10 }), 
                max: Some(PageNumber { value: 40 }) 
            })),
            
            // Input 2: origin-page=35, absolute=[min=[~ 40] max=[~ 70]], relative=[min=[~ 5] max=[~ 35]]
            (35, Some(TimelockRange { 
                min: Some(PageNumber { value: 40 }), 
                max: Some(PageNumber { value: 70 }) 
            }), Some(TimelockRange { 
                min: Some(PageNumber { value: 5 }), 
                max: Some(PageNumber { value: 35 }) 
            })),
        ];
        
        let inputs = create_test_inputs(inputs_data);
        let result = calculate_timelock_range(&inputs);
        
        // Input 1: relative becomes [40, 70] (30+10, 30+40), merged with absolute [38, 75] = [40, 70]
        // Input 2: relative becomes [40, 70] (35+5, 35+35), merged with absolute [40, 70] = [40, 70]
        // Intersection: [40, 70]
        assert_eq!(result.min, Some(PageNumber { value: 40 }));
        assert_eq!(result.max, Some(PageNumber { value: 70 }));
    }

    #[test]
    fn test7_complex_interaction() {
        // Additional test: Complex interaction with overlapping ranges
        
        let inputs_data = vec![
            // Input 1: Tight absolute range
            (10, Some(TimelockRange { 
                min: Some(PageNumber { value: 100 }), 
                max: Some(PageNumber { value: 110 }) 
            }), None),
            
            // Input 2: Wide relative range
            (50, None, Some(TimelockRange { 
                min: Some(PageNumber { value: 45 }), 
                max: Some(PageNumber { value: 65 }) 
            })),
            
            // Input 3: Mixed with overlap
            (20, Some(TimelockRange { 
                min: Some(PageNumber { value: 90 }), 
                max: Some(PageNumber { value: 120 }) 
            }), Some(TimelockRange { 
                min: Some(PageNumber { value: 75 }), 
                max: Some(PageNumber { value: 95 }) 
            })),
        ];
        
        let inputs = create_test_inputs(inputs_data);
        let result = calculate_timelock_range(&inputs);
        
        // Input 1: [100, 110]
        // Input 2: [95, 115] (50+45, 50+65)
        // Input 3: relative [95, 115] (20+75, 20+95), merged with absolute [90, 120] = [95, 115]
        // Intersection: [100, 110]
        assert_eq!(result.min, Some(PageNumber { value: 100 }));
        assert_eq!(result.max, Some(PageNumber { value: 110 }));
    }
}