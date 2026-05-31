//! Compare our derived pubkey with the one stored in test.signed

use bytes::Bytes;
use nockapp::noun::slab::NounSlab;
use noun_serde::NounDecode;
use std::fs;
use tx_types::transaction_types::{SchnorrPubkey, SpendBody, F6LT};
use tx_types::transaction_types_v1::*;

const TEST_MNEMONIC: &str = "fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float";

#[test]
fn compare_pubkeys() {
    println!("\n=== Comparing pubkeys ===\n");

    // Derive our pubkey from mnemonic
    let seed = siger_core::cheetah::bip39_to_seed(TEST_MNEMONIC, "").expect("bip39");
    let (sk_be, _cc) = siger_core::cheetah::master_from_seed(&seed);
    let pk = tx_types::crypto::cheetah_pub_from_sk(sk_be);

    let our_pk = SchnorrPubkey {
        x: F6LT { values: pk[0] },
        y: F6LT { values: pk[1] },
        inf: false,
    };

    println!("Our derived pubkey:");
    println!(
        "  x: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        our_pk.x.values[0],
        our_pk.x.values[1],
        our_pk.x.values[2],
        our_pk.x.values[3],
        our_pk.x.values[4],
        our_pk.x.values[5]
    );
    println!(
        "  y: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        our_pk.y.values[0],
        our_pk.y.values[1],
        our_pk.y.values[2],
        our_pk.y.values[3],
        our_pk.y.values[4],
        our_pk.y.values[5]
    );
    println!(
        "  PKH: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        our_pk.to_hash().values[0],
        our_pk.to_hash().values[1],
        our_pk.to_hash().values[2],
        our_pk.to_hash().values[3],
        our_pk.to_hash().values[4]
    );

    // Load test.signed and get stored pubkey
    let signed_data = fs::read("../../test.signed").expect("read test.signed");
    let mut slab: NounSlab = NounSlab::new();
    let noun = slab.cue_into(Bytes::from(signed_data)).expect("cue");
    let v1 = RawTransactionV1::from_noun(&noun).expect("decode as V1");

    for (name, spend) in v1.spends.map.tap() {
        if let SpendBody::V1(sb) = &spend.body {
            for (pkh, sig_val) in sb.witness.pkh.map.tap() {
                let stored_pk = &sig_val.pk;

                println!("\nStored pubkey from test.signed:");
                println!(
                    "  x: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    stored_pk.x.values[0],
                    stored_pk.x.values[1],
                    stored_pk.x.values[2],
                    stored_pk.x.values[3],
                    stored_pk.x.values[4],
                    stored_pk.x.values[5]
                );
                println!(
                    "  y: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    stored_pk.y.values[0],
                    stored_pk.y.values[1],
                    stored_pk.y.values[2],
                    stored_pk.y.values[3],
                    stored_pk.y.values[4],
                    stored_pk.y.values[5]
                );
                println!(
                    "  PKH: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    pkh.values[0], pkh.values[1], pkh.values[2], pkh.values[3], pkh.values[4]
                );

                // Compare
                println!("\nComparison:");
                if our_pk.x.values == stored_pk.x.values {
                    println!("  ✓ x coordinates MATCH");
                } else {
                    println!("  ✗ x coordinates DIFFER");
                }

                if our_pk.y.values == stored_pk.y.values {
                    println!("  ✓ y coordinates MATCH");
                } else {
                    println!("  ✗ y coordinates DIFFER");
                }

                // Sign with stored pubkey vs our pubkey
                let sig_hash = sb.compute_sig_hash();
                println!("\nSigning with our pubkey (via schnorr_sign_tx):");
                let (our_chal, our_sig) = siger_core::cheetah::schnorr_sign_tx(
                    sk_be,
                    (our_pk.x.values, our_pk.y.values),
                    sig_hash.values,
                );
                println!(
                    "  Chal: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    our_chal.values[0],
                    our_chal.values[1],
                    our_chal.values[2],
                    our_chal.values[3],
                    our_chal.values[4],
                    our_chal.values[5],
                    our_chal.values[6],
                    our_chal.values[7]
                );

                println!("\nSigning with stored pubkey (via schnorr_sign_tx):");
                let (stored_chal, stored_sig) = siger_core::cheetah::schnorr_sign_tx(
                    sk_be,
                    (stored_pk.x.values, stored_pk.y.values),
                    sig_hash.values,
                );
                println!(
                    "  Chal: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    stored_chal.values[0],
                    stored_chal.values[1],
                    stored_chal.values[2],
                    stored_chal.values[3],
                    stored_chal.values[4],
                    stored_chal.values[5],
                    stored_chal.values[6],
                    stored_chal.values[7]
                );

                println!("\nExpected from test.signed:");
                println!(
                    "  Chal: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    sig_val.sig.chal.values.values[0],
                    sig_val.sig.chal.values.values[1],
                    sig_val.sig.chal.values.values[2],
                    sig_val.sig.chal.values.values[3],
                    sig_val.sig.chal.values.values[4],
                    sig_val.sig.chal.values.values[5],
                    sig_val.sig.chal.values.values[6],
                    sig_val.sig.chal.values.values[7]
                );
            }
        }
    }

    println!("\n=== Done ===\n");
}
