use crate::util::{pretty_noun, transaction_name_from_noun};
use anyhow::{anyhow, Context};
use bytes::Bytes;
use nockapp::noun::slab::NounSlab;
use nockvm::noun::Noun;
use noun_serde::NounDecode;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use tx_types::transaction_types_v1::{compute_tx_id_v1, RawTransactionV1, SpendsV1};
use tx_types::RawTransaction;

#[derive(Debug, Clone, NounDecode)]
struct WalletTransactionV1 {
    pub name: String,
    pub spends: SpendsV1,
}

fn decode_no_panic<T: NounDecode>(noun: &Noun) -> Option<T> {
    catch_unwind(AssertUnwindSafe(|| T::from_noun(noun)))
        .ok()
        .and_then(|r| r.ok())
}

fn raw_v1_from_spends(spends: SpendsV1) -> RawTransaction {
    RawTransaction::V1(RawTransactionV1 {
        version: 1,
        id: compute_tx_id_v1(&spends),
        spends,
    })
}

fn decode_tx_v1_wrapper(noun: &Noun) -> Option<SpendsV1> {
    let root = noun.as_cell().ok()?;
    let tag = root.head().as_atom().ok()?.as_u64().ok()?;
    if tag != 1 {
        return None;
    }
    let t1 = root.tail().as_cell().ok()?;
    let t2 = t1.tail().as_cell().ok()?;
    decode_no_panic::<SpendsV1>(&t2.head())
}

fn bythos_raw_from_noun(noun: &Noun) -> anyhow::Result<RawTransaction> {
    if let Some(raw) = decode_no_panic::<RawTransaction>(noun) {
        return match raw {
            RawTransaction::V1(_) => Ok(raw),
            RawTransaction::V0(_) => Err(anyhow!("pre-Bythos V0 transactions are unsupported")),
        };
    }

    if let Ok(cell) = noun.as_cell() {
        if let Some(raw) = decode_no_panic::<RawTransaction>(&cell.head()) {
            return match raw {
                RawTransaction::V1(_) => Ok(raw),
                RawTransaction::V0(_) => Err(anyhow!("pre-Bythos V0 transactions are unsupported")),
            };
        }
    }

    if let Some(spends) = decode_tx_v1_wrapper(noun) {
        return Ok(raw_v1_from_spends(spends));
    }

    if let Some(wallet) = decode_no_panic::<WalletTransactionV1>(noun) {
        let computed = compute_tx_id_v1(&wallet.spends);
        if wallet.name != computed.to_b58() {
            println!(
                "wallet name differs from computed txid: {}",
                computed.to_b58()
            );
        }
        return Ok(raw_v1_from_spends(wallet.spends));
    }

    Err(anyhow!(
        "unrecognized Bythos transaction shape; expected V1 raw-tx, tx:transact, tx-v1 wrapper, or [name spends]"
    ))
}

pub fn run(
    draft_path: &str,
    dump_noun: bool,
    max_depth: usize,
    max_items: usize,
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

    if dump_noun {
        println!(
            "noun:\n{}",
            pretty_noun(&noun, max_depth.max(1), max_items.max(1))
        );
    }

    let raw = bythos_raw_from_noun(&noun)?;

    // Typed summary
    crate::util::print_raw_details(&raw);

    Ok(())
}
