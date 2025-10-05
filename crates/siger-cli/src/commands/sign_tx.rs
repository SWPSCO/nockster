// here's where the magic happens!!
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serialport::SerialPort;

use nockapp::noun::slab::NounSlab;
use nockvm::noun::{Noun, T};
use noun_serde::{NounDecode, NounEncode};

use siger_core::{Frame, Msg, Request, Response, PROTO_V1};
use tx_types::collections::{ZMap, ZSet};
use tx_types::transaction_types::*;
use tx_types::RawTransaction;

use crate::keys;
use crate::serial::{open, round_trip_frame};
use crate::util::{
    fmt_u64x5, sig_hash_for_input, t8_from_device, transaction_name_from_bytes,
    transaction_name_from_noun, transaction_to_raw,
};

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

fn parse_signer_paths(path_args: &[String]) -> Result<Vec<Vec<u32>>> {
    if path_args.is_empty() {
        // default to master node "m"
        return Ok(vec![Vec::<u32>::new()]);
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
) -> Result<Vec<(Vec<u32>, SchnorrPubkey)>> {
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
                let pk = SchnorrPubkey {
                    x: F6LT { values: x },
                    y: F6LT { values: y },
                    inf: false,
                };
                out.push((path.clone(), pk));
            }
            Response::Err { code } => return Err(anyhow!("GetCheetahPub failed (code {code})")),
            _ => return Err(anyhow!("unrecognized response to GetCheetahPub")),
        }
    }
    Ok(out)
}

enum Outer {
    RawTx,
    TxTransact { tail_jam: Vec<u8> },
    WalletTx(Transaction),
}

fn detect_outer(bytes: &[u8]) -> Result<(Outer, RawTransaction, Noun)> {
    let mut slab: NounSlab = NounSlab::new();
    let noun: Noun = slab
        .cue_into(Bytes::from(bytes.to_vec()))
        .map_err(|e| anyhow!("cue failed: {e:?}"))?;

    // try wallet transaction
    if let Ok(tx) = Transaction::from_noun(&noun) {
        let raw = transaction_to_raw(&tx);
        return Ok((Outer::WalletTx(tx), raw, noun));
    }

    // try [raw-tx tail]
    if let Ok(cell) = noun.as_cell() {
        if let Ok(r) = RawTransaction::from_noun(&cell.head()) {
            // capture tail as jam for perfect round-trip later
            let mut s2: NounSlab = NounSlab::new();
            let copied_tail = s2.copy_into(cell.tail());
            s2.copy_into(copied_tail);
            let tail_jam = s2.jam().to_vec();
            return Ok((Outer::TxTransact { tail_jam }, r, noun));
        }
    }

    // try bare raw-tx
    if let Ok(r) = RawTransaction::from_noun(&noun) {
        return Ok((Outer::RawTx, r, noun));
    }

    Err(anyhow!(
        "unrecognized noun shape; cannot decode as wallet transaction, [raw-tx tail], or raw-tx"
    ))
}

