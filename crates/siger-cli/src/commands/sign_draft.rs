use std::fs;
use std::path::{Path, PathBuf};
use std::panic::{catch_unwind, AssertUnwindSafe};

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use nockapp::noun::slab::NounSlab;
use nockvm::noun::{Noun, T};
use noun_serde::{NounDecode, NounEncode};

use siger_core::{FragKind, Request, Response};
use tx_types::transaction_types_v1::{compute_tx_id_v1, RawTransactionV1, SpendsV1};

use crate::serial::{open, send_blob_and_recv_outbound, send_call};
use crate::util::transaction_name_from_bytes;

fn default_out_path_for(input: &str, explicit: Option<&str>) -> PathBuf {
    match explicit {
        Some(s) if !s.is_empty() => PathBuf::from(s),
        _ => {
            let p = Path::new(input);
            let mut out = p.to_path_buf();
            out.set_extension("tx");
            out
        }
    }
}

#[derive(Debug, Clone, NounDecode, NounEncode)]
struct WalletTransactionV1 {
    pub name: String,
    pub spends: SpendsV1,
}

fn decode_no_panic<T: NounDecode>(noun: &Noun) -> Option<T> {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let decoded = catch_unwind(AssertUnwindSafe(|| T::from_noun(noun)));
    std::panic::set_hook(default_hook);
    decoded.ok().and_then(|r| r.ok())
}

fn rewrite_txid_v1_host(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut slab_in: NounSlab = NounSlab::new();
    let noun: Noun = slab_in
        .cue_into(Bytes::from(bytes.to_vec()))
        .map_err(|e| anyhow!("cue failed: {e:?}"))?;

    // wallet transaction wrapper: [name spends]
    if let Some(wallet) = decode_no_panic::<WalletTransactionV1>(&noun) {
        let computed_id = compute_tx_id_v1(&wallet.spends);
        let name = computed_id.to_b58();
        if wallet.name == name {
            return Ok(bytes.to_vec());
        }
        let mut slab_out: NounSlab = NounSlab::new();
        let updated = WalletTransactionV1 {
            name,
            spends: wallet.spends,
        };
        let out_noun = updated.to_noun(&mut slab_out);
        slab_out.set_root(out_noun);
        return Ok(slab_out.jam().to_vec());
    }

    // tx:transact wrapper: [raw-tx tail]
    if let Ok(cell) = noun.as_cell() {
        if let Some(mut raw) = decode_no_panic::<RawTransactionV1>(&cell.head()) {
            let computed_id = compute_tx_id_v1(&raw.spends);
            if raw.id == computed_id {
                return Ok(bytes.to_vec());
            }
            raw.id = computed_id;
            let mut slab_out: NounSlab = NounSlab::new();
            let head_noun = raw.to_noun(&mut slab_out);
            let tail_copied = slab_out.copy_into(cell.tail());
            let wrapped = T(&mut slab_out, &[head_noun, tail_copied]);
            slab_out.set_root(wrapped);
            return Ok(slab_out.jam().to_vec());
        }
    }

    // bare raw-tx
    if let Some(mut raw) = decode_no_panic::<RawTransactionV1>(&noun) {
        let computed_id = compute_tx_id_v1(&raw.spends);
        if raw.id == computed_id {
            return Ok(bytes.to_vec());
        }
        raw.id = computed_id;
        let mut slab_out: NounSlab = NounSlab::new();
        let out_noun = raw.to_noun(&mut slab_out);
        slab_out.set_root(out_noun);
        return Ok(slab_out.jam().to_vec());
    }

    Err(anyhow!(
        "host tx-id rewrite failed: output is not a V1 wallet tx, tx:transact, or raw-tx"
    ))
}

pub fn run(
    port: &str,
    baud: u32,
    draft_path: &str,
    out_opt: Option<&str>,
    slot: u8,
    host_txid: bool,
) -> Result<()> {
    let draft_bytes = fs::read(draft_path).with_context(|| format!("read {draft_path}"))?;

    let mut sp = open(port, baud)?;

    // Select seed slot (defaults to 0).
    match send_call(&mut *sp, 0x5001, Request::SelectSeed { slot })? {
        Response::Ok => {}
        Response::Err { code } => return Err(anyhow!("SelectSeed failed with code {code}")),
        other => return Err(anyhow!("unexpected SelectSeed response: {other:?}")),
    }

    let mut out_bytes = send_blob_and_recv_outbound(&mut *sp, 0xD001, FragKind::SignDraft, &draft_bytes)
        .context("device SignDraft")?;

    if host_txid {
        out_bytes = rewrite_txid_v1_host(&out_bytes)?;
        if let Ok(name) = transaction_name_from_bytes(&out_bytes) {
            println!("host tx-id: {name}");
        }
    }

    let out_path = default_out_path_for(draft_path, out_opt);
    fs::write(&out_path, &out_bytes).with_context(|| format!("write {}", out_path.display()))?;

    println!(
        "wrote {} bytes to {}",
        out_bytes.len(),
        out_path.display()
    );
    Ok(())
}
