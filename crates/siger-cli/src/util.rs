extern crate alloc;

use crate::commands::plan::InputSigningPlan;
use crate::keys::LockKey;
use crate::serial::RW;
use alloc::fmt::Debug;
use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use cobs;
use hex;
use nockapp::noun::slab::NounSlab;
use nockapp::AtomExt;
use nockvm::mem::NockStack;
use nockvm::noun::T;
use nockvm::noun::{Atom, IndirectAtom, Noun};
use nockvm::serialization::cue;
use noun_serde::{NounDecode, NounEncode};
use postcard::{from_bytes_cobs, to_allocvec};
use siger_core::{Frame, Msg, Request, Response, PROTO_V1};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use tx_types::collections::{ZMap, ZSet};
use tx_types::transaction_types::{
    Coins, Hash, NName, PageNumber, Spend, SpendBody, TimelockRange, Transaction, T8,
    SchnorrPubkey, SchnorrSignature, Chal, Sig, Signature, F6LT,
};
use tx_types::transaction_types_v0::*;
use tx_types::RawTransaction;

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

fn is_printable_ascii(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|&b| (b == 0x09) || (b == 0x0A) || (b == 0x0D) || (0x20..=0x7E).contains(&b))
}

/// Extract transaction ID from various noun formats
pub fn transaction_name_from_noun(noun: &Noun) -> Result<String> {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    // Try RawTransaction first (works for both V0 and V1) - wrap in catch_unwind
    if let Ok(Ok(raw)) = catch_unwind(AssertUnwindSafe(|| RawTransaction::from_noun(noun))) {
        let id = match &raw {
            RawTransaction::V0(v0) => &v0.id,
            RawTransaction::V1(v1) => &v1.id,
        };
        return Ok(id.to_b58());
    }

    if let Ok(cell) = noun.as_cell() {
        let head = cell.head();

        if let Ok(Ok(raw_head)) = catch_unwind(AssertUnwindSafe(|| RawTransaction::from_noun(&head))) {
            let id = match &raw_head {
                RawTransaction::V0(v0) => &v0.id,
                RawTransaction::V1(v1) => &v1.id,
            };
            return Ok(id.to_b58());
        }

        // Try wallet Transaction (V0 format) - may panic on invalid data
        if let Ok(Ok(tx)) = catch_unwind(AssertUnwindSafe(|| Transaction::from_noun(&head))) {
            return Ok(tx.name);
        }

        if let Ok(atom) = head.as_atom() {
            if let Ok(bytes) = atom.to_bytes_until_nul() {
                if !bytes.is_empty() && is_printable_ascii(&bytes) {
                    return Ok(String::from_utf8_lossy(&bytes).to_string());
                }
            }
        }
    }

    // Try wallet Transaction at top level (may panic on invalid data)
    if let Ok(Ok(tx)) = catch_unwind(AssertUnwindSafe(|| Transaction::from_noun(noun))) {
        return Ok(tx.name);
    }

    Err(anyhow!("unable to extract transaction identifier"))
}

pub fn transaction_name_from_bytes(bytes: &[u8]) -> Result<String> {
    let mut slab: NounSlab = NounSlab::new();
    let noun = slab
        .cue_into(Bytes::from(bytes.to_vec()))
        .map_err(|e| anyhow!("cue failed: {e:?}"))?;
    transaction_name_from_noun(&noun)
}

/// Compute signature hash for a V0 transaction input
pub fn sig_hash_for_input_v0(raw: &RawTransactionV0, name: &NName) -> Hash {
    // clone + strip sigs from the target spend
    let mut spend = raw.inputs.p.get(name).expect("input missing").spend.clone();
    spend.signature = None;

    // Use spend.sig_hash() to match reference implementation
    // This computes: hash([seeds.to_sig_hashable(), fee])
    spend.sig_hash()
}

