/// Compare jack-test.tx (siger-signed) with nw.known-good.tx (wallet-signed)
/// These should have IDENTICAL signatures since they're the same transaction
use bytes::Bytes;
use nockapp::noun::slab::NounSlab;
use nockvm::noun::Noun;
use noun_serde::NounDecode;
use std::fs;
use tx_types::transaction_types::*;
use tx_types::transaction_types_v0::*;
use tx_types::RawTransaction;

fn load_tx(path: &str) -> RawTransactionV0 {
    let data = fs::read(path).expect(&format!("read {}", path));
    let mut slab: NounSlab = NounSlab::new();
    let noun: Noun = slab.cue_into(Bytes::from(data)).expect("cue");

    // Try different forms
    if let Ok(raw) = RawTransaction::from_noun(&noun) {
        eprintln!("  {} decoded as bare RawTransaction", path);
        return match raw {
            RawTransaction::V0(v0) => v0,
            RawTransaction::V1(_) => panic!("V1 not supported"),
        };
    }

    // Try [raw-tx tail] form (tx:transact)
    if let Ok(cell) = noun.as_cell() {
        if let Ok(raw) = RawTransaction::from_noun(&cell.head()) {
            eprintln!("  {} decoded as [raw-tx tail]", path);
            return match raw {
                RawTransaction::V0(v0) => v0,
                RawTransaction::V1(_) => panic!("V1 not supported"),
            };
        }

        // Try wallet transaction form [name inputs]
        if let Ok(tx_wallet) = Transaction::from_noun(&noun) {
            eprintln!("  {} decoded as Transaction (wallet form)", path);
            let inputs_v0 = match &tx_wallet.p {
                Inputs::V0(v0) => v0.clone(),
                Inputs::V1(_) => panic!("V1 not supported"),
            };
            let total_fees: u64 = inputs_v0
                .p
                .tap()
                .iter()
                .map(|(_, input)| input.spend.fee.value)
                .sum();
            return RawTransactionV0 {
                id: Hash { values: [0; 5] },
                inputs: inputs_v0,
                timelock_range: TimelockRange {
                    min: None,
                    max: None,
                },
                total_fees: Coins { value: total_fees },
            };
        }
    }

    panic!("Could not decode transaction from {}", path);
}

#[test]
fn test_compare_known_good_signatures() {
    println!("\n{}", "=".repeat(80));
    println!("COMPARING KNOWN-GOOD SIGNATURES");
    println!("{}\n", "=".repeat(80));

    let jack = load_tx("../../jack-test.tx");
    let nw = load_tx("../../nw.known-good.tx");

    println!("jack-test.tx (siger-signed):");
    println!("  Transaction ID: {}", jack.id.to_b58());

    println!("\nnw.known-good.tx (wallet-signed):");
    println!("  Transaction ID: {}", nw.id.to_b58());

    if jack.id.to_b58() == nw.id.to_b58() {
        println!("\n✓ Transaction IDs match - these are the same transaction");
    } else {
        println!("\n⚠ Transaction IDs differ - these might be different transactions");
    }

    let jack_inputs: Vec<_> = jack.inputs.p.tap();
    let nw_inputs: Vec<_> = nw.inputs.p.tap();

    for (i, ((jack_name, jack_input), (nw_name, nw_input))) in
        jack_inputs.iter().zip(nw_inputs.iter()).enumerate()
    {
        println!("\nInput {}:", i);

        if let (Some(jack_sig_map), Some(nw_sig_map)) =
            (&jack_input.spend.signature, &nw_input.spend.signature)
        {
            let jack_sigs: Vec<_> = jack_sig_map.map.tap();
            let nw_sigs: Vec<_> = nw_sig_map.map.tap();

            for (j, ((jack_pk, jack_sig), (nw_pk, nw_sig))) in
                jack_sigs.iter().zip(nw_sigs.iter()).enumerate()
            {
                println!("  Signature {}:", j);

                if jack_sig.chal.values.values == nw_sig.chal.values.values {
                    println!("    ✓ Challenges match");
                    println!("      Chal: {:08x?}", jack_sig.chal.values.values);
                } else {
                    println!("    ❌ CHALLENGES DIFFER");
                    println!("      Jack: {:08x?}", jack_sig.chal.values.values);
                    println!("      NW:   {:08x?}", nw_sig.chal.values.values);
                }

                if jack_sig.sig.values.values == nw_sig.sig.values.values {
                    println!("    ✓ Signatures match");
                    println!("      Sig: {:08x?}", jack_sig.sig.values.values);
                } else {
                    println!("    ❌ SIGNATURES DIFFER");
                    println!("      Jack: {:08x?}", jack_sig.sig.values.values);
                    println!("      NW:   {:08x?}", nw_sig.sig.values.values);
                }
            }
        }
    }

    println!("\n{}", "=".repeat(80));
}
