//! Verify a local ignored Bythos v1 signed transaction fixture.

use bytes::Bytes;
use nockapp::noun::slab::NounSlab;
use noun_serde::NounDecode;
use std::fs;
use tx_types::transaction_types::{Hash, SpendBody};
use tx_types::transaction_types_v1::*;
use tx_types::validation::schnorr_verify_digest;

const LOCAL_BYTHOS_V1_SIGNED_TX: &str = "../../test.signed";

#[test]
#[ignore = "requires local ignored Bythos v1 fixture artifacts"]
fn verify_local_bythos_v1_signed_signature() {
    println!("\n=== Verifying local Bythos v1 signed transaction ===\n");

    let signed_data =
        fs::read(LOCAL_BYTHOS_V1_SIGNED_TX).expect("read local Bythos v1 signed tx fixture");
    let mut slab: NounSlab = NounSlab::new();
    let noun = slab.cue_into(Bytes::from(signed_data)).expect("cue");

    // Decode as RawTransactionV1
    let v1 = RawTransactionV1::from_noun(&noun).expect("decode as V1");

    for (name, spend) in v1.spends.map.tap() {
        println!("Spend: {:?}", name);

        if let SpendBody::V1(sb) = &spend.body {
            // Get sig_hash
            let sig_hash = sb.compute_sig_hash();
            println!(
                "  sig_hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                sig_hash.values[0],
                sig_hash.values[1],
                sig_hash.values[2],
                sig_hash.values[3],
                sig_hash.values[4]
            );

            // Get signatures
            for (pkh, sig_val) in sb.witness.pkh.map.tap() {
                println!(
                    "  PKH: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    pkh.values[0], pkh.values[1], pkh.values[2], pkh.values[3], pkh.values[4]
                );

                let pk = &sig_val.pk;

                println!(
                    "  PK.x: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    pk.x.values[0],
                    pk.x.values[1],
                    pk.x.values[2],
                    pk.x.values[3],
                    pk.x.values[4],
                    pk.x.values[5]
                );
                println!(
                    "  PK.y: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    pk.y.values[0],
                    pk.y.values[1],
                    pk.y.values[2],
                    pk.y.values[3],
                    pk.y.values[4],
                    pk.y.values[5]
                );

                println!("\n  Attempting verification with sig_hash...");

                // Verify signature
                let verified =
                    schnorr_verify_digest(pk.clone(), sig_hash.clone(), sig_val.sig.clone());

                if verified {
                    println!("  ✓ Signature VERIFIED! local Bythos v1 fixture is valid");
                } else {
                    println!("  ✗ Signature NOT verified for sig_hash");

                    // Try the known-good message
                    let known_good = Hash {
                        values: [
                            0xb5a460c35639f670_u64,
                            0x5669f17d0d1c673b_u64,
                            0x7117e0793673d153_u64,
                            0x08351a9913062377_u64,
                            0xcf9bbbba73a69824_u64,
                        ],
                    };

                    println!("\n  Trying verification with known-good message...");
                    let verified_kg =
                        schnorr_verify_digest(pk.clone(), known_good, sig_val.sig.clone());

                    if verified_kg {
                        println!("  ✓ Signature verifies for KNOWN-GOOD message instead!");
                    } else {
                        println!("  ✗ Also doesn't verify for known-good message");
                    }
                }
            }
        }
    }

    println!("\n=== Done ===\n");
}