pub fn print_raw_details(raw: &RawTransaction) {
    match raw {
        RawTransaction::V0(v0) => print_raw_details_v0(v0),
        RawTransaction::V1(v1) => {
            println!("raw-tx (V1):");
            println!("  version      = {}", v1.version);
            println!("  id           = {}", v1.id.to_b58());
            println!("  spends       = {}", v1.spends.map.wyt());
            for (idx, (name, spend)) in v1.spends.map.tap().into_iter().enumerate() {
                println!("  - spend[{}]:", idx);
                if name.p.len() >= 2 {
                    println!(
                        "      name        = [{:?} {:?}]",
                        name.p[0].to_b58(),
                        name.p[1].to_b58()
                    );
                }
                // Get fee from the SpendBody
                let fee = match &spend.body {
                    SpendBody::V0(v0) => v0.fee.value,
                    SpendBody::V0ToV1(v0tov1) => v0tov1.fee.value,
                    SpendBody::V1(v1) => v1.fee.value,
                };
                println!("      fee         = {}", fee);
            }
        }
    }
}

fn print_raw_details_v0(raw: &RawTransactionV0) {
    let id_str = fmt_u64x5(&raw.id.values);
    let inputs_count = raw.inputs.p.wyt();
    let tl_min = raw.timelock_range.min.as_ref().map(|p| p.value);
    let tl_max = raw.timelock_range.max.as_ref().map(|p| p.value);
    let fee = raw.total_fees.value;

    println!("raw-tx (V0):");
    println!("  id           = {}", raw.id.to_b58());
    println!("  inputs       = {}", inputs_count);
    println!("  timelock     = [{:?}, {:?}]", tl_min, tl_max);
    println!("  total_fees   = {}", fee);

    for (idx, (name, input)) in raw.inputs.p.tap().into_iter().enumerate() {
        println!("  - input[{}]:", idx);
        let name = &input.note.name;
        if name.p.len() >= 2 {
            println!(
                "      name        = [{:?} {:?}]",
                name.p[0].to_b58(),
                name.p[1].to_b58()
            );
        } else {
            println!("      name        = <unexpected arity {}>", name.p.len());
        }
        println!("      origin_page = {}", input.note.meta.origin_page.value);
        println!("      assets      = {}", input.note.assets.value);

        // Source
        let src_hash = fmt_u64x5(&input.note.source.p.values);
        println!(
            "      source      = {{ hash={}, coinbase={} }}",
            src_hash, input.note.source.is_coinbase
        );

        // Timelock (range this note can spend, absolute)
        let (i_min, i_max) = input.calculate_timelock_range();
        println!("      timelock    = [{:?}, {:?}]", i_min, i_max);

        // Show up to 8 pubkeys (x only) for brevity
        let (m, pks_b58) = input.note.lock.to_b58();
        println!("      lock        = {}-of-{} signers", m, pks_b58.len());
        for (i, pk) in pks_b58.iter().enumerate() {
            println!("        pk[{}] = {}", i, pk);
        }

        // Spend / fee / sigs
        println!("      fee         = {}", input.spend.fee.value);
        let sig_count = input
            .spend
            .signature
            .as_ref()
            .map(|m| m.map.wyt())
            .unwrap_or(0);
        println!("      signatures  = {}", sig_count);

        if let Some(sigmap) = &input.spend.signature {
            for (sidx, (pk, sig)) in sigmap.map.tap().into_iter().take(8).enumerate() {
                let x = fmt_u64x6(&pk.x.values);
                let chal = fmt_u64x8(&sig.chal.values.values);
                let s = fmt_u64x8(&sig.sig.values.values);
                println!(
                    "        sig[{sidx}] pk.X={x} chal={chal} s={s}{}",
                    if sidx == 7 && sig_count > 8 {
                        " …"
                    } else {
                        ""
                    }
                );
            }
        }
        print_input_seeds_v0(&input);
    }
    summarize_outputs_v0(raw);
}

pub fn summarize_outputs(tx: &RawTransaction) -> BTreeMap<LockKey, u128> {
    match tx {
        RawTransaction::V0(v0) => summarize_outputs_v0(v0),
        RawTransaction::V1(_v1) => BTreeMap::new(), // V1 doesn't have this structure
    }
}

fn summarize_outputs_v0(tx: &RawTransactionV0) -> BTreeMap<LockKey, u128> {
    let mut by_lock: BTreeMap<LockKey, u128> = BTreeMap::new();

    for (_name, input) in tx.inputs.p.tap().into_iter() {
        for seed in input.spend.seeds.set.iter() {
            *by_lock.entry(LockKey(seed.recipient.clone())).or_insert(0) += seed.gift.value as u128;
        }
    }
    by_lock
}

