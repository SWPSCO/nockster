use std::path::{Path, PathBuf};
use std::fs;

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;

use nockapp::noun::slab::NounSlab;
use nockvm::noun::Noun;
use noun_serde::NounDecode;

use tx_types::transaction_types::*;
use tx_types::RawTransaction;

use siger_core::FragKind;

use crate::serial::{open, send_blob_and_recv_outbound};
use crate::util::{debug_shape, pretty_noun, transaction_to_raw, raw_from_inputs, print_raw_details};

/// If `explicit_out` is None or Some("") -> replace extension with `.tx`.
fn default_out_path_for(draft_path: &str, explicit_out: Option<&str>) -> PathBuf {
    match explicit_out {
        Some(s) if !s.is_empty() => PathBuf::from(s),
        _ => {
            let p = Path::new(draft_path);
            let mut out = p.to_path_buf();
            out.set_extension("tx");
            out
        }
    }
}

fn decode_to_raw(data: &[u8]) -> Result<(Noun, RawTransaction)> {
    let mut slab: NounSlab = NounSlab::new();
    let noun: Noun = slab
        .cue_into(Bytes::from(data.to_vec()))
        .map_err(|e| anyhow!("cue failed: {e:?}"))?;

    // Try all known shapes → RawTransaction
    if let Ok(r) = RawTransaction::from_noun(&mut slab, &noun) {
        return Ok((noun, r));
    }
    if let Ok(cell) = noun.as_cell() {
        if let Ok(r) = RawTransaction::from_noun(&mut slab, &cell.head()) {
            return Ok((noun, r));
        }
    }
    if let Ok(tx_wallet) = Transaction::from_noun(&noun) {
        return Ok((noun, transaction_to_raw(&tx_wallet)));
    }
    if let Ok(cell2) = noun.as_cell() {
        if let Ok(inputs) = Inputs::from_noun(&cell2.tail()) {
            return Ok((noun, raw_from_inputs(inputs)));
        }
    }

    Err(anyhow!(
        "unrecognized noun shape ({}): cannot decode as transaction",
        debug_shape(&noun)
    ))
}

fn count_total_sigs(raw: &RawTransaction) -> usize {
    raw.inputs
        .p
        .tap()
        .into_iter()
        .map(|(_n, i)| i.spend.signature.as_ref().map(|m| m.map.wyt()).unwrap_or(0))
        .sum()
}

pub fn run(port: &str, baud: u32, draft_path: &str, out_opt: Option<&str>) -> Result<()> {
    // 1) read file
    let bytes = fs::read(draft_path).with_context(|| format!("read {draft_path}"))?;

    // 2) pre-decode → typed summary
    let (noun_before, mut raw_before) = decode_to_raw(&bytes)?;
    println!("file: {draft_path}");
    println!("shape: {}", debug_shape(&noun_before));

    // Ensure txid is populated (RawTransaction::new() may compute it, but be explicit)
    raw_before.id = raw_before.compute_id();
    let txid_str = raw_before.id.to_base58();
    let sigs_before = count_total_sigs(&raw_before);

    println!("txid: {}", txid_str);
    print_raw_details(&raw_before);
    println!("signatures (pre): {}", sigs_before);

    // 3) send draft to device for signing
    let mut sp = open(port, baud)?;
    let ret = send_blob_and_recv_outbound(&mut *sp, 0x99, FragKind::SignDraft, &bytes)?;

    // 4) post-decode returned bytes, if possible
    let mut wrote_bytes = false;
    let out_path = default_out_path_for(draft_path, out_opt);

    match decode_to_raw(&ret) {
        Ok((_noun_after, mut raw_after)) => {
            raw_after.id = raw_after.compute_id();
            let sigs_after = count_total_sigs(&raw_after);
            println!("received {} bytes (valid jammed tx)", ret.len());
            println!("txid (post): {}", raw_after.id.to_base58());
            println!("signatures (post): {}", sigs_after);
            if raw_after.id != raw_before.id {
                eprintln!("⚠ txid changed between pre/post (this is unusual)");
            }
            fs::write(&out_path, &ret)
                .with_context(|| format!("write {}", out_path.display()))?;
            println!("wrote {} bytes to {}", ret.len(), out_path.display());
            wrote_bytes = true;
        }
        Err(e) => {
            eprintln!("warning: returned blob did not decode as a known tx form: {e}");
            // Still write raw bytes so you can inspect
            fs::write(&out_path, &ret)
                .with_context(|| format!("write {}", out_path.display()))?;
            println!("wrote {} bytes to {}", ret.len(), out_path.display());
            wrote_bytes = true;
        }
    }

    // 5) optionally dump raw noun (like inspect) — keep off by default, show hint
    if !wrote_bytes {
        println!("received {} bytes:", ret.len());
        println!("{}", hex::encode(&ret));
    }

    Ok(())
}
