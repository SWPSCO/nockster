use nockapp::noun::slab::NounSlab;
/// Compare flat array tip5_hash_words vs noun-based Tip5Hasher
use nockster_core::cheetah::test_tip5_hash_words;
use nockvm::noun::{Atom, Cell};
use tx_types::hashing::tip5::Tip5Hasher;

#[test]
fn test_compare_hash_methods() {
    println!("\n{}", "=".repeat(80));
    println!("COMPARING TIP5 HASH METHODS");
    println!("{}\n", "=".repeat(80));

    let public_words = [
        0x101, 0x102, 0x103, 0x104, 0x105, 0x106, 0x201, 0x202, 0x203, 0x204, 0x205, 0x206,
    ];
    let message = [
        0xb5a460c35639f670,
        0x5669f17d0d1c673b,
        0x7117e0793673d153,
        0x08351a9913062377,
        0xcf9bbbba73a69824,
    ];

    let mut transcript = [0u64; 17];
    transcript[..12].copy_from_slice(&public_words);
    transcript[12..].copy_from_slice(&message);

    println!("Transcript (17 words):");
    println!("  public words: {:016x?}", &public_words[..3]);
    println!("  msg:  {:016x?}", &message[..3]);

    // Method 1: Flat array hasher
    let hash_flat = test_tip5_hash_words(&transcript);
    println!("\nFlat array hash (tip5_hash_words):");
    println!("  {:016x?}", hash_flat);

    // Method 2: Noun-based hasher
    let mut slab: NounSlab = NounSlab::new();
    let mut list = Atom::new(&mut slab, 0).as_noun();
    for &element in transcript.iter().rev() {
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