fn print_input_seeds_v0(input: &InputV0) {
    for (k, seed) in input.spend.seeds.set.iter().enumerate() {
        let (m, pks_b58) = seed.recipient.to_b58();
        println!(
            "      seed[{k}]: gift = {}, to {m}-of-{}",
            seed.gift.value,
            pks_b58.len()
        );
        for (j, pk) in pks_b58.iter().enumerate() {
            println!("        pk[{j}] = {pk}");
        }
        println!("        parent = {}", seed.parent_hash.to_b58());
    }
}

pub fn print_outputs(tx: &RawTransaction) {
    let outs = summarize_outputs(tx);
    println!("outputs (derived from seeds): {}", outs.len());
    for (i, (LockKey(lock), amt)) in outs.iter().enumerate() {
        let (m, pks_b58) = lock.to_b58();
        println!("  out[{i}]: gift = {amt}, to {m}-of-{}", pks_b58.len());
        for (j, pk) in pks_b58.iter().enumerate() {
            println!("    pk[{j}] = {pk}");
        }
    }
}

#[inline]
pub fn t8_from_device(words: [u64; 8]) -> T8 {
    // The ESP firmware returns T8 values which may be in two different formats:
    // Case 1: 4x64-bit words (MSW..LSW) with upper 4 words zeroed
    //         This happens when the device sends fewer than 8 limbs
    // Case 2: 8x limbs, each containing a 32-bit value in the low bits
    //         This is the standard T8 format from be32_atom_to_t8_le

    // Case 1: device gave 4x64 (MSW..LSW) and left the top half zeroed
    if words[4..].iter().all(|&w| w == 0) {
        let mut v = [0u64; 8];
        // words[3] is least-significant 64 bits if device sent MSW..LSW
        for i in 0..4 {
            let w = words[i];
            v[i * 2 + 0] = (w & 0xffff_ffff) as u64; // low 32 bits
            v[i * 2 + 1] = (w >> 32) as u64; // high 32 bits
        }
        T8 { values: v }
    } else {
        // Case 2: device already gave 8 limbs; ensure high halves are zero
        let mut v = [0u64; 8];
        for i in 0..8 {
            v[i] = words[i] & 0xffff_ffff;
        }
        T8 { values: v }
    }
}

pub fn pretty_noun(n: &Noun, max_depth: usize, max_items: usize) -> String {
    fn fmt_atom(atom: &Atom) -> String {
        // cord if it has a terminating NUL and is printable
        if let Ok(bytes) = atom.to_bytes_until_nul() {
            let b: Vec<u8> = bytes.to_vec();
            if is_printable_ascii(&b) {
                return format!("atom(cord:\"{}\")", String::from_utf8_lossy(&b));
            }
        }
        // otherwise: show small atoms as hex, big ones summarized
        let nbits = nockvm::serialization::met0_usize(atom.clone());
        let nbytes = (nbits + 7) / 8;
        if nbytes <= 64 {
            let mut v = vec![0u8; nbytes];
            let _ = atom.as_bitslice();
            format!("atom({} bytes, 0x{})", nbytes, hex::encode(v))
        } else {
            format!("atom({} bytes)", nbytes)
        }
    }

    fn try_collect_list(mut n: Noun, max_items: usize) -> Option<(Vec<Noun>, bool)> {
        let mut out = Vec::new();
        for _ in 0..max_items {
            if let Ok(cell) = n.as_cell() {
                out.push(cell.head());
                n = cell.tail();
                if let Ok(a) = n.as_atom() {
                    if a.as_u64() == Ok(0) {
                        return Some((out, false));
                    }
                }
            } else {
                return None;
            }
        }
        Some((out, true)) // truncated
    }

    fn go(n: Noun, depth: usize, max_depth: usize, max_items: usize, indent: usize) -> String {
        if depth >= max_depth {
            return "...".into();
        }
        if let Ok(a) = n.as_atom() {
            return fmt_atom(&a);
        }
        if let Ok(c) = n.as_cell() {
            // try render as list if shape matches
            if let Some((els, truncated)) = try_collect_list(n, max_items) {
                let mut s = String::new();
                s.push_str("[\n");
                for (i, el) in els.into_iter().enumerate() {
                    s.push_str(&" ".repeat(indent + 2));
                    s.push_str(&go(el, depth + 1, max_depth, max_items, indent + 2));
                    if i + 1 < max_items {
                        s.push('\n');
                    }
                }
                if truncated {
                    s.push_str(&" ".repeat(indent + 2));
                    s.push_str("…\n");
                }
                s.push_str(&" ".repeat(indent));
                s.push(']');
                return s;
            }
            // generic cell (head .. tail)
            let mut s = String::new();
            s.push_str("[\n");
            s.push_str(&" ".repeat(indent + 2));
            s.push_str(&go(c.head(), depth + 1, max_depth, max_items, indent + 2));
            s.push_str(",\n");
            s.push_str(&" ".repeat(indent + 2));
            s.push_str(&go(c.tail(), depth + 1, max_depth, max_items, indent + 2));
            s.push_str("\n");
            s.push_str(&" ".repeat(indent));
            s.push(']');
            return s;
        }
        "<?>".into()
    }

    go(n.clone(), 0, max_depth, max_items, 0)
}

