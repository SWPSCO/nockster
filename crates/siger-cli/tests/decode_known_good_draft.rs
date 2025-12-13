//! Decode known-good.draft to find what message gets signed

use bytes::Bytes;
use nockapp::noun::slab::NounSlab;
use nockvm::noun::Noun;
use noun_serde::NounDecode;
use std::fs;
use tx_types::transaction_types::SpendBody;
use tx_types::transaction_types_v1::*;
use tx_types::RawTransaction;

// WalletTransaction format: [name: String, spends: SpendsV1]
fn decode_wallet_tx(noun: &Noun) -> Option<(String, SpendsV1)> {
    let cell = noun.as_cell().ok()?;

    // Head is name (b58 string)
    let name_atom = cell.head().as_atom().ok()?;
    let name_bytes = name_atom.as_ne_bytes();
    let name = std::str::from_utf8(&name_bytes).ok()?.to_string();

    // Tail is spends
    let spends = SpendsV1::from_noun(&cell.tail()).ok()?;

    Some((name, spends))
}

#[test]
fn decode_known_good_draft_as_wallet_tx() {
    println!("\n=== Decoding known-good.draft as WalletTransaction ===\n");

    let draft_data = fs::read("../../known-good.draft").expect("read known-good.draft");
    let mut slab: NounSlab = NounSlab::new();
    let noun = slab.cue_into(Bytes::from(draft_data)).expect("cue");

    if let Some((name, spends)) = decode_wallet_tx(&noun) {
        println!("Name: {}", name);
        println!("Spends count: {}", spends.map.wyt());

        for (spend_name, spend) in spends.map.tap() {
            println!("\n  Spend: {:?}", spend_name);
            if let SpendBody::V1(sb) = &spend.body {
                let sig_hash = sb.compute_sig_hash();
                println!("    Fee: {}", sb.fee.value);
                println!("    sig_hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    sig_hash.values[0], sig_hash.values[1], sig_hash.values[2],
                    sig_hash.values[3], sig_hash.values[4]);

                // Check if sig_hash matches known-good message
                let known_good = [
                    0xb5a460c35639f670_u64,
                    0x5669f17d0d1c673b_u64,
                    0x7117e0793673d153_u64,
                    0x08351a9913062377_u64,
                    0xcf9bbbba73a69824_u64,
                ];

                if sig_hash.values == known_good {
                    println!("    ✓ sig_hash MATCHES known-good message!");
                } else {
                    println!("    ✗ sig_hash differs from known-good message");
                    println!("    Expected: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                        known_good[0], known_good[1], known_good[2], known_good[3], known_good[4]);
                }
            }
        }
    } else {
        println!("Could not decode as WalletTransaction");
    }

    println!("\n=== Done ===\n");
}
