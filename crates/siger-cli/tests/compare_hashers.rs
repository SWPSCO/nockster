use nockapp::noun::slab::NounSlab;
use nockvm::noun::{Atom, Cell};
/// Compare flat array tip5_hash_words vs noun-based Tip5Hasher
use siger_core::cheetah::{cheetah_pub_from_sk, test_tip5_hash_words};
use tx_types::hashing::tip5::Tip5Hasher;

#[test]
fn test_compare_hash_methods() {
    println!("\n{}", "=".repeat(80));
    println!("COMPARING TIP5 HASH METHODS");
    println!("{}\n", "=".repeat(80));

    let mnemonic = "around squeeze nerve chronic trophy kiwi enroll identify depth bicycle radio gate critic child claim outer detect plug market visual stuff finish crime abuse";
    let seed = siger_core::cheetah::bip39_to_seed(mnemonic, "").expect("bip39");
    let (sk_be, _cc) = siger_core::cheetah::master_from_seed(&seed);
    let pk = cheetah_pub_from_sk(sk_be);

    let message = [
        0xb5a460c35639f670,
        0x5669f17d0d1c673b,
        0x7117e0793673d153,
        0x08351a9913062377,
        0xcf9bbbba73a69824,
    ];

    // Build nonce transcript (17 elements: pk.x + pk.y + message)
    let mut nonce_transcript = [0u64; 17];
    nonce_transcript[..6].copy_from_slice(&pk[0]);
    nonce_transcript[6..12].copy_from_slice(&pk[1]);
    nonce_transcript[12..].copy_from_slice(&message);

    println!("Nonce transcript (17 words):");
    println!("  pk.x: {:016x?}", &pk[0][..3]);
    println!("  pk.y: {:016x?}", &pk[1][..3]);
    println!("  msg:  {:016x?}", &message[..3]);

    // Method 1: Flat array hasher
    let hash_flat = test_tip5_hash_words(&nonce_transcript);
    println!("\nFlat array hash (tip5_hash_words):");
    println!("  {:016x?}", hash_flat);

    // Method 2: Noun-based hasher
    let mut slab: NounSlab = NounSlab::new();
    let mut list = Atom::new(&mut slab, 0).as_noun();
    for &element in nonce_transcript.iter().rev() {
        let atom = Atom::new(&mut slab, element).as_noun();
        list = Cell::new(&mut slab, atom, list).as_noun();
    }
    let hash_noun = Tip5Hasher::hash_varlen(list).unwrap();
    println!("\nNoun-based hash (Tip5Hasher):");
    println!("  {:016x?}", hash_noun.values);

    if hash_flat == hash_noun.values {
        println!("\n✓ Hashes MATCH!");
    } else {
        println!("\n❌ Hashes DIFFER!");
        println!("This means tip5_hash_words doesn't match Tip5Hasher");
    }

    println!("\n{}", "=".repeat(80));
}
