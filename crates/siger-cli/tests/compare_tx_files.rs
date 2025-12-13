//! Compare test.tx and test.signed to understand the difference

use bytes::Bytes;
use nockapp::noun::slab::NounSlab;
use noun_serde::NounDecode;
use std::fs;
use tx_types::transaction_types::SpendBody;
use tx_types::transaction_types_v1::*;
use tx_types::RawTransaction;

#[test]
fn compare_tx_files() {
    println!("\n=== Comparing test.tx and test.signed ===\n");

    // Load both files
    let tx_data = fs::read("../../test.tx").expect("read test.tx");
    let signed_data = fs::read("../../test.signed").expect("read test.signed");

    println!("test.tx size: {} bytes", tx_data.len());
    println!("test.signed size: {} bytes", signed_data.len());
    println!("Size difference: {} bytes", signed_data.len() as i64 - tx_data.len() as i64);

    // Decode test.tx
    let mut slab1: NounSlab = NounSlab::new();
    let noun1 = slab1.cue_into(Bytes::from(tx_data)).expect("cue test.tx");

    // Decode test.signed
    let mut slab2: NounSlab = NounSlab::new();
    let noun2 = slab2.cue_into(Bytes::from(signed_data)).expect("cue test.signed");

    // Try to decode test.tx as RawTransactionV1
    println!("\n--- test.tx ---");
    let mut tx_sig_hash = None;
    if let Ok(v1) = RawTransactionV1::from_noun(&noun1) {
        println!("Decoded as RawTransactionV1");
        println!("  txid: {}", v1.id.to_b58());
        for (name, spend) in v1.spends.map.tap() {
            println!("  Spend: {:?}", name);
            if let SpendBody::V1(sb) = &spend.body {
                let sig_hash = sb.compute_sig_hash();
                tx_sig_hash = Some(sig_hash.clone());
                println!("    Fee: {}", sb.fee.value);
                println!("    sig_hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    sig_hash.values[0], sig_hash.values[1], sig_hash.values[2],
                    sig_hash.values[3], sig_hash.values[4]);
                println!("    PKH sigs count: {}", sb.witness.pkh.map.wyt());
                if sb.witness.pkh.map.wyt() > 0 {
                    println!("    (HAS SIGNATURES!)");
                }
            }
        }
    } else if let Ok(raw) = RawTransaction::from_noun(&noun1) {
        match raw {
            RawTransaction::V1(v1) => {
                println!("Decoded as RawTransaction::V1");
                println!("  txid: {}", v1.id.to_b58());
                for (name, spend) in v1.spends.map.tap() {
                    println!("  Spend: {:?}", name);
                    if let SpendBody::V1(sb) = &spend.body {
                        println!("    Fee: {}", sb.fee.value);
                        println!("    PKH sigs count: {}", sb.witness.pkh.map.wyt());
                        if sb.witness.pkh.map.wyt() > 0 {
                            println!("    (HAS SIGNATURES!)");
                        }
                    }
                }
            }
            _ => println!("Decoded as non-V1 RawTransaction"),
        }
    } else {
        println!("Could not decode test.tx");
    }

    // Try to decode test.signed as RawTransactionV1
    println!("\n--- test.signed ---");
    let mut signed_sig_hash = None;
    if let Ok(v1) = RawTransactionV1::from_noun(&noun2) {
        println!("Decoded as RawTransactionV1");
        println!("  txid: {}", v1.id.to_b58());
        for (name, spend) in v1.spends.map.tap() {
            println!("  Spend: {:?}", name);
            if let SpendBody::V1(sb) = &spend.body {
                let sig_hash = sb.compute_sig_hash();
                signed_sig_hash = Some(sig_hash.clone());
                println!("    Fee: {}", sb.fee.value);
                println!("    sig_hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    sig_hash.values[0], sig_hash.values[1], sig_hash.values[2],
                    sig_hash.values[3], sig_hash.values[4]);
                println!("    PKH sigs count: {}", sb.witness.pkh.map.wyt());
                if sb.witness.pkh.map.wyt() > 0 {
                    println!("    (HAS SIGNATURES!)");
                    for (pkh, sig_val) in sb.witness.pkh.map.tap() {
                        println!("    PKH: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                            pkh.values[0], pkh.values[1], pkh.values[2], pkh.values[3], pkh.values[4]);
                        println!("    Chal: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                            sig_val.sig.chal.values.values[0], sig_val.sig.chal.values.values[1],
                            sig_val.sig.chal.values.values[2], sig_val.sig.chal.values.values[3],
                            sig_val.sig.chal.values.values[4], sig_val.sig.chal.values.values[5],
                            sig_val.sig.chal.values.values[6], sig_val.sig.chal.values.values[7]);
                    }
                }
            }
        }
    } else if let Ok(raw) = RawTransaction::from_noun(&noun2) {
        match raw {
            RawTransaction::V1(v1) => {
                println!("Decoded as RawTransaction::V1");
                println!("  txid: {}", v1.id.to_b58());
                for (name, spend) in v1.spends.map.tap() {
                    println!("  Spend: {:?}", name);
                    if let SpendBody::V1(sb) = &spend.body {
                        println!("    Fee: {}", sb.fee.value);
                        println!("    PKH sigs count: {}", sb.witness.pkh.map.wyt());
                        if sb.witness.pkh.map.wyt() > 0 {
                            println!("    (HAS SIGNATURES!)");
                        }
                    }
                }
            }
            _ => println!("Decoded as non-V1 RawTransaction"),
        }
    } else {
        println!("Could not decode test.signed");
    }

    // Compare sig_hashes
    println!("\n--- Comparison ---");
    if let (Some(tx_sh), Some(signed_sh)) = (&tx_sig_hash, &signed_sig_hash) {
        if tx_sh.values == signed_sh.values {
            println!("✓ sig_hash MATCHES between test.tx and test.signed");
        } else {
            println!("✗ sig_hash DIFFERS between test.tx and test.signed");
            println!("  test.tx:     {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                tx_sh.values[0], tx_sh.values[1], tx_sh.values[2], tx_sh.values[3], tx_sh.values[4]);
            println!("  test.signed: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                signed_sh.values[0], signed_sh.values[1], signed_sh.values[2], signed_sh.values[3], signed_sh.values[4]);
        }
    } else {
        println!("Could not extract sig_hash from one or both files");
    }

    println!("\n=== Done ===\n");
}
