use siger_core::cheetah::bip39_to_seed;
/// Test signature generation step by step to find where we diverge from reference
use siger_core::cheetah::{cheetah_pub_from_sk, test_tip5_hash_words};

#[test]
fn test_signature_step_by_step() {
    println!("\n{}", "=".repeat(80));
    println!("SIGNATURE GENERATION STEP BY STEP");
    println!("{}\n", "=".repeat(80));

    // Known good mnemonic
    let mnemonic = "around squeeze nerve chronic trophy kiwi enroll identify depth bicycle radio gate critic child claim outer detect plug market visual stuff finish crime abuse";

    // Generate seed and keys
    let seed = bip39_to_seed(mnemonic, "").expect("bip39");
    let (sk_be, _cc) = siger_core::cheetah::master_from_seed(&seed);
    let pk = cheetah_pub_from_sk(sk_be);

    println!("Secret key (first 8 bytes): {:02x?}", &sk_be[..8]);
    println!("Public key x: {:016x?}", pk[0]);
    println!("Public key y: {:016x?}", pk[1]);

    // Test message (from known-good.draft)
    let message = [
        0xb5a460c35639f670,
        0x5669f17d0d1c673b,
        0x7117e0793673d153,
        0x08351a9913062377,
        0xcf9bbbba73a69824,
    ];

    println!("\nMessage: {:016x?}", message);

    // Step 1: Build nonce transcript [pk.x, pk.y, message]
    let mut nonce_transcript = [0u64; 17];
    nonce_transcript[..6].copy_from_slice(&pk[0]);
    nonce_transcript[6..12].copy_from_slice(&pk[1]);
    nonce_transcript[12..].copy_from_slice(&message);

    println!("\nNonce transcript (17 elements):");
    for (i, &val) in nonce_transcript.iter().enumerate() {
        println!("  [{}]: {:016x}", i, val);
    }

    // Step 2: Hash to get nonce
    let nonce_hash = test_tip5_hash_words(&nonce_transcript);
    println!("\nNonce hash (5 elements): {:016x?}", nonce_hash);

    println!("\n{}", "=".repeat(80));
}
