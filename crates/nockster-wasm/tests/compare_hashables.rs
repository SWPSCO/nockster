//! Compare WASM hashable construction with reference implementation

use nockapp::noun::slab::NounSlab;
use nockapp::Bytes;
use noun_serde::{NounDecode, NounEncode};
use std::fs;
use tx_types::hashing::hasher::hash_hashable;
use tx_types::transaction_types::*;
use tx_types::transaction_types_v0::*;

#[test]
fn test_pubkey_hash_comparison() {
    println!("\n=== Testing Pubkey Hash ===");

    // The pubkey from demo-tx
    let pk = SchnorrPubkey {
        x: F6LT {
            values: [
                1213264621707318396,
                7644592116046038696,
                12750713645667184650,
                4785470970688526859,
                14650880413807991875,
                12274556524416646944,
            ],
        },
        y: F6LT {
            values: [
                18177236637613408617,
                10958279360383408893,
                1240025389805216209,
                14139010256592505920,
                18119211718294268888,
                6152380099229918899,
            ],
        },
        inf: false,
    };

    // Reference implementation (uses NounSlab)
    let mut slab: NounSlab = NounSlab::new();
    let pk_noun = pk.to_noun(&mut slab);
    let reference_hash = tx_types::hashing::tip5::Tip5Hasher::hash_noun_varlen(pk_noun).unwrap();

    println!(
        "Reference pubkey hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        reference_hash.values[0],
        reference_hash.values[1],
        reference_hash.values[2],
        reference_hash.values[3],
        reference_hash.values[4]
    );

    println!("Expected:              065e981ee38e23e9_3cb4c3764b81b8f1_abb22883f963bef7_a8ee11acc9f9527f_177577e778eb4145");
}

#[test]
fn test_seed_to_sig_hashable_comparison() {
    println!("\n=== Testing Seed to_sig_hashable ===");

    // Load demo-tx.draft
    let draft_bytes = fs::read("../../demo-tx.draft").expect("demo-tx.draft not found");
    let mut slab: NounSlab = NounSlab::new();
    let noun = slab.cue_into(Bytes::from(draft_bytes)).expect("cue failed");

    let tx = Transaction::from_noun(&noun).expect("parse failed");
    // Transaction.p is Inputs enum, extract V0 variant
    let inputs_v0 = match &tx.p {
        Inputs::V0(v0) => v0.clone(),
        Inputs::V1(_) => panic!("Expected V0 inputs"),
    };
    let raw = RawTransactionV0 {
        id: Hash { values: [0; 5] },
        inputs: inputs_v0,
        timelock_range: TimelockRange {
            min: None,
            max: None,
        },
        total_fees: Coins { value: 1 },
    };

    // Get the first input
    let all_inputs: Vec<_> = raw.inputs.p.tap();
    let (_name, input) = all_inputs.first().expect("no inputs");

    let mut spend = input.spend.clone();
    spend.signature = None;

    println!("Fee: {}", spend.fee.value);
    println!("Number of seeds: {}", spend.seeds.set.wyt());

    // Get all seeds
    let seeds_vec: Vec<_> = spend.seeds.set.tap();
    for (i, seed) in seeds_vec.iter().enumerate() {
        println!("\nSeed {}: gift={}", i, seed.gift.value);

        // Test individual seed to_sig_hashable
        let seed_hashable = seed.to_sig_hashable();
        let seed_hash = hash_hashable(&seed_hashable);
        println!(
            "  Seed {} hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
            i,
            seed_hash.values[0],
            seed_hash.values[1],
            seed_hash.values[2],
            seed_hash.values[3],
            seed_hash.values[4]
        );

        // Test recipient (Lock) hashable
        let lock_hashable = seed.recipient.to_hashable();
        let lock_hash = hash_hashable(&lock_hashable);
        println!(
            "  Lock hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
            lock_hash.values[0],
            lock_hash.values[1],
            lock_hash.values[2],
            lock_hash.values[3],
            lock_hash.values[4]
        );

        // Test each pubkey in the lock
        for (j, pk) in seed.recipient.pubkeys.iter().enumerate() {
            let mut pk_slab: NounSlab = NounSlab::new();
            let pk_noun = pk.to_noun(&mut pk_slab);
            let pk_hash = tx_types::hashing::tip5::Tip5Hasher::hash_noun_varlen(pk_noun).unwrap();
            println!(
                "    Pubkey {} hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                j,
                pk_hash.values[0],
                pk_hash.values[1],
                pk_hash.values[2],
                pk_hash.values[3],
                pk_hash.values[4]
            );
        }
    }

    // Test the full seeds structure
    let seeds_hashable = spend.seeds.to_sig_hashable();
    let seeds_hash = hash_hashable(&seeds_hashable);
    println!(
        "\nFull seeds hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        seeds_hash.values[0],
        seeds_hash.values[1],
        seeds_hash.values[2],
        seeds_hash.values[3],
        seeds_hash.values[4]
    );
    println!("Expected:        83ddf986733eb15d_7375c7290302c040_387571418b5974ce_f45d0966d8fd4890_d112a5cd59a50ada");

    // Final sig_hash
    let sig_hash = spend.sig_hash();
    println!(
        "\nFinal sig_hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        sig_hash.values[0],
        sig_hash.values[1],
        sig_hash.values[2],
        sig_hash.values[3],
        sig_hash.values[4]
    );
    println!("Expected:       b5a460c35639f670_5669f17d0d1c673b_7117e0793673d153_08351a9913062377_cf9bbbba73a69824");
}

