extern crate alloc;
use crate::util::{enumerate_signing_plans, fmt_u64x5, load_draft_as_raw};
use std::path::Path;
use tx_types::{NName, SchnorrPubkey};

pub struct InputSigningPlan {
    pub name: NName,
    pub m: u64,
    pub combos: alloc::vec::Vec<alloc::vec::Vec<SchnorrPubkey>>,
}

pub fn run(_port: &str, _baud: u32, draft_path: &str) -> anyhow::Result<()> {
    let raw = load_draft_as_raw(Path::new(draft_path))?;

    let id_str = fmt_u64x5(&raw.id.values);
    let inputs_count = raw.inputs.p.wyt();
    let tl_min = raw.timelock_range.min.as_ref().map(|p| p.value);
    let tl_max = raw.timelock_range.max.as_ref().map(|p| p.value);
    let fee = raw.total_fees.value;

    println!(
        "draft: id={}, inputs={}, timelock=[{:?}, {:?}], total_fees={}",
        id_str, inputs_count, tl_min, tl_max, fee
    );

    let plans = enumerate_signing_plans(&raw.inputs);
    for p in plans {
        let total_keys = raw
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
