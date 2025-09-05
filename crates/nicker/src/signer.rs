use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;

use nockapp::noun::slab::NounSlab;
use nockvm::noun::Noun;
use noun_serde::{NounDecode, NounEncode};

use tx_types::transaction_types::*;
use tx_types::RawTransaction;

use siger_core::{PROTO_V1, Msg, Frame, Request, Response};
use crate::serial::{open, round_trip_frame};
use crate::keys; // for parse_path()
use crate::util::{
    debug_shape, print_raw_details, transaction_to_raw, raw_from_inputs,
    sum_inputs_fees, union_inputs_timelock_range,
};

/// Wrapper shape we must preserve when we write the signed file.
enum WrapperShape {
    RawOnly,
    TxTransact { tail: Noun },          // [ raw-tx  ..tail ]
    WalletTx { tx: Transaction },       // transaction:wt
    NameInputsPair { name_head: Noun }, // [ name  inputs ]
}

/// `--out` handling:
/// - None or Some("") -> `<input>.tx`
/// - Some("...")      -> exact path
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

/// Decode the incoming jam into a `RawTransaction`, also returning the original wrapper shape.
fn decode_shape_and_raw(bytes: &[u8]) -> Result<(WrapperShape, RawTransaction, Noun)> {
    let mut slab: NounSlab = NounSlab::new();
    let noun: Noun = slab
        .cue_into(Bytes::from(bytes.to_vec()))
        .map_err(|e| anyhow!("cue failed: {e:?}"))?;

    // 1) raw-tx directly
    if let Ok(raw) = RawTransaction::from_noun(&mut slab, &noun) {
        return Ok((WrapperShape::RawOnly, raw, noun));
    }

    // 2) tx:transact — head is raw
    if let Ok(cell) = noun.as_cell() {
        if let Ok(raw) = RawTransaction::from_noun(&mut slab, &cell.head()) {
            let tail = cell.tail();
            return Ok((WrapperShape::TxTransact { tail }, raw, noun));
        }
    }

    // 3) wallet wrapper (transaction:wt)
    if let Ok(tx_wallet) = Transaction::from_noun(&noun) {
        let raw = transaction_to_raw(&tx_wallet);
        return Ok((WrapperShape::WalletTx { tx: tx_wallet }, raw, noun));
    }

    // 4) [name inputs] pair
    if let Ok(cell) = noun.as_cell() {
        if let Ok(inputs) = Inputs::from_noun(&cell.tail()) {
            let raw = raw_from_inputs(inputs);
            let name_head = cell.head();
            return Ok((WrapperShape::NameInputsPair { name_head }, raw, noun));
        }
    }

    Err(anyhow!(
        "unrecognized noun shape ({}): cannot decode as transaction",
        debug_shape(&noun)
    ))
}

/// Build a jam preserving original wrapper, but with `new_inputs`.
fn rebuild_preserving_shape(shape: &WrapperShape, new_inputs: Inputs) -> Result<Vec<u8>> {
    // Reconstitute a RawTransaction from the new inputs (recompute fees/timelock/id)
    let mut new_raw = RawTransaction::new(
        new_inputs.clone(),
        union_inputs_timelock_range(&new_inputs),
        Coins { value: sum_inputs_fees(&new_inputs) },
    );
    new_raw.id = new_raw.compute_id();

    let mut slab = NounSlab::new();
    let jam = match shape {
        WrapperShape::RawOnly => {
            let n = new_raw.to_noun(&mut slab);
            slab.copy_into(n);
            slab.jam().to_vec()
        }
        WrapperShape::TxTransact { tail } => {
            let head = new_raw.to_noun(&mut slab);
            let tail_copied = slab.copy_into_noun(*tail);
            let pair = slab.cell(head, tail_copied);
            slab.copy_into(pair);
            slab.jam().to_vec()
        }
        WrapperShape::WalletTx { tx } => {
            let mut t = tx.clone();
            t.p = new_inputs;
            let n = t.to_noun(&mut slab);
            slab.copy_into(n);
            slab.jam().to_vec()
        }
        WrapperShape::NameInputsPair { name_head } => {
            let name = slab.copy_into_noun(*name_head);
            let ins  = new_inputs.to_noun(&mut slab);
            let pair = slab.cell(name, ins);
            slab.copy_into(pair);
            slab.jam().to_vec()
        }
    };
    Ok(jam)
}

/// Cache (path, pubkey) from device so we don’t ask twice.
fn fetch_device_pks(sp: &mut dyn crate::serial::RW, paths: &[Vec<u32>]) -> Result<Vec<(Vec<u32>, SchnorrPubkey)>> {
    let mut out = Vec::new();
    for path in paths {
        let req = Msg {
            v: PROTO_V1,
            id: 0x4100,
            msg: Frame::One(Request::GetCheetahPub { path: path.clone() }),
        };
        let resp: Msg<Response> = crate::serial::round_trip_frame(sp, &req)?;
        match resp.msg {
            Response::OkCheetahPub { x, y } => {
                out.push((
                    path.clone(),
                    SchnorrPubkey { x: F6LT { values: x }, y: F6LT { values: y }, inf: false },
                ));
            }
            Response::Err { code } => return Err(anyhow!("GetCheetahPub failed (code {code})")),
            _ => return Err(anyhow!("unexpected response to GetCheetahPub")),
        }
    }
    Ok(out)
}