pub fn load_draft_as_raw(path: &Path) -> anyhow::Result<RawTransaction> {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let data = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;

    // Keep allocator alive during decode
    let mut slab: NounSlab = NounSlab::new();
    let noun = slab
        .cue_into(Bytes::from(data))
        .map_err(|e| anyhow!("cue failed: {e:?}"))?;

    // raw-tx (either V0 or V1) - wrap in catch_unwind
    if let Ok(Ok(raw)) = catch_unwind(AssertUnwindSafe(|| RawTransaction::from_noun(&noun))) {
        return Ok(raw);
    }

    // tx:transact — head is raw-tx
    if let Ok(cell) = noun.as_cell() {
        if let Ok(Ok(raw)) = catch_unwind(AssertUnwindSafe(|| RawTransaction::from_noun(&cell.head()))) {
            return Ok(raw);
        }
    }

    // wallet transaction:wt (`p` (inputs) and `name`) - V0 only
    if let Ok(Ok(tx_wallet)) = catch_unwind(AssertUnwindSafe(|| Transaction::from_noun(&noun))) {
        return Ok(RawTransaction::V0(transaction_to_raw(&tx_wallet)));
    }

    // 4) Naked pair: [name inputs] - V0 only
    if let Ok(cell) = noun.as_cell() {
        if let Ok(Ok(inputs)) = catch_unwind(AssertUnwindSafe(|| InputsV0::from_noun(&cell.tail()))) {
            let raw = raw_from_inputs_v0(inputs);
            return Ok(RawTransaction::V0(raw));
        }
    }

    Err(anyhow!(
        "decode failed: not RawTransaction / tx:transact / transaction:wt / [name inputs]"
    ))
}

pub fn raw_from_inputs_v0(inputs: InputsV0) -> RawTransactionV0 {
    use tx_types::hashing::tx_id::compute_tx_id;
    use tx_types::transaction_types::Inputs;

    let total_fees = sum_inputs_fees_v0(&inputs);
    let tl = TimelockRange {
        min: None,
        max: None,
    };

    // Compute tx_id using the public function
    let inputs_enum = Inputs::V0(inputs.clone());
    let tx_id = compute_tx_id(&inputs_enum, &tl, Coins { value: total_fees });

    RawTransactionV0 {
        id: tx_id,
        inputs,
        timelock_range: tl,
        total_fees: Coins { value: total_fees },
    }
}

pub fn decode_cord_like(n: Noun) -> Option<String> {
    // Try cord (bytes up to NUL) → UTF-8 string
    n.as_atom()
        .ok()
        .and_then(|a| a.to_bytes_until_nul().ok())
        .map(|b| String::from_utf8_lossy(&b).to_string())
}

