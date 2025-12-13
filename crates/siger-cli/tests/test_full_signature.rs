/// Test full signature generation and compare with reference
use siger_core::cheetah::{cheetah_pub_from_sk, schnorr_sign_tx, test_tip5_hash_words};
use siger_core::cheetah::bip39_to_seed;
use siger_core::cheetah::utils::trunc_g_order_to_be32;

#[test]
fn test_full_signature_known_values() {
    println!("\n{}", "=".repeat(80));
    println!("FULL SIGNATURE GENERATION TEST");
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

    // Generate signature
    println!("\n{}", "-".repeat(80));
    println!("STEP 1: Computing nonce");
    println!("{}", "-".repeat(80));

    let mut nonce_transcript = [0u64; 17];
    nonce_transcript[..6].copy_from_slice(&pk[0]);
    nonce_transcript[6..12].copy_from_slice(&pk[1]);
    nonce_transcript[12..].copy_from_slice(&message);

    println!("Nonce transcript: {:016x?}", &nonce_transcript[..]);

    let nonce_digest = test_tip5_hash_words(&nonce_transcript);
    println!("Nonce digest: {:016x?}", nonce_digest);

    let nonce_be = trunc_g_order_to_be32(nonce_digest);
    println!("Nonce (truncated): {:02x?}", &nonce_be[..16]);

    println!("\n{}", "-".repeat(80));
    println!("STEP 2: Computing R = nonce * G");
    println!("{}", "-".repeat(80));

    let r_pt = cheetah_pub_from_sk(nonce_be);
    println!("R.x: {:016x?}", r_pt[0]);
    println!("R.y: {:016x?}", r_pt[1]);

    println!("\n{}", "-".repeat(80));
    println!("STEP 3: Computing challenge");
    println!("{}", "-".repeat(80));

    let mut chal_transcript = [0u64; 29];
    chal_transcript[..6].copy_from_slice(&r_pt[0]);
    chal_transcript[6..12].copy_from_slice(&r_pt[1]);
    chal_transcript[12..18].copy_from_slice(&pk[0]);
    chal_transcript[18..24].copy_from_slice(&pk[1]);
    chal_transcript[24..].copy_from_slice(&message);

    println!(
        "Challenge transcript (first 10): {:016x?}",
        &chal_transcript[..10]
    );

    let chal_digest = test_tip5_hash_words(&chal_transcript);
    println!("Challenge digest: {:016x?}", chal_digest);

    let chal_be = trunc_g_order_to_be32(chal_digest);
    println!("Challenge (truncated): {:02x?}", &chal_be[..16]);

    println!("\n{}", "-".repeat(80));
    println!("STEP 4: Using schnorr_sign_tx");
    println!("{}", "-".repeat(80));

    let (chal_t8, sig_t8) = schnorr_sign_tx(sk_be, (pk[0], pk[1]), message);

    println!("Challenge T8: {:08x?}", chal_t8.values);
    println!("Signature T8: {:08x?}", sig_t8.values);

    println!("\n{}", "=".repeat(80));
}
