//! Debug message format differences

use tx_types::{Hash, SchnorrPubkey, F6LT};

const TEST_MNEMONIC: &str = "fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float";

// The mnemonic used in test_full_signature.rs
const KNOWN_GOOD_MNEMONIC: &str = "around squeeze nerve chronic trophy kiwi enroll identify depth bicycle radio gate critic child claim outer detect plug market visual stuff finish crime abuse";

#[test]
fn debug_message_formats() {
    println!("\n=== Debugging message format ===\n");

    // Known-good message from test_full_signature.rs (produces correct signature)
    let known_good_message = [
        0xb5a460c35639f670_u64,
        0x5669f17d0d1c673b_u64,
        0x7117e0793673d153_u64,
        0x08351a9913062377_u64,
        0xcf9bbbba73a69824_u64,
    ];

    // sig_hash from test.tx (our signing differs from test.signed)
    let test_tx_sig_hash = [
        0xe1a8f75a7da568cb_u64,
        0x471bdb94980b0950_u64,
        0x87a549130bdf74d2_u64,
        0x642e0577cb3bbebf_u64,
        0xa94875f34771612a_u64,
    ];

    println!("--- Comparing message formats ---");
    println!("Known-good message (produces correct sig with known-good mnemonic):");
    println!(
        "  as u64: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        known_good_message[0],
        known_good_message[1],
        known_good_message[2],
        known_good_message[3],
        known_good_message[4]
    );

    // Convert to bytes (LE)
    let known_bytes: Vec<u8> = known_good_message
        .iter()
        .flat_map(|&v| v.to_le_bytes())
        .collect();
    println!("  as bytes (LE): {:02x?}", &known_bytes[..]);

    // Convert to bytes (BE)
    let known_bytes_be: Vec<u8> = known_good_message
        .iter()
        .flat_map(|&v| v.to_be_bytes())
        .collect();
    println!("  as bytes (BE): {:02x?}", &known_bytes_be[..]);

    println!("\ntest.tx sig_hash (our sig differs from test.signed):");
    println!(
        "  as u64: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        test_tx_sig_hash[0],
        test_tx_sig_hash[1],
        test_tx_sig_hash[2],
        test_tx_sig_hash[3],
        test_tx_sig_hash[4]
    );

    let test_bytes: Vec<u8> = test_tx_sig_hash
        .iter()
        .flat_map(|&v| v.to_le_bytes())
        .collect();
    println!("  as bytes (LE): {:02x?}", &test_bytes[..]);

    // Now let's sign both with TEST_MNEMONIC and see what happens
    println!("\n--- Signing with TEST_MNEMONIC ---");

    let seed = siger_core::cheetah::bip39_to_seed(TEST_MNEMONIC, "").expect("bip39");
    let (sk_be, _) = siger_core::cheetah::master_from_seed(&seed);
    let pk = tx_types::crypto::cheetah_pub_from_sk(sk_be);

    let pubkey = SchnorrPubkey {
        x: F6LT { values: pk[0] },
        y: F6LT { values: pk[1] },
        inf: false,
    };
    let our_pkh = pubkey.to_hash();

    println!(
        "Our PKH: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        our_pkh.values[0],
        our_pkh.values[1],
        our_pkh.values[2],
        our_pkh.values[3],
        our_pkh.values[4]
    );

    // Sign known-good message
    let (chal1, sig1) =
        siger_core::cheetah::schnorr_sign_tx(sk_be, (pk[0], pk[1]), known_good_message);
    println!("\nSigning known-good message:");
    println!("  Chal: {:08x?}", chal1.values);
    println!("  Sig:  {:08x?}", sig1.values);

    // Sign test.tx sig_hash
    let (chal2, sig2) =
        siger_core::cheetah::schnorr_sign_tx(sk_be, (pk[0], pk[1]), test_tx_sig_hash);
    println!("\nSigning test.tx sig_hash:");
    println!("  Chal: {:08x?}", chal2.values);
    println!("  Sig:  {:08x?}", sig2.values);

    // Try byte-swapped version of test.tx sig_hash
    let swapped_sig_hash: [u64; 5] = [
        test_tx_sig_hash[0].swap_bytes(),
        test_tx_sig_hash[1].swap_bytes(),
        test_tx_sig_hash[2].swap_bytes(),
        test_tx_sig_hash[3].swap_bytes(),
        test_tx_sig_hash[4].swap_bytes(),
    ];
    println!(
        "\nSwapped sig_hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        swapped_sig_hash[0],
        swapped_sig_hash[1],
        swapped_sig_hash[2],
        swapped_sig_hash[3],
        swapped_sig_hash[4]
    );

    let (chal3, sig3) =
        siger_core::cheetah::schnorr_sign_tx(sk_be, (pk[0], pk[1]), swapped_sig_hash);
    println!("Signing swapped sig_hash:");
    println!("  Chal: {:08x?}", chal3.values);
    println!("  Sig:  {:08x?}", sig3.values);

    // Try reversed order
    let reversed_sig_hash: [u64; 5] = [
        test_tx_sig_hash[4],
        test_tx_sig_hash[3],
        test_tx_sig_hash[2],
        test_tx_sig_hash[1],
        test_tx_sig_hash[0],
    ];
    println!(
        "\nReversed sig_hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        reversed_sig_hash[0],
        reversed_sig_hash[1],
        reversed_sig_hash[2],
        reversed_sig_hash[3],
        reversed_sig_hash[4]
    );

    let (chal4, sig4) =
        siger_core::cheetah::schnorr_sign_tx(sk_be, (pk[0], pk[1]), reversed_sig_hash);
    println!("Signing reversed sig_hash:");
    println!("  Chal: {:08x?}", chal4.values);
    println!("  Sig:  {:08x?}", sig4.values);

    // Expected from test.signed (T8 format - each u64 has lower 32 bits only)
    let expected_chal: [u64; 8] = [
        0x4ed16b28, 0xb3bae0f8, 0x1b90a638, 0x456cdeeb, 0x5b182da6, 0x82d6be33, 0x3b976661,
        0x6cff5472,
    ];
    println!("\n--- Expected challenge from test.signed ---");
    println!("  Expected: {:08x?}", expected_chal);
    println!("  Our (sig_hash): {:08x?}", chal2.values);

    // Try with known-good mnemonic to see what happens
    println!("\n--- Trying with KNOWN_GOOD_MNEMONIC ---");
    let seed_kg = siger_core::cheetah::bip39_to_seed(KNOWN_GOOD_MNEMONIC, "").expect("bip39");
    let (sk_be_kg, _) = siger_core::cheetah::master_from_seed(&seed_kg);
    let pk_kg = tx_types::crypto::cheetah_pub_from_sk(sk_be_kg);

    let pubkey_kg = SchnorrPubkey {
        x: F6LT { values: pk_kg[0] },
        y: F6LT { values: pk_kg[1] },
        inf: false,
    };
    let pkh_kg = pubkey_kg.to_hash();
    println!(
        "Known-good PKH: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        pkh_kg.values[0], pkh_kg.values[1], pkh_kg.values[2], pkh_kg.values[3], pkh_kg.values[4]
    );

    // Sign test.tx sig_hash with known-good key
    let (chal_kg, sig_kg) =
        siger_core::cheetah::schnorr_sign_tx(sk_be_kg, (pk_kg[0], pk_kg[1]), test_tx_sig_hash);
    println!("Signing test.tx sig_hash with known-good key:");
    println!("  Chal: {:08x?}", chal_kg.values);
    println!("  Sig:  {:08x?}", sig_kg.values);

    // Check if it matches expected
    if chal_kg.values == expected_chal {
        println!("  ✓ Challenge MATCHES! (test.signed was signed with known-good mnemonic)");
    } else {
        println!("  ✗ Challenge still differs");
    }

    println!("\n=== Done ===\n");
}