/// Convert wallet Transaction (V0) to RawTransactionV0
pub fn transaction_to_raw(tx: &Transaction) -> RawTransactionV0 {
    use tx_types::hashing::tx_id::compute_tx_id;
    use tx_types::transaction_types::Inputs;

    // Transaction.p is Inputs (enum), extract V0 version
    let inputs = match &tx.p {
        Inputs::V0(v0) => v0.clone(),
        Inputs::V1(_) => panic!("transaction_to_raw only works with V0 transactions"),
    };
    let total_fees = sum_inputs_fees_v0(&inputs);
    let tl = union_inputs_timelock_range_v0(&inputs);

    // Compute tx_id using the public function
    let inputs_enum = Inputs::V0(inputs.clone());
    let tx_id = compute_tx_id(&inputs_enum, &tl, Coins { value: total_fees });

    RawTransactionV0 {
        id: tx_id,
        inputs,
        timelock_range: tl,
        total_fees: Coins { value: total_fees },
    }
}

fn sum_inputs_fees_v0(inputs: &InputsV0) -> u64 {
    inputs
        .p
        .tap()
        .into_iter()
        .fold(0u64, |acc, (_n, i)| acc.saturating_add(i.spend.fee.value))
}

fn union_inputs_timelock_range_v0(inputs: &InputsV0) -> TimelockRange {
    let mut min_page: Option<u64> = None;
    let mut max_page: Option<u64> = None;
    for (_name, input) in inputs.p.tap().into_iter() {
        let (i_min, i_max) = input.calculate_timelock_range();
        if let Some(v) = i_min {
            min_page = Some(min_page.map_or(v, |m| m.min(v)));
        }
        if let Some(v) = i_max {
            max_page = Some(max_page.map_or(v, |m| m.max(v)));
        }
    }
    TimelockRange {
        min: min_page.map(|v| PageNumber { value: v }),
        max: max_page.map(|v| PageNumber { value: v }),
    }
}

pub fn enumerate_signing_plans(inputs: &InputsV0) -> alloc::vec::Vec<InputSigningPlan> {
    let pairs: alloc::vec::Vec<(NName, InputV0)> = inputs.p.tap();
    pairs
        .into_iter()
        .map(|(name, input)| {
            let m = input.note.lock.m as usize;
            let mut keys: alloc::vec::Vec<SchnorrPubkey> = input.note.lock.pubkeys.tap();
            keys.sort(); // needs Ord on SchnorrPubkey

            let mut combos = alloc::vec::Vec::new();
            if m > 0 && m <= keys.len() {
                let mut cur = alloc::vec::Vec::with_capacity(m);
                fn choose(
                    out: &mut alloc::vec::Vec<alloc::vec::Vec<SchnorrPubkey>>,
                    keys: &[SchnorrPubkey],
                    m: usize,
                    start: usize,
                    cur: &mut alloc::vec::Vec<SchnorrPubkey>,
                ) {
                    if cur.len() == m {
                        out.push(cur.clone());
                        return;
                    }
                    for i in start..keys.len() {
                        cur.push(keys[i].clone());
                        choose(out, keys, m, i + 1, cur);
                        cur.pop();
                    }
                }
                choose(&mut combos, &keys, m, 0, &mut cur);
            }

            InputSigningPlan {
                name,
                m: input.note.lock.m,
                combos,
            }
        })
        .collect()
}

