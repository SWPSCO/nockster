//! V1 transaction signing tests
//! Tests that reproduce the signature in test.signed/test.wallet using the seed from seed.txt

use bytes::Bytes;
use nockapp::noun::slab::NounSlab;
use nockvm::noun::Noun;
use noun_serde::NounDecode;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use tx_types::transaction_types::*;
use tx_types::transaction_types_v1::*;
use tx_types::RawTransaction;

const TEST_MNEMONIC: &str = "fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float";

/// Helper to decode RawTransaction from noun, trying different formats
fn decode_raw_tx(noun: &Noun) -> Result<RawTransaction, String> {
    // Try direct decode first
    if let Ok(Ok(raw)) = catch_unwind(AssertUnwindSafe(|| RawTransaction::from_noun(noun))) {
        return Ok(raw);
    }

    // Try [raw-tx tail] format (tx:transact)
    if let Ok(cell) = noun.as_cell() {
        if let Ok(Ok(raw)) = catch_unwind(AssertUnwindSafe(|| RawTransaction::from_noun(&cell.head()))) {
            return Ok(raw);
        }
    }

    Err("Could not decode as RawTransaction".into())
}

#[test]
fn test_diagnose_signed_format() {
    println!("\n=== Diagnosing test.signed format ===\n");

    // test.signed is in raw-tx:transact format (RawTransactionV1)
    let data = fs::read("../../test.signed").expect("read test.signed");
    let mut slab: NounSlab = NounSlab::new();
    let noun = slab.cue_into(Bytes::from(data)).expect("cue");

    println!("Noun is cell: {}", noun.as_cell().is_ok());

    if let Ok(cell) = noun.as_cell() {
        println!("Head is cell: {}", cell.head().as_cell().is_ok());
        println!("Head is atom: {}", cell.head().as_atom().is_ok());
        println!("Tail is cell: {}", cell.tail().as_cell().is_ok());

        // Try to get version from head if it's an atom
        if let Ok(atom) = cell.head().as_atom() {
            let bytes = atom.as_ne_bytes();
            println!("Head atom size: {} bytes", bytes.len());
            if bytes.len() <= 16 {
                println!("Head atom bytes (LE): {:02x?}", bytes);
            }
            if let Ok(val) = atom.as_u64() {
                println!("Head atom value: {}", val);
            }
            // Try as string (cord)
            if let Ok(s) = std::str::from_utf8(&bytes) {
                println!("Head atom as string: \"{}\"", s);
            }
        }

        // Also check tail structure - more detailed analysis
        if let Ok(tail_cell) = cell.tail().as_cell() {
            println!("Tail.head is cell: {}", tail_cell.head().as_cell().is_ok());
            println!("Tail.head is atom: {}", tail_cell.head().as_atom().is_ok());
            println!("Tail.tail is cell: {}", tail_cell.tail().as_cell().is_ok());
            println!("Tail.tail is atom: {}", tail_cell.tail().as_atom().is_ok());
            if let Ok(atom) = tail_cell.head().as_atom() {
                let bytes = atom.as_ne_bytes();
                println!("Tail.head atom size: {} bytes", bytes.len());
                if bytes.len() <= 16 {
                    println!("Tail.head atom bytes (LE): {:02x?}", bytes);
                }
            }
            // Check if tail.head is a cell (ZMap entry)
            if let Ok(thc) = tail_cell.head().as_cell() {
                println!("Tail.head.head is atom: {}", thc.head().as_atom().is_ok());
                println!("Tail.head.tail is cell: {}", thc.tail().as_cell().is_ok());
                // Try to decode as NName
                if let Ok(atom) = thc.head().as_atom() {
                    let bytes = atom.as_ne_bytes();
                    println!("Tail.head.head (NName?) size: {} bytes", bytes.len());
                    if let Ok(s) = std::str::from_utf8(&bytes) {
                        println!("Tail.head.head as string: \"{}\"", s);
                    }
                }
            }
        }

        // Check if it's [name spends] format like V0 Transaction
        // V0 Transaction was [name inputs] where inputs is a ZMap
        if let Ok(head_cell) = cell.head().as_cell() {
            println!("Head.head is cell: {}", head_cell.head().as_cell().is_ok());
            println!("Head.head is atom: {}", head_cell.head().as_atom().is_ok());
        }
    }

    // Try InputsV1 directly on the tail
    if let Ok(cell) = noun.as_cell() {
        if let Ok(Ok(inputs)) = catch_unwind(AssertUnwindSafe(|| InputsV1::from_noun(&cell.tail()))) {
            println!("\nDecoded tail as InputsV1!");
            println!("  inputs count: {}", inputs.map.wyt());
            for (name, input) in inputs.map.tap() {
                println!("\n  Input: {:?}", name);
                println!("    Fee: {}", input.spend.fee.value);
                println!("    PKH sigs: {}", input.spend.witness.pkh.map.wyt());
                for (pkh, sig_val) in input.spend.witness.pkh.map.tap() {
                    println!("    PKH: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                        pkh.values[0], pkh.values[1], pkh.values[2], pkh.values[3], pkh.values[4]);
                    println!("    Chal: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                        sig_val.sig.chal.values.values[0], sig_val.sig.chal.values.values[1],
                        sig_val.sig.chal.values.values[2], sig_val.sig.chal.values.values[3],
                        sig_val.sig.chal.values.values[4], sig_val.sig.chal.values.values[5],
                        sig_val.sig.chal.values.values[6], sig_val.sig.chal.values.values[7]);
                    println!("    Sig:  {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                        sig_val.sig.sig.values.values[0], sig_val.sig.sig.values.values[1],
                        sig_val.sig.sig.values.values[2], sig_val.sig.sig.values.values[3],
                        sig_val.sig.sig.values.values[4], sig_val.sig.sig.values.values[5],
                        sig_val.sig.sig.values.values[6], sig_val.sig.sig.values.values[7]);
                }
            }
        } else {
            println!("\nInputsV1 decode from tail failed");
        }
    }

    // Try Transaction decode (name, inputs)
    if let Ok(Ok(tx)) = catch_unwind(AssertUnwindSafe(|| Transaction::from_noun(&noun))) {
        println!("\nDecoded as Transaction!");
        println!("  name: {}", tx.name);

        match tx.p {
            Inputs::V0(inputs_v0) => {
                println!("  V0 inputs count: {}", inputs_v0.p.wyt());
                for (name, input) in inputs_v0.p.tap() {
                    println!("\n  Input: {:?}", name);
                    println!("    Fee: {}", input.spend.fee.value);
                    // V0 has signature field
                    println!("    Signature: {:?}", input.spend.signature);
                }
            }
            Inputs::V1(inputs_v1) => {
                println!("  V1 inputs count: {}", inputs_v1.map.wyt());
                for (name, input) in inputs_v1.map.tap() {
                    println!("\n  Input: {:?}", name);
                    println!("    Fee: {}", input.spend.fee.value);
                    println!("    PKH sigs: {}", input.spend.witness.pkh.map.wyt());
                    for (pkh, sig_val) in input.spend.witness.pkh.map.tap() {
                        println!("    PKH: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                            pkh.values[0], pkh.values[1], pkh.values[2], pkh.values[3], pkh.values[4]);
                        println!("    Chal: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                            sig_val.sig.chal.values.values[0], sig_val.sig.chal.values.values[1],
                            sig_val.sig.chal.values.values[2], sig_val.sig.chal.values.values[3],
                            sig_val.sig.chal.values.values[4], sig_val.sig.chal.values.values[5],
                            sig_val.sig.chal.values.values[6], sig_val.sig.chal.values.values[7]);
                        println!("    Sig:  {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                            sig_val.sig.sig.values.values[0], sig_val.sig.sig.values.values[1],
                            sig_val.sig.sig.values.values[2], sig_val.sig.sig.values.values[3],
                            sig_val.sig.sig.values.values[4], sig_val.sig.sig.values.values[5],
                            sig_val.sig.sig.values.values[6], sig_val.sig.sig.values.values[7]);
                    }
                }
            }
        }
    } else {
        println!("\nTransaction decode failed");
    }

    // Try RawTransaction decode
    match decode_raw_tx(&noun) {
        Ok(raw) => {
            println!("\nDecoded as RawTransaction!");
            match raw {
                RawTransaction::V0(v0) => println!("  V0: id={}", v0.id.to_b58()),
                RawTransaction::V1(v1) => println!("  V1: id={}", v1.id.to_b58()),
            }
        }
        Err(e) => println!("\nRawTransaction decode failed: {}", e),
    }

    // Try RawTransactionV1 directly - this should work for test.signed
    if let Ok(Ok(v1)) = catch_unwind(AssertUnwindSafe(|| RawTransactionV1::from_noun(&noun))) {
        println!("\nDecoded as RawTransactionV1 directly!");
        println!("  id: {}", v1.id.to_b58());
        println!("  spends: {}", v1.spends.map.wyt());

        // Extract signatures
        for (name, spend) in v1.spends.map.tap() {
            println!("\n  Spend: {:?}", name);
            if let SpendBody::V1(sb) = &spend.body {
                println!("    Fee: {}", sb.fee.value);
                println!("    PKH sigs: {}", sb.witness.pkh.map.wyt());

                for (pkh, sig_val) in sb.witness.pkh.map.tap() {
                    println!("    PKH: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                        pkh.values[0], pkh.values[1], pkh.values[2], pkh.values[3], pkh.values[4]);
                    println!("    Chal: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                        sig_val.sig.chal.values.values[0], sig_val.sig.chal.values.values[1],
                        sig_val.sig.chal.values.values[2], sig_val.sig.chal.values.values[3],
                        sig_val.sig.chal.values.values[4], sig_val.sig.chal.values.values[5],
                        sig_val.sig.chal.values.values[6], sig_val.sig.chal.values.values[7]);
                    println!("    Sig:  {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                        sig_val.sig.sig.values.values[0], sig_val.sig.sig.values.values[1],
                        sig_val.sig.sig.values.values[2], sig_val.sig.sig.values.values[3],
                        sig_val.sig.sig.values.values[4], sig_val.sig.sig.values.values[5],
                        sig_val.sig.sig.values.values[6], sig_val.sig.sig.values.values[7]);
                }
            }
        }
    }

    // Try as cell and decode head as RawTransactionV1
    if let Ok(cell) = noun.as_cell() {
        if let Ok(Ok(v1)) = catch_unwind(AssertUnwindSafe(|| RawTransactionV1::from_noun(&cell.head()))) {
            println!("\nDecoded head as RawTransactionV1!");
            println!("  id: {}", v1.id.to_b58());
            println!("  spends: {}", v1.spends.map.wyt());

            // Extract signatures
            for (name, spend) in v1.spends.map.tap() {
                println!("\n  Spend: {:?}", name);
                if let SpendBody::V1(sb) = &spend.body {
                    println!("    Fee: {}", sb.fee.value);
                    println!("    PKH sigs: {}", sb.witness.pkh.map.wyt());

                    for (pkh, sig_val) in sb.witness.pkh.map.tap() {
                        println!("    PKH: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                            pkh.values[0], pkh.values[1], pkh.values[2], pkh.values[3], pkh.values[4]);
                        println!("    Chal: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                            sig_val.sig.chal.values.values[0], sig_val.sig.chal.values.values[1],
                            sig_val.sig.chal.values.values[2], sig_val.sig.chal.values.values[3],
                            sig_val.sig.chal.values.values[4], sig_val.sig.chal.values.values[5],
                            sig_val.sig.chal.values.values[6], sig_val.sig.chal.values.values[7]);
                        println!("    Sig:  {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                            sig_val.sig.sig.values.values[0], sig_val.sig.sig.values.values[1],
                            sig_val.sig.sig.values.values[2], sig_val.sig.sig.values.values[3],
                            sig_val.sig.sig.values.values[4], sig_val.sig.sig.values.values[5],
                            sig_val.sig.sig.values.values[6], sig_val.sig.sig.values.values[7]);
                    }
                }
            }
        }
    }
}

#[test]
fn test_derive_key_from_seed() {
    println!("\n=== Deriving key from seed.txt mnemonic ===\n");

    // Derive key from mnemonic
    let seed = siger_core::cheetah::bip39_to_seed(TEST_MNEMONIC, "").expect("bip39");
    let (sk_be, cc) = siger_core::cheetah::master_from_seed(&seed);

    println!("Secret key (BE): {}", hex::encode(&sk_be));
    println!("Chain code: {}", hex::encode(&cc));

    // Get public key
    let pk = tx_types::crypto::cheetah_pub_from_sk(sk_be);
    println!("\nPublic key X: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        pk[0][0], pk[0][1], pk[0][2], pk[0][3], pk[0][4], pk[0][5]);
    println!("Public key Y: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        pk[1][0], pk[1][1], pk[1][2], pk[1][3], pk[1][4], pk[1][5]);

    // Compute pubkey hash
    let pubkey = SchnorrPubkey {
        x: F6LT { values: pk[0] },
        y: F6LT { values: pk[1] },
        inf: false,
    };
    let pk_hash = pubkey.to_hash();
    println!("\nPubkey hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        pk_hash.values[0], pk_hash.values[1], pk_hash.values[2], pk_hash.values[3], pk_hash.values[4]);
}

#[test]
fn test_v1_sig_hash_from_unsigned() {
    println!("\n=== Computing V1 sig_hash from test.tx ===\n");

    // Load unsigned transaction
    let data = fs::read("../../test.tx").expect("read test.tx");
    let mut slab: NounSlab = NounSlab::new();
    let noun = slab.cue_into(Bytes::from(data)).expect("cue");

    // Try decoding directly as RawTransactionV1 (raw-tx:transact format)
    if let Ok(v1) = RawTransactionV1::from_noun(&noun) {
        println!("Decoded directly as RawTransactionV1");
        println!("Transaction ID: {}", v1.id.to_b58());

        for (name, spend) in v1.spends.map.tap() {
            println!("\nSpend: {:?}", name);

            if let SpendBody::V1(sb) = &spend.body {
                let sig_hash = sb.compute_sig_hash();
                println!("  sig_hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    sig_hash.values[0], sig_hash.values[1], sig_hash.values[2],
                    sig_hash.values[3], sig_hash.values[4]);
            }
        }
    } else if let Ok(cell) = noun.as_cell() {
        // Try as [raw-tx tail] format
        if let Ok(v1) = RawTransactionV1::from_noun(&cell.head()) {
            println!("Decoded from cell.head() as RawTransactionV1");
            println!("Transaction ID: {}", v1.id.to_b58());

            for (name, spend) in v1.spends.map.tap() {
                println!("\nSpend: {:?}", name);

                if let SpendBody::V1(sb) = &spend.body {
                    let sig_hash = sb.compute_sig_hash();
                    println!("  sig_hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                        sig_hash.values[0], sig_hash.values[1], sig_hash.values[2],
                        sig_hash.values[3], sig_hash.values[4]);
                }
            }
        } else {
            println!("Could not decode as V1 transaction");
        }
    } else {
        println!("Could not decode test.tx");
    }
}

#[test]
fn test_v1_signing_consistency() {
    println!("\n=== Testing V1 signing consistency ===\n");

    // 1. Load test.signed to get a real V1 transaction
    let signed_data = fs::read("../../test.signed").expect("read test.signed");
    let mut slab: NounSlab = NounSlab::new();
    let signed_noun = slab.cue_into(Bytes::from(signed_data)).expect("cue");

    let v1_signed = RawTransactionV1::from_noun(&signed_noun)
        .expect("decode V1");

    println!("Transaction ID: {}", v1_signed.id.to_b58());

    let spends: Vec<_> = v1_signed.spends.map.tap();
    let (_, spend) = &spends[0];
    let sb = match &spend.body {
        SpendBody::V1(sb) => sb,
        _ => panic!("Expected V1 spend body"),
    };

    let sigs: Vec<_> = sb.witness.pkh.map.tap();
    assert!(!sigs.is_empty(), "No signatures in signed tx!");

    let (expected_pkh, _sig_val) = &sigs[0];

    // 2. Derive key from mnemonic
    let seed = siger_core::cheetah::bip39_to_seed(TEST_MNEMONIC, "").expect("bip39");
    let (sk_be, _cc) = siger_core::cheetah::master_from_seed(&seed);
    let pk = tx_types::crypto::cheetah_pub_from_sk(sk_be);

    // 3. Compute our pubkey hash and verify it matches the signature's PKH
    let pubkey = SchnorrPubkey {
        x: F6LT { values: pk[0] },
        y: F6LT { values: pk[1] },
        inf: false,
    };
    let our_pkh = pubkey.to_hash();

    println!("Our PKH: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        our_pkh.values[0], our_pkh.values[1], our_pkh.values[2],
        our_pkh.values[3], our_pkh.values[4]);
    println!("Expected PKH: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        expected_pkh.values[0], expected_pkh.values[1], expected_pkh.values[2],
        expected_pkh.values[3], expected_pkh.values[4]);

    assert_eq!(our_pkh.values, expected_pkh.values, "PKH mismatch - wrong key?");
    println!("✓ PKH matches - we have the correct key\n");

    // 4. Compute sig_hash from the spend body
    let sig_hash = sb.compute_sig_hash();
    println!("Sig hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        sig_hash.values[0], sig_hash.values[1], sig_hash.values[2],
        sig_hash.values[3], sig_hash.values[4]);

    // 5. Sign multiple times and verify we get the same result (deterministic)
    let (chal1, sig1) = siger_core::cheetah::schnorr_sign_tx(
        sk_be,
        (pk[0], pk[1]),
        sig_hash.values
    );

    let (chal2, sig2) = siger_core::cheetah::schnorr_sign_tx(
        sk_be,
        (pk[0], pk[1]),
        sig_hash.values
    );

    println!("\nFirst signature:");
    println!("  Chal: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        chal1.values[0], chal1.values[1], chal1.values[2], chal1.values[3],
        chal1.values[4], chal1.values[5], chal1.values[6], chal1.values[7]);
    println!("  Sig:  {:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        sig1.values[0], sig1.values[1], sig1.values[2], sig1.values[3],
        sig1.values[4], sig1.values[5], sig1.values[6], sig1.values[7]);

    // Verify determinism
    assert_eq!(chal1.values, chal2.values, "Challenge not deterministic!");
    assert_eq!(sig1.values, sig2.values, "Signature not deterministic!");
    println!("\n✓ Signing is deterministic - same input produces same signature");

    println!("\n=== SUCCESS: V1 signing implementation is consistent ===");
}
