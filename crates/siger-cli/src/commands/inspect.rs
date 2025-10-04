use crate::util::{
    pretty_noun, raw_from_inputs, transaction_name_from_noun, transaction_to_raw,
};
use anyhow::{anyhow, Context};
use bytes::Bytes;
use nockapp::noun::slab::NounSlab;
use nockvm::noun::Noun;
use noun_serde::NounDecode;
use std::fs;
use tx_types::transaction_types::*;
use tx_types::RawTransaction;

fn print_raw_details(raw: &RawTransaction) {
    crate::util::print_raw_details(raw)
}

pub fn run(
    draft_path: &str,
    dump_noun: bool,
    max_depth: usize,
    max_items: usize,
) -> anyhow::Result<()> {
    let data = fs::read(draft_path).with_context(|| format!("read {draft_path}"))?;

    // Keep allocator alive while noun is in scope
    let mut slab: NounSlab = NounSlab::new();
    let noun: Noun = slab
        .cue_into(Bytes::from(data.clone()))
        .map_err(|e| anyhow!("cue failed: {e:?}"))?;

    // Basic shape
    println!("file: {draft_path}");

    if let Ok(name) = transaction_name_from_noun(&noun) {
        println!("txid: {name}");
    }

    // Try all known shapes → RawTransaction
    let raw = match RawTransaction::from_noun(&noun) {
        Ok(r) => {
            println!("detected: raw-tx:transact");
            r
        }
        Err(_) => {
            // tx:transact — head is raw
            if let Ok(cell) = noun.as_cell() {
                if let Ok(r) = RawTransaction::from_noun(&cell.head()) {
                    println!("detected: tx:transact (head is raw-tx)");
                    r
                } else {
                    // wallet transaction:wt
                    if let Ok(tx_wallet) = Transaction::from_noun(&noun) {
                        transaction_to_raw(&tx_wallet)
                    } else {
                        // bare [name inputs]
                        if let Ok(cell2) = noun.as_cell() {
                            if let Ok(inputs) = Inputs::from_noun(&cell2.tail()) {
                                raw_from_inputs(inputs)
                            } else {
                                return Err(anyhow!("unrecognized noun shape; cannot decode as any transaction form"));
                            }
                        } else {
                            return Err(anyhow!(
                                "unrecognized noun shape; not a cell and not raw-tx"
                            ));
                        }
                    }
                }
            } else {
                // not a cell; try wallet or give up
                if let Ok(tx_wallet) = Transaction::from_noun(&noun) {
                    transaction_to_raw(&tx_wallet)
                } else {
                    return Err(anyhow!(
                        "unrecognized noun shape; cannot decode as any transaction form"
                    ));
                }
            }
        }
    };

    // Typed summary
    print_raw_details(&raw);

    // Optional raw noun dump
    if dump_noun {
        println!("\n-- raw noun dump --");
        let s = pretty_noun(&noun, max_depth, max_items);
        println!("{s}");
    }

    Ok(())
}
