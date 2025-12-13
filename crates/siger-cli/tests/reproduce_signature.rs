//! Test to reproduce the exact signature from test.signed

use tx_types::{SchnorrPubkey, F6LT};

const TEST_MNEMONIC: &str = "fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float";

#[test]
fn reproduce_test_signed_signature() {
    println!("\n=== Reproducing test.signed signature ===\n");

    // The sig_hash from both test.tx and test.signed
    let sig_hash = [
        0xe1a8f75a7da568cb_u64,
        0x471bdb94980b0950_u64,
        0x87a549130bdf74d2_u64,
        0x642e0577cb3bbebf_u64,
        0xa94875f34771612a_u64,
    ];

    // Expected values from test.signed (T8 format with upper 32 bits zero)
    let expected_chal_t8 = [
        0x000000004ed16b28_u64,
        0x00000000b3bae0f8_u64,
        0x000000001b90a638_u64,
        0x00000000456cdeeb_u64,
        0x000000005b182da6_u64,
        0x0000000082d6be33_u64,
        0x000000003b976661_u64,
        0x000000006cff5472_u64,
    ];

    let expected_sig_t8 = [
        0x00000000a3b17134_u64,
        0x00000000d418cc57_u64,
        0x00000000b42ac46d_u64,
        0x00000000d171fdfd_u64,
        0x0000000079d7ac2d_u64,
        0x000000007913be98_u64,
        0x0000000091f16585_u64,
        0x0000000074c1599a_u64,
    ];

    // Expected PKH
    let expected_pkh = [
        0x0eda0970034393fd_u64,
        0x300ccf58a82636b5_u64,
        0x6d74190eb595fe4e_u64,
        0x951a40d0de7769d2_u64,
        0x43481c45bfd7235b_u64,
    ];

    // Derive key from mnemonic
    let seed = siger_core::cheetah::bip39_to_seed(TEST_MNEMONIC, "").expect("bip39");
    let (sk_be, _cc) = siger_core::cheetah::master_from_seed(&seed);
    let pk = tx_types::crypto::cheetah_pub_from_sk(sk_be);

    // Compute our PKH
    let pubkey = SchnorrPubkey {
        x: F6LT { values: pk[0] },
        y: F6LT { values: pk[1] },
        inf: false,
    };
    let our_pkh = pubkey.to_hash();

    println!("Our PKH: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        our_pkh.values[0], our_pkh.values[1], our_pkh.values[2], our_pkh.values[3], our_pkh.values[4]);
    println!("Expected PKH: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        expected_pkh[0], expected_pkh[1], expected_pkh[2], expected_pkh[3], expected_pkh[4]);

    assert_eq!(our_pkh.values, expected_pkh, "PKH mismatch - wrong key!");
    println!("✓ PKH matches - we have the correct key");

    // Sign the sig_hash
    println!("\nSigning sig_hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        sig_hash[0], sig_hash[1], sig_hash[2], sig_hash[3], sig_hash[4]);

    let (chal_t8, sig_t8) = siger_core::cheetah::schnorr_sign_tx(sk_be, (pk[0], pk[1]), sig_hash);

    // Convert T8 format (8x u32) to u64 for comparison
    let our_chal: Vec<u64> = chal_t8.values.iter().map(|&v| v as u64).collect();
    let our_sig: Vec<u64> = sig_t8.values.iter().map(|&v| v as u64).collect();

    println!("\nOur Challenge (T8 as u64):");
    for (i, v) in our_chal.iter().enumerate() {
        print!("{:016x}", v);
        if i < 7 { print!("_"); }
    }
    println!();

    println!("Expected Challenge:");
    for (i, v) in expected_chal_t8.iter().enumerate() {
        print!("{:016x}", v);
        if i < 7 { print!("_"); }
    }
    println!();

    println!("\nOur Signature (T8 as u64):");
    for (i, v) in our_sig.iter().enumerate() {
        print!("{:016x}", v);
        if i < 7 { print!("_"); }
    }
    println!();

    println!("Expected Signature:");
    for (i, v) in expected_sig_t8.iter().enumerate() {
        print!("{:016x}", v);
        if i < 7 { print!("_"); }
    }
    println!();

    // Also print in the raw u32 format
    println!("\nOur Challenge (raw u32): {:08x?}", chal_t8.values);
    println!("Our Signature (raw u32): {:08x?}", sig_t8.values);

    // Check if they match
    let chal_matches = our_chal.iter().zip(expected_chal_t8.iter()).all(|(a, b)| a == b);
    let sig_matches = our_sig.iter().zip(expected_sig_t8.iter()).all(|(a, b)| a == b);

    if chal_matches {
        println!("\n✓ Challenge MATCHES!");
    } else {
        println!("\n✗ Challenge DIFFERS");
    }

    if sig_matches {
        println!("✓ Signature MATCHES!");
    } else {
        println!("✗ Signature DIFFERS");
    }

    println!("\n=== Done ===\n");
}