/// True if ZSet contains an element equal to `x`.
fn zset_contains<T>(zs: &tx_types::collections::ZSet<T>, x: &T) -> bool
where
    T: noun_serde::NounEncode + Clone + core::fmt::Debug + PartialEq + Ord,
{
    zs.iter().any(|t| t == x)
}

/// Parse user-provided `--path` arguments into Vec<Vec<u32>>.
/// Accepts “m/44'/0'/0'/0/0” or “m” or empty => m
fn parse_signer_paths(path_args: &[String]) -> Result<Vec<Vec<u32>>> {
    if path_args.is_empty() {
        return Ok(vec![Vec::<u32>::new()]); // default "m"
    }
    let mut out = Vec::new();
    for s in path_args {
        let v = keys::parse_path(s).map_err(|e| anyhow!(e))?;
        out.push(v);
    }
    Ok(out)
}

/// The main entry point used by the CLI subcommand.
pub fn run(port: &str, baud: u32, draft_path: &str, out_opt: Option<&str>) -> Result<()> {
    // 1) Read + decode incoming jam, remember shape
    let in_bytes = fs::read(draft_path).with_context(|| format!("read {draft_path}"))?;
    let (shape, mut raw, noun_before) = decode_shape_and_raw(&in_bytes)?;
    raw.id = raw.compute_id();

    println!("file: {draft_path}");
    println!("shape: {}", debug_shape(&noun_before));
    println!("txid (pre): {}", raw.id.to_base58());
    print_raw_details(&raw);

    // 2) Figure out which derivation paths we should try (from CLI env var or config).
    //    Here we read them from SIGER_PATHS env or default to just "m".
    //    If you already pass them on the CLI, replace this with args.
    let path_list: Vec<String> = std::env::var("SIGER_PATHS")
        .ok()
        .map(|s| s.split(';').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect())
        .unwrap_or_else(|| vec!["m".to_string()]);
    let signer_paths = parse_signer_paths(&path_list)?;

    // 3) Open device, cache device pubkeys for all candidate paths
    let mut sp = open(port, baud)?;
    let dev_keys = fetch_device_pks(&mut *sp, &signer_paths)?;

    // 4) Build signatures: for each input, if its lock contains a device pk, ask device to sign txid
    let txid5 = raw.id.values;
    let mut new_inputs = tx_types::collections::ZMap::new();

    for (name, mut input) in raw.inputs.p.tap() {
        let lock_pks = &input.note.lock.pubkeys;

        // ensure we have a signature map (ZMap<SchnorrPubkey, SchnorrSignature>)
        let mut sig_map: tx_types::collections::ZMap<SchnorrPubkey, SchnorrSignature> =
            input.spend.signature.as_ref().map(|s| s.map.clone()).unwrap_or_else(tx_types::collections::ZMap::new);

        for (path, pk_dev) in dev_keys.iter() {
            if !zset_contains(lock_pks, pk_dev) {
                continue;
            }

            // Ask device to sign this txid for this path
            let req = Msg {
                v: PROTO_V1,
                id: 0x4200,
                msg: Frame::One(Request::SignTxId { path: path.clone(), txid5 }),
            };
            let resp: Msg<Response> = round_trip_frame(&mut *sp, &req)?;
            let (chal, sig) = match resp.msg {
                Response::OkCheetahSig { chal, sig } => (chal, sig),
                Response::Err { code } => return Err(anyhow!("SignTxId failed (code {code})")),
                _ => return Err(anyhow!("unexpected response to SignTxId")),
            };

            let schnorr_sig = SchnorrSignature {
                chal: Chal { values: T8 { values: chal } },
                sig:  Sig  { values: T8 { values: sig  } },
            };
            sig_map.put(pk_dev.clone(), schnorr_sig);
        }

        if sig_map.wyt() > 0 {
            input.spend.signature = Some(Signature { map: sig_map });
        }
        new_inputs.put(name, input);
    }

    // 5) Rewrap the signed Inputs back into the original wrapper, jam, write
    let signed_inputs = Inputs { p: new_inputs };
    let out_jam = rebuild_preserving_shape(&shape, signed_inputs)?;

    let out_path = default_out_path_for(draft_path, out_opt);
    fs::write(&out_path, &out_jam)
        .with_context(|| format!("write {}", out_path.display()))?;

    // 6) quick post sanity
    if let Ok((_, raw_after, _)) = decode_shape_and_raw(&out_jam) {
        println!("txid (post): {}", raw_after.id.to_base58());
        let pre = count_total_sigs(&raw);
        let post = count_total_sigs(&raw_after);
        println!("signatures: {pre} -> {post}");
    }

    println!("wrote {} bytes to {}", out_jam.len(), out_path.display());
    Ok(())
}

fn count_total_sigs(raw: &RawTransaction) -> usize {
    raw.inputs
        .p
        .tap()
        .into_iter()
        .map(|(_n, i)| i.spend.signature.as_ref().map(|m| m.map.wyt()).unwrap_or(0))
        .sum()
}
