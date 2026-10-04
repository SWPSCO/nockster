//! Compare WASM hashable construction with reference implementation

use nockapp::noun::slab::NounSlab;
use noun_serde::NounEncode;
use tx_types::transaction_types::*;

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

    assert_eq!(
        reference_hash.values,
        [
            0x065e981ee38e23e9,
            0x3cb4c3764b81b8f1,
            0xabb22883f963bef7,
            0xa8ee11acc9f9527f,
            0x177577e778eb4145,
        ]
    );
}

#[test]
fn test_wasm_pubkey_hash_vs_reference() {
    println!("\n=== Testing WASM Pubkey Hash vs Reference ===");

    use nockster_wasm::build_schnorr_pubkey_hashable;
    use tx_types::hashing::hashable::Hashable;

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

    let wasm_hashable = build_schnorr_pubkey_hashable(&pk).unwrap();
    let wasm_hash = match wasm_hashable {
        Hashable::Hash(h) => h,
        _ => panic!("Expected Hash variant"),
    };

    println!(
        "WASM:      {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        wasm_hash.values[0],
        wasm_hash.values[1],
        wasm_hash.values[2],
        wasm_hash.values[3],
        wasm_hash.values[4]
    );

    assert_eq!(
        wasm_hash.values, reference_hash.values,
        "Pubkey hash mismatch!"
    );
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
