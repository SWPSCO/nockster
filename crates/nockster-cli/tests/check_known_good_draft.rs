//! Inspect a local ignored Bythos v1 draft fixture.

use bytes::Bytes;
use nockapp::noun::slab::NounSlab;
use noun_serde::NounDecode;
use std::fs;
use tx_types::transaction_types::SpendBody;
use tx_types::transaction_types_v1::*;
use tx_types::RawTransaction;

const LOCAL_BYTHOS_V1_DRAFT: &str = "../../known-good.draft";

#[test]
#[ignore = "requires local ignored Bythos v1 fixture artifacts"]
fn check_local_bythos_v1_draft_sig_hash() {
    println!("\n=== Checking local Bythos v1 draft fixture ===\n");

    // The known-good message from test_full_signature.rs
    let known_good_message = [
        0xb5a460c35639f670_u64,
        0x5669f17d0d1c673b_u64,
        0x7117e0793673d153_u64,
        0x08351a9913062377_u64,
        0xcf9bbbba73a69824_u64,
    ];
    println!(
        "Known-good message from test: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        known_good_message[0],
        known_good_message[1],
        known_good_message[2],
        known_good_message[3],
        known_good_message[4]
    );

    let draft_data = fs::read(LOCAL_BYTHOS_V1_DRAFT).expect("read local Bythos v1 draft fixture");
    println!("Bythos v1 draft size: {} bytes", draft_data.len());

    let mut slab: NounSlab = NounSlab::new();
    let noun = slab
        .cue_into(Bytes::from(draft_data))
        .expect("cue local Bythos v1 draft fixture");

    // Try to decode as RawTransactionV1
    if let Ok(v1) = RawTransactionV1::from_noun(&noun) {
        println!("Decoded as RawTransactionV1");
        println!("  txid: {}", v1.id.to_b58());
        for (name, spend) in v1.spends.map.tap() {
            println!("  Spend: {:?}", name);
            if let SpendBody::V1(sb) = &spend.body {
                let sig_hash = sb.compute_sig_hash();
                println!("    Fee: {}", sb.fee.value);
                println!(
                    "    sig_hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    sig_hash.values[0],
                    sig_hash.values[1],
                    sig_hash.values[2],
                    sig_hash.values[3],
                    sig_hash.values[4]
                );

                if sig_hash.values == known_good_message {
                    println!("    ✓ sig_hash MATCHES known-good message!");
                } else {
                    println!("    ✗ sig_hash DIFFERS from known-good message");
                }
            }
        }
    } else if let Ok(raw) = RawTransaction::from_noun(&noun) {
        match raw {
            RawTransaction::V1(v1) => {
                println!("Decoded as RawTransaction::V1");
                println!("  txid: {}", v1.id.to_b58());
                for (name, spend) in v1.spends.map.tap() {
                    println!("  Spend: {:?}", name);
                    if let SpendBody::V1(sb) = &spend.body {
                        let sig_hash = sb.compute_sig_hash();
                        println!("    Fee: {}", sb.fee.value);
                        println!(
                            "    sig_hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                            sig_hash.values[0],
                            sig_hash.values[1],
                            sig_hash.values[2],
                            sig_hash.values[3],
                            sig_hash.values[4]
                        );

                        if sig_hash.values == known_good_message {
                            println!("    ✓ sig_hash MATCHES known-good message!");
                        } else {
                            println!("    ✗ sig_hash DIFFERS from known-good message");
                        }
                    }
                }
            }
            _ => println!("Decoded as non-V1 RawTransaction"),
        }
    } else {
        println!("Could not decode as V1");

        // Try raw noun inspection
        if let Ok(cell) = noun.as_cell() {
            println!("Top-level cell");
            if let Ok(_head_cell) = cell.head().as_cell() {
                println!("  Head is cell");
                // Try [raw-tx ...] format
                if let Ok(v1) = RawTransactionV1::from_noun(&cell.head()) {
                    println!("  Head decodes as RawTransactionV1!");
                    println!("    txid: {}", v1.id.to_b58());
                    for (name, spend) in v1.spends.map.tap() {
                        println!("    Spend: {:?}", name);
                        if let SpendBody::V1(sb) = &spend.body {
                            let sig_hash = sb.compute_sig_hash();
                            println!(
                                "      sig_hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                                sig_hash.values[0],
                                sig_hash.values[1],
                                sig_hash.values[2],
                                sig_hash.values[3],
                                sig_hash.values[4]
                            );
                        }
                    }
                }
            } else if let Ok(head_atom) = cell.head().as_atom() {
                let bytes = head_atom.as_ne_bytes();
                println!("  Head is atom ({} bytes)", bytes.len());
                if let Ok(val) = head_atom.as_u64() {
                    println!("  Head value: {}", val);
                }
            }
        }
    }

    println!("\n=== Done ===\n");
}
