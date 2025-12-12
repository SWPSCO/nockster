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

use siger_core::{Frame, Msg, Request, Response, MAX_INFO_CHEETAH_PUBS, PROTO_V1};
use tx_types::collections::{ZMap, ZSet};
use tx_types::transaction_types::{
    Hash, NName, Spend, SpendBody, Chal, Sig, SchnorrPubkey, SchnorrSignature,
    Signature, F6LT, Transaction,
};
use tx_types::transaction_types_v0::*;
use tx_types::transaction_types_v1::{
    compute_tx_id_v1, PkhSignature, PkhSignatureValue, RawTransactionV1, SpendsV1
};
use tx_types::RawTransaction;

use crate::keys;
use crate::serial::{open, round_trip_frame, send_call};
use crate::util::{fmt_u64x5, t8_from_device, transaction_name_from_bytes};

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

fn signer_slot() -> u8 {
    std::env::var("SIGER_SLOT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .and_then(|v| {
            if v <= u8::MAX as u16 {
                Some(v as u8)
            } else {
                None
            }
        })
        .unwrap_or(0)
}

fn fetch_device_pks(
    sp: &mut dyn SerialPort,
    slot: u8,
    paths: &[Vec<u32>],
) -> Result<Vec<(Vec<u32>, SchnorrPubkey)>> {
    let mut out = Vec::new();
    for path in paths {
        let req = Msg {
            v: PROTO_V1,
            id: 0x4100,
            msg: Frame::One(Request::GetCheetahPub {
                slot,
                path: path.clone(),
            }),
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
    WalletTxV0(Transaction),
}

fn detect_outer(bytes: &[u8]) -> Result<(Outer, RawTransaction, Noun)> {
    let mut slab: NounSlab = NounSlab::new();
    let noun: Noun = slab
        .cue_into(Bytes::from(bytes.to_vec()))
        .map_err(|e| anyhow!("cue failed: {e:?}"))?;

    // try wallet transaction (v0 only for now)
    if let Ok(tx) = Transaction::from_noun(&noun) {
        let raw = transaction_to_raw_v0(&tx);
        return Ok((Outer::WalletTxV0(tx), RawTransaction::V0(raw), noun));
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

/// Convert wallet Transaction to RawTransactionV0
fn transaction_to_raw_v0(tx: &Transaction) -> RawTransactionV0 {
    // Re-use existing logic from util module or inline
    crate::util::transaction_to_raw(tx)
}

/// Get transaction ID regardless of version
fn get_tx_id(raw: &RawTransaction) -> Hash {
    match raw {
        RawTransaction::V0(v0) => v0.id.clone(),
        RawTransaction::V1(v1) => v1.id.clone(),
    }
}

/// Compute sig hash for a given input/spend
fn compute_sig_hash(raw: &RawTransaction, name: &NName) -> Hash {
    match raw {
        RawTransaction::V0(v0) => crate::util::sig_hash_for_input_v0(v0, name),
        RawTransaction::V1(v1) => {
            // For V1, compute sig hash from the spend's seeds and fee
            if let Some(spend) = v1.spends.map.get(name) {
                // Extract SpendV1 from the Spend wrapper
                match &spend.body {
                    SpendBody::V1(v1) => v1.compute_sig_hash(),
                    _ => Hash { values: [0; 5] },
                }
            } else {
                // Return zero hash if spend not found
                Hash { values: [0; 5] }
            }
        }
    }
}

fn run_device(port: &str, baud: u32, draft_path: &str, out_opt: Option<&str>) -> Result<()> {
    let in_bytes = fs::read(draft_path).with_context(|| format!("read {draft_path}"))?;
    let (outer, raw, _noun_before) = detect_outer(&in_bytes)?;

    let tx_id = get_tx_id(&raw);
    let tx_name_before = tx_id.to_b58();

    println!("file:  {draft_path}");
    println!("txid:  {tx_name_before}");

    // Check transaction version
    let is_v1 = matches!(raw, RawTransaction::V1(_));
    println!("version: {}", if is_v1 { "V1" } else { "V0" });

    // collect desired signer derivation paths (optional override via env)
    let env_paths: Option<Vec<String>> = std::env::var("SIGER_PATHS")
        .or_else(|_| std::env::var("SIGER_PATH"))
        .ok()
        .map(|s| {
            s.split(|c| c == ';' || c == ',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
        });

    let mut sp = open(port, baud)?;

    let slot = signer_slot();
    match send_call(&mut *sp, 0x4001, Request::SelectSeed { slot })? {
        Response::Ok => {}
        Response::Err { code } => anyhow::bail!("SelectSeed failed with code {code}"),
        other => anyhow::bail!("unexpected SelectSeed response: {other:?}"),
    }

    let info_resp: Response = send_call(&mut *sp, 0x4000, Request::GetInfo)?;
    let available_keys = match info_resp {
        Response::Info { cheetah_pubs, .. } => cheetah_pubs,
        other => anyhow::bail!("unexpected GetInfo response: {other:?}"),
    };

    let mut preloaded_keys: Vec<(u8, Vec<u32>, SchnorrPubkey)> = available_keys
        .into_iter()
        .map(|pubinfo| {
            let pk = SchnorrPubkey {
                x: F6LT { values: pubinfo.x },
                y: F6LT { values: pubinfo.y },
                inf: false,
            };
            (pubinfo.slot, pubinfo.path, pk)
        })
        .collect();

    if preloaded_keys.len() > MAX_INFO_CHEETAH_PUBS {
        preloaded_keys.truncate(MAX_INFO_CHEETAH_PUBS);
    }

    let signer_paths = if let Some(path_strings) = env_paths {
        parse_signer_paths(&path_strings)?
    } else if !preloaded_keys.is_empty() {
        preloaded_keys
            .iter()
            .filter(|(s, _, _)| *s == slot)
            .map(|(_, path, _)| path.clone())
            .collect()
    } else {
        vec![Vec::<u32>::new()]
    };

    // Ensure we have device pubkeys for each requested path
    let mut dev_keys: Vec<(Vec<u32>, SchnorrPubkey)> = Vec::new();
    for path in signer_paths {
        if let Some((_, pref_path, pk)) = preloaded_keys
            .iter()
            .find(|(s, p, _)| *s == slot && p.as_slice() == path.as_slice())
        {
            dev_keys.push((pref_path.clone(), pk.clone()));
        } else {
            let fetched = fetch_device_pks(&mut *sp, slot, &[path.clone()])?;
            dev_keys.extend(fetched);
        }
    }

    println!("device keys: {}", dev_keys.len());

    // Sign based on version
    let (out_bytes, signed_count) = match raw {
        RawTransaction::V0(v0) => sign_v0_transaction(
            &mut *sp,
            slot,
            &dev_keys,
            v0,
            &outer,
        )?,
        RawTransaction::V1(v1) => sign_v1_transaction(
            &mut *sp,
            slot,
            &dev_keys,
            v1,
            &outer,
        )?,
    };

    // write output
    let out_path = default_out_path_for(draft_path, out_opt);
    fs::write(&out_path, &out_bytes).with_context(|| format!("write {}", out_path.display()))?;

    let tx_name_after = transaction_name_from_bytes(&out_bytes).unwrap_or_else(|_| tx_name_before.clone());
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

/// Sign a V0 transaction
fn sign_v0_transaction(
    sp: &mut dyn SerialPort,
    slot: u8,
    dev_keys: &[(Vec<u32>, SchnorrPubkey)],
    raw: RawTransactionV0,
    outer: &Outer,
) -> Result<(Bytes, usize)> {
    let mut new_inputs: ZMap<NName, InputV0> = ZMap::new();
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
                let msg_hash = crate::util::sig_hash_for_input_v0(&raw, &name);
                let msg5: [u64; 5] = msg_hash.values;
                println!("    signing hash for {:?}: {}", name, fmt_u64x5(&msg5));
                let req = Msg {
                    v: PROTO_V1,
                    id: 0x4200,
                    msg: Frame::One(Request::SignSpendHash {
                        slot,
                        path: path.clone(),
                        msg5,
                    }),
                };
                let resp: Msg<Response> = round_trip_frame(sp, &req)?;
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
    updated.inputs = InputsV0 { p: new_inputs };

    // recalculate the transaction ID with the signed inputs
    use tx_types::transaction_types::Inputs;
    let inputs_enum = Inputs::V0(updated.inputs.clone());
    updated.id = tx_types::hashing::tx_id::compute_tx_id(
        &inputs_enum,
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
            let mut out_slab: NounSlab = NounSlab::new();
            let head = updated.to_noun(&mut out_slab);
            let tail = out_slab
                .cue_into(Bytes::from(tail_jam.clone()))
                .expect("cue original tail");
            let cell = T(&mut out_slab, &[head, tail]);
            out_slab.copy_into(cell);
            out_slab.jam()
        }
        Outer::WalletTxV0(ref tx) => {
            use tx_types::transaction_types::Inputs;
            let mut tx_clone = tx.clone();
            tx_clone.p = Inputs::V0(updated.inputs.clone());
            let mut out_slab: NounSlab = NounSlab::new();
            let n = tx_clone.to_noun(&mut out_slab);
            out_slab.copy_into(n);
            out_slab.jam()
        }
    };

    Ok((out_bytes, signed_count))
}

/// Sign a V1 transaction
fn sign_v1_transaction(
    sp: &mut dyn SerialPort,
    slot: u8,
    dev_keys: &[(Vec<u32>, SchnorrPubkey)],
    raw: RawTransactionV1,
    outer: &Outer,
) -> Result<(Bytes, usize)> {
    let mut new_spends: ZMap<NName, Spend> = ZMap::new();
    let mut signed_count = 0usize;

    for (name, mut spend) in raw.spends.map.tap() {
        // Extract SpendV1 from the Spend wrapper
        let spend_v1 = match &mut spend.body {
            SpendBody::V1(v1) => v1,
            _ => {
                // Not a V1 spend body, keep as-is
                new_spends.put(name, spend);
                continue;
            }
        };

        // Get existing signatures or create new map
        let mut sig_map: ZMap<Hash, PkhSignatureValue> = spend_v1.witness.pkh.map.clone();

        // For each device key, check if it can sign this spend
        for (path, pk_dev) in dev_keys.iter() {
            let pk_hash = pk_dev.to_hash();

            // Check if this pubkey hash is needed for this spend
            // In V1, we check if the pubkey hash matches a leaf in the lock merkle tree
            // For now, we'll try to sign if the pubkey hash isn't already in the signature map
            if sig_map.get(&pk_hash).is_none() {
                // Compute sig hash for this spend
                let msg_hash = spend_v1.compute_sig_hash();
                let msg5: [u64; 5] = msg_hash.values;

                println!("    signing V1 spend {:?}: {}", name, fmt_u64x5(&msg5));

                let req = Msg {
                    v: PROTO_V1,
                    id: 0x4200,
                    msg: Frame::One(Request::SignSpendHash {
                        slot,
                        path: path.clone(),
                        msg5,
                    }),
                };
                let resp: Msg<Response> = round_trip_frame(sp, &req)?;
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

                // Create PkhSignatureValue with pubkey and signature
                let sig_value = PkhSignatureValue {
                    pk: pk_dev.clone(),
                    sig: schnorr_sig,
                };

                // Add to signature map keyed by pubkey hash
                sig_map.put(pk_hash, sig_value);
                signed_count += 1;
            }
        }

        // Update the witness with new signatures
        spend_v1.witness.pkh = PkhSignature { map: sig_map };
        new_spends.put(name, spend);
    }

    let mut updated = raw.clone();
    updated.spends = SpendsV1 { map: new_spends };

    // Recompute transaction ID for V1
    updated.id = compute_tx_id_v1(&updated.spends);

    let out_bytes = match outer {
        Outer::RawTx => {
            let mut out_slab: NounSlab = NounSlab::new();
            let n = updated.to_noun(&mut out_slab);
            out_slab.copy_into(n);
            out_slab.jam()
        }
        Outer::TxTransact { tail_jam } => {
            let mut out_slab: NounSlab = NounSlab::new();
            let head = updated.to_noun(&mut out_slab);
            let tail = out_slab
                .cue_into(Bytes::from(tail_jam.clone()))
                .expect("cue original tail");
            let cell = T(&mut out_slab, &[head, tail]);
            out_slab.copy_into(cell);
            out_slab.jam()
        }
        Outer::WalletTxV0(_) => {
            // V1 transactions shouldn't be in V0 wallet format
            return Err(anyhow!("V1 transaction cannot be stored in V0 wallet format"));
        }
    };

    Ok((out_bytes, signed_count))
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
    let (outer, raw, _noun) = detect_outer(&draft_bytes)?;

    // Apply signatures based on version
    let out_bytes = match raw {
        RawTransaction::V0(mut v0) => {
            let mut new_inputs: ZMap<NName, InputV0> = ZMap::new();

            for (name, mut input) in v0.inputs.p.tap() {
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

            v0.inputs = InputsV0 { p: new_inputs };

            match outer {
                Outer::RawTx => {
                    let mut out_slab: NounSlab = NounSlab::new();
                    let n = v0.to_noun(&mut out_slab);
                    out_slab.copy_into(n);
                    out_slab.jam()
                }
                Outer::TxTransact { tail_jam } => {
                    let mut out_slab: NounSlab = NounSlab::new();
                    let raw_n = v0.to_noun(&mut out_slab);
                    let tail_n: Noun = out_slab
                        .cue_into(Bytes::from(tail_jam))
                        .context("Failed to cue tail")?;
                    let cell_n = T(&mut out_slab, &[raw_n, tail_n]);
                    out_slab.copy_into(cell_n);
                    out_slab.jam()
                }
                Outer::WalletTxV0(ref wallet_tx) => {
                    use tx_types::transaction_types::Inputs;
                    let mut tx_clone = wallet_tx.clone();
                    tx_clone.p = Inputs::V0(v0.inputs);
                    let mut out_slab: NounSlab = NounSlab::new();
                    let n = tx_clone.to_noun(&mut out_slab);
                    out_slab.copy_into(n);
                    out_slab.jam()
                }
            }
        }
        RawTransaction::V1(mut v1) => {
            let mut new_spends: ZMap<NName, Spend> = ZMap::new();

            for (name, mut spend) in v1.spends.map.tap() {
                let this_name = nname_b58(&name);

                // Extract SpendV1 from the Spend wrapper
                if let SpendBody::V1(ref mut spend_v1) = spend.body {
                    let mut sig_map = spend_v1.witness.pkh.map.clone();

                    for sig_data in &sigs {
                        if sig_data.input_name == this_name {
                            println!("Applying V1 signature to spend {}", this_name);

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

                            let pk_hash = pk.to_hash();
                            let sig_value = PkhSignatureValue { pk, sig: schnorr_sig };
                            sig_map.put(pk_hash, sig_value);
                        }
                    }

                    spend_v1.witness.pkh = PkhSignature { map: sig_map };
                }
                new_spends.put(name, spend);
            }

            v1.spends = SpendsV1 { map: new_spends };

            match outer {
                Outer::RawTx => {
                    let mut out_slab: NounSlab = NounSlab::new();
                    let n = v1.to_noun(&mut out_slab);
                    out_slab.copy_into(n);
                    out_slab.jam()
                }
                Outer::TxTransact { tail_jam } => {
                    let mut out_slab: NounSlab = NounSlab::new();
                    let raw_n = v1.to_noun(&mut out_slab);
                    let tail_n: Noun = out_slab
                        .cue_into(Bytes::from(tail_jam))
                        .context("Failed to cue tail")?;
                    let cell_n = T(&mut out_slab, &[raw_n, tail_n]);
                    out_slab.copy_into(cell_n);
                    out_slab.jam()
                }
                Outer::WalletTxV0(_) => {
                    return Err(anyhow!("V1 transaction cannot be stored in V0 wallet format"));
                }
            }
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
