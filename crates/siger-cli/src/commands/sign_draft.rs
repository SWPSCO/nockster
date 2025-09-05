// src/cmd/sign_tx.rs
use std::fs;
use std::path::{Path, PathBuf};
use serialport::{SerialPort};
use anyhow::{anyhow, Context, Result};
use bytes::Bytes;

use nockapp::noun::slab::NounSlab;
use nockvm::noun::Noun;
use noun_serde::{NounDecode, NounEncode};

use tx_types::transaction_types::*;
use crate::util::{debug_shape, transaction_to_raw};
use siger_core::{PROTO_V1, Msg, Frame, Request, Response};

use crate::serial::{open, round_trip_frame};
use crate::keys;

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

/// true if zset contains an element equal to `x`
fn zset_contains<T>(zs: &tx_types::collections::ZSet<T>, x: &T) -> bool
where
    T: noun_serde::NounEncode + Clone + core::fmt::Debug + PartialEq + Ord,
{
    zs.iter().any(|t| t == x)
}

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

fn fetch_device_pks(
    sp: &mut dyn SerialPort,
    paths: &[Vec<u32>],
) -> anyhow::Result<Vec<(Vec<u32>, SchnorrPubkey)>> {
    let mut out = Vec::new();
    for path in paths {
        let req = Msg {
            v: PROTO_V1,
            id: 0x4100,
            msg: Frame::One(Request::GetCheetahPub { path: path.clone() }),
        };
        let resp: Msg<Response> = round_trip_frame(sp, &req)?;
        match resp.msg {
            Response::OkCheetahPub { x, y } => {
                out.push((
                    path.clone(),
                    SchnorrPubkey {
                        x: F6LT { values: x },
                        y: F6LT { values: y },
                        inf: false,
                    },
                ));
            }
            Response::Err { code } => return Err(anyhow!("GetCheetahPub failed (code {code})")),
            _ => return Err(anyhow!("unexpected response to GetCheetahPub")),
        }
    }
    Ok(out)
}

pub fn run(port: &str, baud: u32, draft_path: &str, out_opt: Option<&str>) -> Result<()> {
    // 1) read jam -> Transaction
    let data = fs::read(draft_path).with_context(|| format!("read {draft_path}"))?;
    let mut slab: NounSlab = NounSlab::new();
    let noun: Noun = slab
        .cue_into(Bytes::from(data.clone()))
        .map_err(|e| anyhow!("cue failed: {e:?}"))?;

    println!("file: {draft_path}");
    println!("shape: {}", debug_shape(&noun));

    let mut tx: Transaction = Transaction::from_noun(&noun)
        .map_err(|_| anyhow!("expected Transaction jam"))?;

    let raw = transaction_to_raw(&tx); // convert to RawTransaction
    let txid5 = raw.id.values;
    println!("txid: {}", raw.id.to_base58());

    let path_list: Vec<String> = std::env::var("SIGER_PATHS")
        .ok()
        .map(|s| {
            s.split(';')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_else(|| vec!["m".to_string()]);
    let signer_paths = parse_signer_paths(&path_list)?;

    // device pubs for those paths
    let mut sp = open(port, baud)?;
    let dev_keys = fetch_device_pks(&mut *sp, &signer_paths)?;

    // for each input, if its lock contains a device pk -> SignTxId -> insert sig
    let mut new_inputs = tx_types::collections::ZMap::new();

    for (name, mut input) in tx.p.p.tap() {
        let lock = &input.note.lock;
        let mut sig_map: tx_types::collections::ZMap<SchnorrPubkey, SchnorrSignature> =
            input
                .spend
                .signature
                .as_ref()
                .map(|s| s.map.clone())
                .unwrap_or_else(tx_types::collections::ZMap::new);

        for (path, pk_dev) in dev_keys.iter() {
            if !zset_contains(&lock.pubkeys, pk_dev) {
                continue;
            }

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

    tx.p = Inputs { p: new_inputs };

    // jam the result
    let mut out_slab: NounSlab = NounSlab::new();
    let tx_noun = tx.to_noun(&mut out_slab);
    out_slab.copy_into(tx_noun);
    let out_jam = out_slab.jam();

    let out_path = default_out_path_for(draft_path, out_opt);
    fs::write(&out_path, &out_jam)
        .with_context(|| format!("write {}", out_path.display()))?;

    println!("wrote {} bytes to {}", out_jam.len(), out_path.display());
    Ok(())
}
