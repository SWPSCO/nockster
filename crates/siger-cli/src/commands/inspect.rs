use crate::util::{pretty_noun, raw_from_inputs_v0, transaction_name_from_noun, transaction_to_raw};
use anyhow::{anyhow, Context};
use bytes::Bytes;
use nockapp::noun::slab::NounSlab;
use nockvm::noun::Noun;
use noun_serde::NounDecode;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use tx_types::transaction_types::{Transaction, Inputs};
use tx_types::transaction_types_v0::InputsV0;
use tx_types::RawTransaction;

fn print_raw_details(raw: &RawTransaction) {
    crate::util::print_raw_details(raw)
}

pub fn run(
    draft_path: &str,
    _dump_noun: bool,
    _max_depth: usize,
    _max_items: usize,
) -> anyhow::Result<()> {
    let data = fs::read(draft_path).with_context(|| format!("read {draft_path}"))?;
    let mut slab: NounSlab = NounSlab::new();
    let noun: Noun = slab
        .cue_into(Bytes::from(data.clone()))
        .map_err(|e| anyhow!("cue failed: {e:?}"))?;

    println!("file: {draft_path}");

    if let Ok(name) = transaction_name_from_noun(&noun) {
        println!("txid: {name}");
    }

    // Try all known shapes → RawTransaction
    // Helper to safely try Transaction::from_noun (can panic on invalid data)
    let try_wallet_tx = |n: &Noun| -> Option<Transaction> {
        catch_unwind(AssertUnwindSafe(|| Transaction::from_noun(n)))
            .ok()
            .and_then(|r| r.ok())
    };

    let raw = match catch_unwind(AssertUnwindSafe(|| RawTransaction::from_noun(&noun))) {
        Ok(Ok(r)) => {
            println!("detected: raw-tx:transact");
            r
        }
        _ => {
            // tx:transact — head is raw
            if let Ok(cell) = noun.as_cell() {
                let head_raw = catch_unwind(AssertUnwindSafe(|| RawTransaction::from_noun(&cell.head())));
                if let Ok(Ok(r)) = head_raw {
                    r
                } else {
                    // wallet transaction:wt
                    if let Some(tx_wallet) = try_wallet_tx(&noun) {
                        RawTransaction::V0(transaction_to_raw(&tx_wallet))
                    } else {
                        // bare [name inputs] - V0 only
                        if let Ok(cell2) = noun.as_cell() {
                            let inputs_result = catch_unwind(AssertUnwindSafe(|| InputsV0::from_noun(&cell2.tail())));
                            if let Ok(Ok(inputs)) = inputs_result {
                                RawTransaction::V0(raw_from_inputs_v0(inputs))
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
                if let Some(tx_wallet) = try_wallet_tx(&noun) {
                    RawTransaction::V0(transaction_to_raw(&tx_wallet))
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

    Ok(())
}
