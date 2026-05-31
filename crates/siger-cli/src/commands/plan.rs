extern crate alloc;
use crate::util::{enumerate_signing_plans, fmt_u64x5, load_draft_as_raw};
use anyhow::anyhow;
use std::path::Path;
use tx_types::{NName, RawTransaction, SchnorrPubkey};

pub struct InputSigningPlan {
    pub name: NName,
    pub m: u64,
    pub combos: alloc::vec::Vec<alloc::vec::Vec<SchnorrPubkey>>,
}

pub fn run(_port: &str, _baud: u32, draft_path: &str) -> anyhow::Result<()> {
    let raw = load_draft_as_raw(Path::new(draft_path))?;

    // Only V0 transactions have the signing plan structure
    let v0 = match &raw {
        RawTransaction::V0(v0) => v0,
        RawTransaction::V1(_) => {
            return Err(anyhow!(
                "signing plans are only supported for V0 transactions"
            ))
        }
    };

    let id_str = fmt_u64x5(&v0.id.values);
    let inputs_count = v0.inputs.p.wyt();
    let tl_min = v0.timelock_range.min.as_ref().map(|p| p.value);
    let tl_max = v0.timelock_range.max.as_ref().map(|p| p.value);
    let fee = v0.total_fees.value;

    println!(
        "draft: id={}, inputs={}, timelock=[{:?}, {:?}], total_fees={}",
        id_str, inputs_count, tl_min, tl_max, fee
    );

    let plans = enumerate_signing_plans(&v0.inputs);
    for p in plans {
        let total_keys = v0
            .inputs
            .p
            .tap()
            .iter()
            .find(|(n, _)| *n == p.name)
            .map(|(_, i)| i.note.lock.pubkeys.wyt())
            .unwrap_or(0);
        println!(
            "input {:?}: m-of-n = {} of {}, combos={}",
            p.name.to_hash().to_b58(),
            p.m,
            total_keys,
            p.combos.len()
        );
        for (i, combo) in p.combos.iter().enumerate() {
            println!("  combo#{i}: {} keys", combo.len());
        }
    }
    Ok(())
}
