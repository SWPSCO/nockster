//! Test signing with the CORRECT nonce derivation (includes secret key)

use bytes::Bytes;
use nockapp::noun::slab::NounSlab;
use noun_serde::NounDecode;
use std::fs;
use tx_types::crypto::cheetah::point::cheetah_pub_from_sk;
use tx_types::crypto::utils::{
    add_mod_n, be32_atom_to_t8_le, mul_mod_n, t8_to_be32, trunc_g_order_to_be32,
};
use tx_types::hashing::hasher::hash_transcript_list;
use tx_types::transaction_types::{Hash, SpendBody, T8};
use tx_types::transaction_types_v1::*;

const TEST_MNEMONIC: &str = "fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float";

/// Fixed signing implementation that includes secret key in nonce derivation
fn schnorr_sign_fixed(sk_t8: T8, pk: ([u64; 6], [u64; 6]), message: [u64; 5]) -> (T8, T8) {
    // Convert sk_t8 to big-endian bytes
    let sk_be = t8_to_be32(&sk_t8);

    // FIXED: Include secret key in nonce transcript
    // Hoon: [(f6lt-to-list x.pubkey) (f6lt-to-list y.pubkey) m-list sk-as-32-bit-belts ~]
    let nonce_digest = hash_transcript_list(&[
        &pk.0[..],         // pubkey.x (6 elements)
        &pk.1[..],         // pubkey.y (6 elements)
        &message[..],      // message (5 elements)
        &sk_t8.values[..], // secret key (8 elements) - THIS WAS MISSING!
    ])
    .expect("hash");

    let nonce_be = trunc_g_order_to_be32(nonce_digest.values);

    // Compute R = nonce × G
    let r_pt = cheetah_pub_from_sk(nonce_be);

    // Challenge transcript: [R.x, R.y, pk.x, pk.y, message]
    let chal_digest = hash_transcript_list(&[
        &r_pt[0],     // R.x (6 elements)
        &r_pt[1],     // R.y (6 elements)
        &pk.0[..],    // pubkey.x (6 elements)
        &pk.1[..],    // pubkey.y (6 elements)
        &message[..], // message (5 elements)
    ])
    .expect("hash");
    let chal_be = trunc_g_order_to_be32(chal_digest.values);

    // Compute signature: s = (nonce + chal * sk) mod n
    let chal_times_sk = mul_mod_n(&chal_be, &sk_be);
    let s_be = add_mod_n(&nonce_be, &chal_times_sk);

    // Convert to T8 format
    let chal_t8 = be32_atom_to_t8_le(&chal_be);
    let sig_t8 = be32_atom_to_t8_le(&s_be);

    (chal_t8, sig_t8)
}

/// Convert [u8; 32] big-endian to T8 format
fn be32_to_t8(be: &[u8; 32]) -> T8 {
    // Reverse to little-endian
    let mut le = [0u8; 32];
    for i in 0..32 {
        le[i] = be[31 - i];
    }

    // Pack into 8x u32 values stored in u64
    let mut values = [0u64; 8];
    for i in 0..8 {
        values[i] =
            u32::from_le_bytes([le[i * 4], le[i * 4 + 1], le[i * 4 + 2], le[i * 4 + 3]]) as u64;
    }
    T8 { values }
}

#[test]
fn test_fixed_signing_matches_test_signed() {
    println!("\n=== Testing FIXED signing (with sk in nonce) ===\n");

    // Derive key from mnemonic
    let seed = siger_core::cheetah::bip39_to_seed(TEST_MNEMONIC, "").expect("bip39");
    let (sk_be, _cc) = siger_core::cheetah::master_from_seed(&seed);
    let pk = tx_types::crypto::cheetah_pub_from_sk(sk_be);
    let sk_t8 = be32_to_t8(&sk_be);

    // Load test.signed and get sig_hash and expected signature
    let signed_data = fs::read("../../test.signed").expect("read test.signed");
    let mut slab: NounSlab = NounSlab::new();
    let noun = slab.cue_into(Bytes::from(signed_data)).expect("cue");
    let v1 = RawTransactionV1::from_noun(&noun).expect("decode as V1");

    for (name, spend) in v1.spends.map.tap() {
        if let SpendBody::V1(sb) = &spend.body {
            let sig_hash = sb.compute_sig_hash();

            for (pkh, sig_val) in sb.witness.pkh.map.tap() {
                let expected_chal = &sig_val.sig.chal.values;
                let expected_sig = &sig_val.sig.sig.values;

                println!(
                    "sig_hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    sig_hash.values[0],
                    sig_hash.values[1],
                    sig_hash.values[2],
                    sig_hash.values[3],
                    sig_hash.values[4]
                );

                // Sign with FIXED implementation
                let (our_chal, our_sig) =
                    schnorr_sign_fixed(sk_t8.clone(), (pk[0], pk[1]), sig_hash.values);

                println!("\nExpected challenge:");
                println!(
                    "  {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    expected_chal.values[0],
                    expected_chal.values[1],
                    expected_chal.values[2],
                    expected_chal.values[3],
                    expected_chal.values[4],
                    expected_chal.values[5],
                    expected_chal.values[6],
                    expected_chal.values[7]
                );

                println!("\nOur challenge (fixed):");
                println!(
                    "  {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    our_chal.values[0],
                    our_chal.values[1],
                    our_chal.values[2],
                    our_chal.values[3],
                    our_chal.values[4],
                    our_chal.values[5],
                    our_chal.values[6],
                    our_chal.values[7]
                );

                if our_chal.values == expected_chal.values {
                    println!("\n✓ Challenge MATCHES!");
                } else {
                    println!("\n✗ Challenge still differs");
                }

                if our_sig.values == expected_sig.values {
                    println!("✓ Signature MATCHES!");
                } else {
                    println!("✗ Signature still differs");
                }
            }
        }
    }

    println!("\n=== Done ===\n");
}
