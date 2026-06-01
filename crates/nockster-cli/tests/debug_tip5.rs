/// Debug TIP5 hashing to understand the difference
use nockster_core::cheetah::test_tip5_hash_words;

#[test]
fn test_tip5_simple() {
    println!("\n{}", "=".repeat(80));
    println!("DEBUG: TIP5 with different inputs");
    println!("{}\n", "=".repeat(80));

    // Test 1: Empty input
    let empty: [u64; 0] = [];
    let hash_empty = test_tip5_hash_words(&empty);
    println!("Hash of empty: {:016x?}", hash_empty);

    // Test 2: Single element
    let single = [1u64];
    let hash_single = test_tip5_hash_words(&single);
    println!("Hash of [1]: {:016x?}", hash_single);

    // Test 3: 10 elements (exactly RATE)
    let ten = [1u64, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let hash_ten = test_tip5_hash_words(&ten);
    println!("Hash of [1..10]: {:016x?}", hash_ten);

    // Test 4: 11 elements (RATE + 1, requires padding)
    let eleven = [1u64, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    let hash_eleven = test_tip5_hash_words(&eleven);
    println!("Hash of [1..11]: {:016x?}", hash_eleven);

    // Test 5: 12 public-coordinate-shaped words.
    let public_words = [
        0x101, 0x102, 0x103, 0x104, 0x105, 0x106, 0x201, 0x202, 0x203, 0x204, 0x205, 0x206,
    ];
    let mut pk_words = [0u64; 12];
    pk_words.copy_from_slice(&public_words);

    let hash_pk = test_tip5_hash_words(&pk_words);
    println!("Hash of public words (12 words): {:016x?}", hash_pk);

    // Test 6: 17-word transcript shape.
    let message = [
        0xb5a460c35639f670,
        0x5669f17d0d1c673b,
        0x7117e0793673d153,
        0x08351a9913062377,
        0xcf9bbbba73a69824,
    ];

    let mut nonce_transcript = [0u64; 17];
    nonce_transcript[..12].copy_from_slice(&public_words);
    nonce_transcript[12..].copy_from_slice(&message);

    let hash_nonce = test_tip5_hash_words(&nonce_transcript);
    println!("Hash of nonce transcript (17 words): {:016x?}", hash_nonce);

    println!("\n{}", "=".repeat(80));
}