/// Sign a draft transaction with the specified paths (V0 only for now)
pub fn sign_draft_with_paths(
    sp: &mut dyn RW,
    draft_path: &str,
    signer_paths: Vec<Vec<u32>>,
) -> Result<RawTransaction> {
    let raw = load_draft_as_raw(Path::new(draft_path))?;

    // Only V0 signing supported in this function for now
    let mut v0 = match raw {
        RawTransaction::V0(v) => v,
        RawTransaction::V1(_) => return Err(anyhow!("sign_draft_with_paths only supports V0 transactions")),
    };

    let slot = signer_slot();
    let select_msg = Msg {
        v: PROTO_V1,
        id: 0x4001,
        msg: Frame::One(Request::SelectSeed { slot }),
    };
    match round_trip_frame(sp, &select_msg)?.msg {
        Response::Ok => {}
        Response::Err { code } => return Err(anyhow!("SelectSeed failed: code {}", code)),
        other => return Err(anyhow!("unexpected response to SelectSeed: {other:?}")),
    }

    // cache pk per path
    let mut path_pks: Vec<(Vec<u32>, SchnorrPubkey)> = Vec::new();
    for path in signer_paths.iter() {
        let resp: Msg<Response> = {
            let m = Msg {
                v: PROTO_V1,
                id: 0x4100,
                msg: Frame::One(Request::GetCheetahPub {
                    slot,
                    path: path.clone(),
                }),
            };
            round_trip_frame(sp, &m)?
        };
        let pk = match resp.msg {
            Response::OkCheetahPub { x, y } => SchnorrPubkey {
                x: F6LT { values: x },
                y: F6LT { values: y },
                inf: false,
            },
            Response::Err { code } => return Err(anyhow!("GetCheetahPub failed: code {}", code)),
            _ => return Err(anyhow!("unexpected response to GetCheetahPub")),
        };
        path_pks.push((path.clone(), pk));
    }

    // sign inputs
    let mut new_inputs = ZMap::new();
    for (name, mut input) in v0.inputs.p.tap() {
        let lock_pks = &input.note.lock.pubkeys; // ZSet<SchnorrPubkey>

        // reuse or create signature map
        let mut sig_map: ZMap<SchnorrPubkey, SchnorrSignature> = input
            .spend
            .signature
            .as_ref()
            .map(|s| s.map.clone())
            .unwrap_or_else(ZMap::new);

        for (path, pk_dev) in path_pks.iter() {
            if !zset_contains(lock_pks, pk_dev) {
                continue;
            }

            let msg5: [u64; 5] = sig_hash_for_input_v0(&v0, &name).values;
            let req = Msg {
                v: PROTO_V1,
                id: 0x4200,
                msg: Frame::One(Request::SignSpendHash {
                    slot,
                    path: path.clone(),
                    msg5,
                    meta: None,
                }),
            };
            let resp: Msg<Response> = round_trip_frame(sp, &req)?;
            let (chal, sig) = match resp.msg {
                Response::OkCheetahSig { chal, sig } => (chal, sig),
                Response::Err { code } => {
                    return Err(anyhow!("SignSpendHash failed: code {}", code))
                }
                _ => return Err(anyhow!("unexpected response to SignSpendHash")),
            };

            let schnorr_sig = SchnorrSignature {
                chal: Chal {
                    values: T8 { values: chal },
                },
                sig: Sig {
                    values: T8 { values: sig },
                },
            };
            sig_map.put(pk_dev.clone(), schnorr_sig);
        }

        if sig_map.wyt() > 0 {
            input.spend.signature = Some(Signature { map: sig_map });
        }
        new_inputs.put(name, input);
    }

    v0.inputs = InputsV0 { p: new_inputs };
    Ok(RawTransaction::V0(v0))
}

// minimal "round_trip" for Msg<Frame> (used above)
pub fn round_trip_frame(sp: &mut dyn RW, req: &Msg<Frame>) -> Result<Msg<Response>> {
    let buf = to_allocvec(req)?;
    write_cobs_frame(sp, &buf)?;
    let mut frame = read_cobs_frame(sp, 4 * 1024)?;
    let resp: Msg<Response> = from_bytes_cobs(&mut frame)?;
    if resp.v != PROTO_V1 {
        return Err(anyhow!("unsupported proto version {}", resp.v));
    }
    if resp.id != req.id {
        return Err(anyhow!(
            "mismatched id: got {}, expected {}",
            resp.id,
            req.id
        ));
    }
    Ok(resp)
}

// generic COBS helpers for RW (used by round_trip_frame)
pub fn write_cobs_frame(sp: &mut dyn RW, payload: &[u8]) -> Result<()> {
    let mut enc = vec![0u8; payload.len() + payload.len() / 254 + 2];
    let n = cobs::encode(payload, &mut enc);
    sp.write_all(&enc[..n])?;
    sp.write_all(&[0])?;
    Ok(())
}

pub fn read_cobs_frame(sp: &mut dyn RW, max_len: usize) -> Result<Vec<u8>> {
    let mut rx = Vec::with_capacity(256);
    let mut b = [0u8; 1];
    loop {
        match sp.read(&mut b) {
            Ok(1) => {
                rx.push(b[0]);
                if rx.len() > max_len {
                    return Err(anyhow!("frame too large (> {} bytes)", max_len));
                }
                if b[0] == 0 {
                    return Ok(rx);
                }
            }
            Ok(_) => continue,
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => return Err(e.into()),
        }
    }
}

