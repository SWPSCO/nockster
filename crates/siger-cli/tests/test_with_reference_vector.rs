use nockapp::noun::slab::NounSlab;
use nockvm::noun::{Atom, Cell};
/// Test with the exact vector from reference test
use siger_core::cheetah::{cheetah_pub_from_sk, test_tip5_hash_words};
use siger_core::cheetah::utils::t8_to_be32;
use tx_types::hashing::tip5::Tip5Hasher;
use tx_types::transaction_types::T8;

#[test]
fn test_reference_vector() {
    println!("\n{}", "=".repeat(80));
    println!("TESTING WITH REFERENCE VECTOR");
    println!("{}\n", "=".repeat(80));

    // From reference test: secret key as T8
    let secret_key_t8 = T8 {
        values: [
            0xbbbb_cccc,
            0x9999_aaaa,
            0x7777_8888,
            0x5555_6666,
            0x3333_4444,
            0x1111_2222,
            0x9abc_def0,
            0x1234_5678,
        ],
    };

    let secret_key_be = t8_to_be32(&secret_key_t8);
    let pk = cheetah_pub_from_sk(secret_key_be);

    // Message from reference: [1, 2, 3, 4, 5]
    let message = [1u64, 2, 3, 4, 5];

    println!("Secret key (T8): {:08x?}", secret_key_t8.values);
    println!("Public key:");
    println!("  x: {:016x?}", &pk[0][..3]);
    println!("  y: {:016x?}", &pk[1][..3]);
    println!("Message: {:?}", message);

    // Build nonce transcript
    let mut nonce_transcript = [0u64; 17];
    nonce_transcript[..6].copy_from_slice(&pk[0]);
    nonce_transcript[6..12].copy_from_slice(&pk[1]);
    nonce_transcript[12..].copy_from_slice(&message);

    // Test flat array hasher
    let hash_flat = test_tip5_hash_words(&nonce_transcript);
    println!("\nFlat array nonce hash:");
    println!("  {:016x?}", hash_flat);

    // Test noun-based hasher
    let mut slab: NounSlab = NounSlab::new();
    let mut list = Atom::new(&mut slab, 0).as_noun();
    for &element in nonce_transcript.iter().rev() {
        let atom = Atom::new(&mut slab, element).as_noun();
        list = Cell::new(&mut slab, atom, list).as_noun();
    }
    let hash_noun = Tip5Hasher::hash_varlen(list).unwrap();
    println!("\nNoun-based nonce hash:");
    println!("  {:016x?}", hash_noun.values);

    if hash_flat == hash_noun.values {
        println!("\n✓ Hashes MATCH!");
    } else {
        println!("\n❌ Hashes DIFFER!");
    }

    // Expected from reference (we'd need to calculate this)
    println!("\n{}", "=".repeat(80));
    println!("Expected challenge from reference: [364619a6, 6af9178c, 46e47b17, f8609591, f4c6b69a, 1a511b32, d7e56411, 2f519cb9]");
    println!("{}", "=".repeat(80));
}