#[test]
#[ignore] // TODO: build_schnorr_pubkey_hashable is not implemented yet
fn test_wasm_pubkey_hash_vs_reference() {
    println!("\n=== Testing WASM Pubkey Hash vs Reference ===");

    // Import the WASM functions we need to test
    // use nockster_wasm::build_schnorr_pubkey_hashable;
    use nockster_wasm::hash_hashable_wasm;

    // The pubkey from demo-tx (seed 1)
    let pk = SchnorrPubkey {
        x: F6LT {
            values: [
                1213264621707318396,
                7644592116046038696,
                12750713645667184650,
                4785470970688526859,
                14650880413807991875,
                12274556524416646944,
            ],
        },
        y: F6LT {
            values: [
                18177236637613408617,
                10958279360383408893,
                1240025389805216209,
                14139010256592505920,
                18119211718294268888,
                6152380099229918899,
            ],
        },
        inf: false,
    };

    // Reference implementation
    let mut slab: NounSlab = NounSlab::new();
    let pk_noun = pk.to_noun(&mut slab);
    let reference_hash = tx_types::hashing::tip5::Tip5Hasher::hash_noun_varlen(pk_noun).unwrap();

    println!(
        "Reference: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        reference_hash.values[0],
        reference_hash.values[1],
        reference_hash.values[2],
        reference_hash.values[3],
        reference_hash.values[4]
    );

    // WASM implementation - build_schnorr_pubkey_hashable not implemented yet
    // let wasm_hashable = build_schnorr_pubkey_hashable(&pk);
    // let wasm_hash = match wasm_hashable {
    //     tx_types::hashing::hashable::Hashable::Hash(h) => h,
    //     _ => panic!("Expected Hash variant"),
    // };

    // println!(
    //     "WASM:      {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
    //     wasm_hash.values[0],
    //     wasm_hash.values[1],
    //     wasm_hash.values[2],
    //     wasm_hash.values[3],
    //     wasm_hash.values[4]
    // );

    // assert_eq!(
    //     wasm_hash.values, reference_hash.values,
    //     "Pubkey hash mismatch!"
    // );
}

#[test]
fn test_understand_noun_hashing() {
    println!("\n=== Understanding how hash_noun_varlen works ===");

    use nockster_wasm::hash_hashable_wasm;
    use nockvm::noun::{Atom, Cell};

    // Create a simple test: a 2-element tuple [1 2]
    let mut slab: NounSlab = NounSlab::new();
    let atom1 = Atom::new(&mut slab, 1u64).as_noun();
    let atom2 = Atom::new(&mut slab, 2u64).as_noun();
    let tuple = Cell::new(&mut slab, atom1, atom2).as_noun();

    let hash1 = tx_types::hashing::tip5::Tip5Hasher::hash_noun_varlen(tuple).unwrap();
    println!(
        "Reference hash of [1 2]: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        hash1.values[0], hash1.values[1], hash1.values[2], hash1.values[3], hash1.values[4]
    );

    // Now try with WASM hasher - use leaf_from_atom for u64 values
    use tx_types::hashing::hashable::Hashable;

    // Helper to create leaf from u64
    fn hashable_leaf_u64(value: u64) -> Hashable {
        Hashable::leaf_from_atom(&value.to_le_bytes())
    }

    let wasm_tuple = Hashable::cell(hashable_leaf_u64(1), hashable_leaf_u64(2));
    let wasm_hash1 = hash_hashable_wasm(&wasm_tuple).unwrap();
    println!(
        "WASM hash of [1 2]:      {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        wasm_hash1.values[0],
        wasm_hash1.values[1],
        wasm_hash1.values[2],
        wasm_hash1.values[3],
        wasm_hash1.values[4]
    );

    // Try a 3-element tuple [1 [2 3]]
    let atom3 = Atom::new(&mut slab, 3u64).as_noun();
    let inner = Cell::new(&mut slab, atom2, atom3).as_noun();
    let tuple3 = Cell::new(&mut slab, atom1, inner).as_noun();

    let hash2 = tx_types::hashing::tip5::Tip5Hasher::hash_noun_varlen(tuple3).unwrap();
    println!(
        "\nReference hash of [1 [2 3]]: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        hash2.values[0], hash2.values[1], hash2.values[2], hash2.values[3], hash2.values[4]
    );

    let wasm_tuple3 = Hashable::cell(
        hashable_leaf_u64(1),
        Hashable::cell(hashable_leaf_u64(2), hashable_leaf_u64(3)),
    );
    let wasm_hash2 = hash_hashable_wasm(&wasm_tuple3).unwrap();
    println!(
        "WASM hash of [1 [2 3]]:      {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        wasm_hash2.values[0],
        wasm_hash2.values[1],
        wasm_hash2.values[2],
        wasm_hash2.values[3],
        wasm_hash2.values[4]
    );
}