// safer ZSet contains using tap()
pub fn zset_contains<T>(zs: &ZSet<T>, x: &T) -> bool
where
    T: NounEncode + Clone + Debug + PartialEq + Ord,
{
    zs.iter().any(|t| t == x)
}

// ---------- tiny utils -------------------------------------------------------

pub fn parse_64(s: &str) -> anyhow::Result<[u8; 64]> {
    let mut h = s.trim();
    if let Some(stripped) = h.strip_prefix("0x") {
        h = stripped;
    }
    let cleaned: String = h
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .collect();

    let bytes =
        hex::decode(&cleaned).map_err(|e| anyhow::anyhow!("invalid hex for 64-byte seed: {e}"))?;
    if bytes.len() != 64 {
        return Err(anyhow::anyhow!(
            "seed must be exactly 64 bytes (got {} bytes)",
            bytes.len()
        ));
    }

    let mut out = [0u8; 64];
    out.copy_from_slice(&bytes);
    Ok(out)
}

pub fn fmt_u64x5(v: &[u64; 5]) -> String {
    v.iter()
        .map(|w| format!("{w:016x}"))
        .collect::<Vec<_>>()
        .join("_")
}
pub fn fmt_u64x6(v: &[u64; 6]) -> String {
    v.iter()
        .map(|w| format!("{w:016x}"))
        .collect::<Vec<_>>()
        .join("_")
}
pub fn fmt_u64x8(v: &[u64; 8]) -> String {
    v.iter()
        .map(|w| format!("{w:016x}"))
        .collect::<Vec<_>>()
        .join("_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tx_types::collections::zset::ZSet;
    use tx_types::transaction_types::*;
    use tx_types::transaction_types_v0::*;

    #[test]
    fn test_sig_hash_for_input_matches_spend_sig_hash() {
        // Create a test spend with some seeds
        let seed = SeedV0 {
            output_source: Some(Source {
                p: Hash {
                    values: [1, 2, 3, 4, 5],
                },
                is_coinbase: false,
            }),
            recipient: Lock {
                m: 1,
                pubkeys: ZSet::new(),
            },
            timelock_intent: None,
            gift: Coins { value: 100 },
            parent_hash: Hash {
                values: [10, 11, 12, 13, 14],
            },
        };

        let mut seeds_set = ZSet::new();
        seeds_set.put(seed);

        let spend = SpendV0 {
            signature: None,
            seeds: SeedsV0 { set: seeds_set },
            fee: Coins { value: 10 },
        };

        // Create a minimal RawTransactionV0 with this spend
        let input = InputV0 {
            note: NNoteV0 {
                meta: NNoteHead {
                    version: 1,
                    origin_page: PageNumber { value: 1 },
                    timelock: Timelock { intent: None },
                },
                name: NName {
                    p: vec![Hash {
                        values: [1, 0, 0, 0, 0],
                    }],
                },
                lock: Lock {
                    m: 1,
                    pubkeys: ZSet::new(),
                },
                source: Source {
                    p: Hash { values: [0; 5] },
                    is_coinbase: false,
                },
                assets: Coins { value: 1000 },
            },
            spend: spend.clone(),
        };

        let name = NName {
            p: vec![Hash {
                values: [1, 0, 0, 0, 0],
            }],
        };

        let mut inputs_map = ZMap::new();
        inputs_map.put(name.clone(), input);

        let raw = RawTransactionV0 {
            id: Hash { values: [0; 5] },
            inputs: InputsV0 { p: inputs_map },
            timelock_range: TimelockRange {
                min: None,
                max: None,
            },
            total_fees: Coins { value: 10 },
        };

        // Test that sig_hash_for_input_v0 returns the same as spend.sig_hash()
        let hash_from_util = sig_hash_for_input_v0(&raw, &name);
        let hash_from_spend = spend.sig_hash();

        assert_eq!(
            hash_from_util.values, hash_from_spend.values,
            "sig_hash_for_input_v0 should return the same hash as spend.sig_hash()"
        );

        println!("sig_hash_for_input_v0 correctly matches spend.sig_hash()");
        println!("  Hash: {:016x?}", hash_from_util.values);
    }
}