fn run_device(port: &str, baud: u32, draft_path: &str, out_opt: Option<&str>) -> Result<()> {
    let in_bytes = fs::read(draft_path).with_context(|| format!("read {draft_path}"))?;
    let (outer, raw, noun_before) = detect_outer(&in_bytes)?;

    let tx_name_before =
        transaction_name_from_noun(&noun_before).unwrap_or_else(|_| raw.id.to_b58());

    println!("file:  {draft_path}");
    println!("txid:  {tx_name_before}");

    // collect desired signer derivation paths
    let path_list: Vec<String> = std::env::var("SIGER_PATHS")
        .or_else(|_| std::env::var("SIGER_PATH"))
        .ok()
        .map(|s| {
            s.split(|c| c == ';' || c == ',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_else(|| vec!["m".to_string()]);
    let signer_paths = parse_signer_paths(&path_list)?;

    // fetch device pubkeys for the requested paths
    let mut sp = open(port, baud)?;
    let dev_keys = fetch_device_pks(&mut *sp, &signer_paths)?;
    println!("device keys: {}", dev_keys.len());

    // sign each input whose lock contains a device key
    let mut new_inputs: ZMap<NName, Input> = ZMap::new();
    let mut signed_count = 0usize;

    for (name, mut input) in raw.inputs.p.tap() {
        let lock = &input.note.lock;

        // reuse or create signature map
        let mut sig_map: ZMap<SchnorrPubkey, SchnorrSignature> = input
            .spend
            .signature
            .as_ref()
            .map(|s| s.map.clone())
            .unwrap_or_else(ZMap::new);

        // match by pubkey
        for (path, pk_dev) in dev_keys.iter() {
            let dev_hash = pk_dev.to_hash();
            if let Some(pk_lock) = lock.pubkeys.iter().find(|pk| pk.to_hash() == dev_hash) {
                // ask device to sign the tx sig hash for this path
                let msg_hash = sig_hash_for_input(&raw, &name);
                let msg5: [u64; 5] = msg_hash.values;
                println!("    signing hash for {:?}: {}", name, fmt_u64x5(&msg5));
                let req = Msg {
                    v: PROTO_V1,
                    id: 0x4200,
                    msg: Frame::One(Request::SignSpendHash {
                        path: path.clone(),
                        msg5,
                    }),
                };
                let resp: Msg<Response> = round_trip_frame(&mut *sp, &req)?;
                let (chal_words, sig_words) = match resp.msg {
                    Response::OkCheetahSig { chal, sig } => (chal, sig),
                    Response::Err { code } => {
                        return Err(anyhow!("SignSpendHash failed (code {code})"))
                    }
                    _ => return Err(anyhow!("unexpected response to SignSpendHash")),
                };

                let schnorr_sig = SchnorrSignature {
                    chal: Chal {
                        values: t8_from_device(chal_words),
                    },
                    sig: Sig {
                        values: t8_from_device(sig_words),
                    },
                };

                // attach signature keyed by the lock's pubkey object
                sig_map.put(pk_lock.clone(), schnorr_sig);
                signed_count += 1;
            }
        }

        if sig_map.wyt() > 0 {
            input.spend.signature = Some(Signature { map: sig_map });
        }

        new_inputs.put(name, input);
    }

    let mut updated = raw.clone();
    updated.inputs = Inputs { p: new_inputs };

    // recalculate the transaction ID with the signed inputs
    updated.id = tx_types::hashing::tx_id::compute_tx_id(
        &updated.inputs,
        &updated.timelock_range,
        updated.total_fees,
    );

    let out_bytes = match outer {
        Outer::RawTx => {
            let mut out_slab: NounSlab = NounSlab::new();
            let n = updated.to_noun(&mut out_slab);
            out_slab.copy_into(n);
            out_slab.jam()
        }
        Outer::TxTransact { tail_jam } => {
            // tx:transact = [raw-tx tail]
            let mut out_slab: NounSlab = NounSlab::new();
            let head = updated.to_noun(&mut out_slab);
            let tail = out_slab
                .cue_into(Bytes::from(tail_jam))
                .expect("cue original tail");
            let cell = T(&mut out_slab, &[head, tail]);
            out_slab.copy_into(cell);
            out_slab.jam()
        }
        Outer::WalletTx(mut tx) => {
            // wallet transaction wrapper [name=@t p=inputs]
            tx.p = updated.inputs.clone();
            let mut out_slab: NounSlab = NounSlab::new();
            let n = tx.to_noun(&mut out_slab);
            out_slab.copy_into(n);
            out_slab.jam()
        }
    };

    // write output
    let out_path = default_out_path_for(draft_path, out_opt);
    fs::write(&out_path, &out_bytes).with_context(|| format!("write {}", out_path.display()))?;

    let tx_name_after =
        transaction_name_from_bytes(&out_bytes).unwrap_or_else(|_| updated.id.to_b58());
    if tx_name_after != tx_name_before {
        eprintln!(
            "warning: tx identifier changed after attaching signatures ({} -> {})",
            tx_name_before, tx_name_after
        );
    }

    println!(
        "wrote {} bytes to {} (added {} signature{})",
        out_bytes.len(),
        out_path.display(),
        signed_count,
        if signed_count == 1 { "" } else { "s" }
    );

    Ok(())
}

// signature data structures for json deserialization
#[derive(Deserialize, Serialize)]
struct SignatureDataJson {
    input_name: String,
    pubkey_x: Vec<String>,
    pubkey_y: Vec<String>,
    chal: Vec<String>,
    sig: Vec<String>,
}

impl SignatureDataJson {
    fn parse(&self) -> Result<SignatureData> {
        let parse_array = |v: &[String], expected: usize, name: &str| -> Result<Vec<u64>> {
            if v.len() != expected {
                return Err(anyhow!(
                    "{} has {} elements, expected {}",
                    name,
                    v.len(),
                    expected
                ));
            }
            v.iter()
                .map(|s| {
                    s.parse::<u64>()
                        .context(format!("Failed to parse {}", name))
                })
                .collect()
        };

        let pubkey_x = parse_array(&self.pubkey_x, 6, "pubkey_x")?;
        let pubkey_y = parse_array(&self.pubkey_y, 6, "pubkey_y")?;
        let chal = parse_array(&self.chal, 8, "chal")?;
        let sig = parse_array(&self.sig, 8, "sig")?;

        Ok(SignatureData {
            input_name: self.input_name.clone(),
            pubkey_x: pubkey_x
                .try_into()
                .map_err(|_| anyhow!("pubkey_x wrong length"))?,
            pubkey_y: pubkey_y
                .try_into()
                .map_err(|_| anyhow!("pubkey_y wrong length"))?,
            chal: chal.try_into().map_err(|_| anyhow!("chal wrong length"))?,
            sig: sig.try_into().map_err(|_| anyhow!("sig wrong length"))?,
        })
    }
}

struct SignatureData {
    input_name: String,
    pubkey_x: [u64; 6],
    pubkey_y: [u64; 6],
    chal: [u64; 8],
    sig: [u64; 8],
}

fn nname_b58(name: &NName) -> String {
    let first = name.p.get(0).map(|h| h.to_b58()).unwrap_or_default();
    let last = name.p.get(1).map(|h| h.to_b58()).unwrap_or_default();
    let no_q = |s: String| s.trim_matches('\"').to_string();
    let (first, last) = (no_q(first), no_q(last));
    if last.is_empty() {
        first
    } else {
        format!("{first} {last}")
    }
}

fn run_apply_signatures(draft_path: &str, out_opt: Option<&str>, sig_path: &str) -> Result<()> {
    println!("Loading draft: {}", draft_path);
    let draft_bytes = fs::read(draft_path).context("Failed to read draft file")?;

    println!("Loading signatures: {}", sig_path);
    let sig_json = fs::read_to_string(sig_path).context("Failed to read signatures file")?;
    let sigs_json: Vec<SignatureDataJson> =
        serde_json::from_str(&sig_json).context("Failed to parse signatures json")?;

    let sigs: Vec<SignatureData> = sigs_json
        .iter()
        .map(|s| s.parse())
        .collect::<Result<Vec<_>>>()?;

    println!("Loaded {} signature(s)", sigs.len());
    let (outer, mut raw, _noun) = detect_outer(&draft_bytes)?;
    // apply signatures to inputs
    let mut new_inputs: ZMap<NName, Input> = ZMap::new();

    for (name, mut input) in raw.inputs.p.tap() {
        let mut sig_map: ZMap<SchnorrPubkey, SchnorrSignature> = input
            .spend
            .signature
            .as_ref()
            .map(|s| s.map.clone())
            .unwrap_or_else(ZMap::new);

        let this_name = nname_b58(&name);

        for sig_data in &sigs {
            if sig_data.input_name == this_name {
                println!("Applying signature to input {}", this_name);

                let pk = SchnorrPubkey {
                    x: F6LT {
                        values: sig_data.pubkey_x,
                    },
                    y: F6LT {
                        values: sig_data.pubkey_y,
                    },
                    inf: false,
                };

                let schnorr_sig = SchnorrSignature {
                    chal: Chal {
                        values: t8_from_device(sig_data.chal),
                    },
                    sig: Sig {
                        values: t8_from_device(sig_data.sig),
                    },
                };

                sig_map.put(pk, schnorr_sig);
            }
        }

        if sig_map.wyt() > 0 {
            input.spend.signature = Some(Signature { map: sig_map });
        }

        new_inputs.put(name, input);
    }

    raw.inputs = Inputs { p: new_inputs };

    let out_bytes = match outer {
        Outer::RawTx => {
            let mut out_slab: NounSlab = NounSlab::new();
            let n = raw.to_noun(&mut out_slab);
            out_slab.copy_into(n);
            out_slab.jam()
        }
        Outer::TxTransact { tail_jam } => {
            let mut out_slab: NounSlab = NounSlab::new();
            let raw_n = raw.to_noun(&mut out_slab);
            let tail_n: Noun = out_slab
                .cue_into(Bytes::from(tail_jam))
                .context("Failed to cue tail")?;
            let cell_n = T(&mut out_slab, &[raw_n, tail_n]);
            out_slab.copy_into(cell_n);
            out_slab.jam()
        }
        Outer::WalletTx(mut wallet_tx) => {
            wallet_tx.p = raw.inputs;
            let mut out_slab: NounSlab = NounSlab::new();
            let n = wallet_tx.to_noun(&mut out_slab);
            out_slab.copy_into(n);
            out_slab.jam()
        }
    };

    let out_path = default_out_path_for(draft_path, out_opt);
    fs::write(&out_path, &out_bytes).context("Failed to write output file")?;

    println!("Signed transaction written to {}", out_path.display());

    Ok(())
}

pub fn run(
    port: &str,
    baud: u32,
    draft_path: &str,
    out_opt: Option<&str>,
    signatures: Option<&str>,
) -> Result<()> {
    if let Some(sig_path) = signatures {
        run_apply_signatures(draft_path, out_opt, sig_path)
    } else {
        run_device(port, baud, draft_path, out_opt)
    }
}

#[allow(dead_code)]
fn zset_contains<T>(zs: &ZSet<T>, x: &T) -> bool
where
    T: noun_serde::NounEncode + Clone + core::fmt::Debug + PartialEq + Ord,
{
    zs.iter().any(|t| t == x)
}
