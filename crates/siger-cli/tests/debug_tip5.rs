/// Debug TIP5 hashing to understand the difference
use siger_core::cheetah::{cheetah_pub_from_sk, test_tip5_hash_words};

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

    // Test 5: Public key coordinates (12 elements)
    let mnemonic = "around squeeze nerve chronic trophy kiwi enroll identify depth bicycle radio gate critic child claim outer detect plug market visual stuff finish crime abuse";
    let seed = siger_core::cheetah::bip39_to_seed(mnemonic, "").expect("bip39");
    let (sk_be, _cc) = siger_core::cheetah::master_from_seed(&seed);
    let pk = cheetah_pub_from_sk(sk_be);

    println!("\nPublic key:");
    println!("  x: {:016x?}", pk[0]);
    println!("  y: {:016x?}", pk[1]);

    let mut pk_words = [0u64; 12];
    pk_words[..6].copy_from_slice(&pk[0]);
    pk_words[6..].copy_from_slice(&pk[1]);

    let hash_pk = test_tip5_hash_words(&pk_words);
    println!("Hash of pubkey (12 words): {:016x?}", hash_pk);

    // Test 6: Nonce transcript for signing (17 elements = pubkey + message)
    let message = [
        0xb5a460c35639f670,
        0x5669f17d0d1c673b,
        0x7117e0793673d153,
        0x08351a9913062377,
        0xcf9bbbba73a69824,
    ];

    let mut nonce_transcript = [0u64; 17];
    nonce_transcript[..6].copy_from_slice(&pk[0]);
    nonce_transcript[6..12].copy_from_slice(&pk[1]);
    nonce_transcript[12..].copy_from_slice(&message);

    let hash_nonce = test_tip5_hash_words(&nonce_transcript);
    println!("Hash of nonce transcript (17 words): {:016x?}", hash_nonce);

    println!("\n{}", "=".repeat(80));
}
