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
use tx_types::transaction_types::*;
use tx_types::Hashable;
use tx_types::RawTransaction;
use tx_types::Tip5Hasher;

pub fn debug_shape(n: &Noun) -> String {
    if let Ok(cell) = n.as_cell() {
        format!("[{:?} ..]", cell.head())
    } else if let Ok(atom) = n.as_atom() {
        match atom.to_bytes_until_nul() {
            Ok(b) => format!("atom(cord:{:?})", String::from_utf8_lossy(&b)),
            _ => format!("atom({} bits)", nockvm::serialization::met0_usize(atom)),
        }
    } else {
        "direct".into()
    }
}

pub fn transaction_name_from_noun(noun: &Noun) -> Result<String> {
    if let Ok(tx) = Transaction::from_noun(noun) {
        return Ok(tx.name);
    }

    if let Ok(raw) = RawTransaction::from_noun(noun) {
        return Ok(raw.id.to_b58());
    }

    if let Ok(cell) = noun.as_cell() {
        let head = cell.head();

        if let Ok(raw_head) = RawTransaction::from_noun(&head) {
            return Ok(raw_head.id.to_b58());
        }

        if let Ok(tx_head) = Transaction::from_noun(&head) {
            return Ok(tx_head.name);
        }

        if let Ok(atom) = head.as_atom() {
            if let Ok(bytes) = atom.to_bytes_until_nul() {
                if !bytes.is_empty() {
                    return Ok(String::from_utf8_lossy(&bytes).to_string());
                }
            }
        }
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

pub fn print_raw_details(raw: &RawTransaction) {
    let id_str = fmt_u64x5(&raw.id.values);
    let inputs_count = raw.inputs.p.wyt();
    let tl_min = raw.timelock_range.min.as_ref().map(|p| p.value);
    let tl_max = raw.timelock_range.max.as_ref().map(|p| p.value);
    let fee = raw.total_fees.value;

    println!("raw-tx:");
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
        print_input_seeds(&input);
    }
    summarize_outputs(raw);
}

pub fn summarize_outputs(tx: &RawTransaction) -> BTreeMap<LockKey, u128> {
    let mut by_lock: BTreeMap<LockKey, u128> = BTreeMap::new();

    for (_name, input) in tx.inputs.p.tap().into_iter() {
        for seed in input.spend.seeds.set.iter() {
            *by_lock.entry(LockKey(seed.recipient.clone())).or_insert(0) += seed.gift.value as u128;
        }
    }
    by_lock
}

pub fn print_input_seeds(input: &Input) {
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
    // Case 1: device gave 4x64 (MSW..LSW) and left the top half zeroed.
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
        // Case 2: device already gave 8 limbs; ensure high halves are zero.
        let mut v = [0u64; 8];
        for i in 0..8 {
            v[i] = words[i] & 0xffff_ffff;
        }
        T8 { values: v }
    }
}

pub fn pretty_noun(n: &Noun, max_depth: usize, max_items: usize) -> String {
    fn is_printable_ascii(bytes: &[u8]) -> bool {
        bytes
            .iter()
            .all(|&b| (b == 0x09) || (b == 0x0A) || (b == 0x0D) || (0x20..=0x7E).contains(&b))
    }

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
    let data = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;

    // Keep allocator alive during decode
    let mut slab: NounSlab = NounSlab::new();
    let noun = slab
        .cue_into(Bytes::from(data))
        .map_err(|e| anyhow!("cue failed: {e:?}"))?;

    // raw-tx
    if let Ok(raw) = RawTransaction::from_noun(&noun) {
        return Ok(raw);
    }

    // tx:transact — head is raw-tx
    if let Ok(cell) = noun.as_cell() {
        if let Ok(raw) = RawTransaction::from_noun(&cell.head()) {
            return Ok(raw);
        }
    }

    // wallet transaction:wt (`p` (inputs) and `name`)
    if let Ok(tx_wallet) = Transaction::from_noun(&noun) {
        return Ok(transaction_to_raw(&tx_wallet));
    }

    // 4) Naked pair: [name inputs]
    if let Ok(cell) = noun.as_cell() {
        if let Ok(inputs) = Inputs::from_noun(&cell.tail()) {
            let raw = raw_from_inputs(inputs);
            return Ok(raw);
        }
    }

    Err(anyhow!(
      "decode failed (shape {}): not RawTransaction / tx:transact / transaction:wt / [name inputs]",
      debug_shape(&noun)
  ))
}

pub fn raw_from_inputs(inputs: Inputs) -> RawTransaction {
    let total_fees = sum_inputs_fees(&inputs);
    let tl = TimelockRange {
        min: None,
        max: None,
    };

    let mut raw = RawTransaction {
        id: Hash { values: [0u64; 5] },
        inputs,
        timelock_range: tl,
        total_fees: Coins { value: total_fees },
    };

    let tail_hashable = Hashable::triple(
        raw.inputs.to_hashable(),
        raw.timelock_range.to_hashable(),
        Hashable::leaf_u64(raw.total_fees.value),
    );
    raw.id = tx_types::hashing::hasher::hash_hashable(&tail_hashable);
    raw
}

pub fn decode_cord_like(n: Noun) -> Option<String> {
    // Try cord (bytes up to NUL) → UTF-8 string
    n.as_atom()
        .ok()
        .and_then(|a| a.to_bytes_until_nul().ok())
        .map(|b| String::from_utf8_lossy(&b).to_string())
}

pub fn transaction_to_raw(tx: &Transaction) -> RawTransaction {
    let inputs = tx.p.clone();
    let total_fees = sum_inputs_fees(&inputs);
    let tl = union_inputs_timelock_range(&inputs);

    let mut raw = RawTransaction {
        id: Hash { values: [0u64; 5] },
        inputs,
        timelock_range: tl,
        total_fees: Coins { value: total_fees },
    };

    let tail_hashable = Hashable::triple(
        raw.inputs.to_hashable(),
        raw.timelock_range.to_hashable(),
        Hashable::leaf_u64(raw.total_fees.value),
    );
    raw.id = tx_types::hashing::hasher::hash_hashable(&tail_hashable);
    raw
}

pub fn sum_inputs_fees(inputs: &Inputs) -> u64 {
    inputs
        .p
        .tap()
        .into_iter()
        .fold(0u64, |acc, (_n, i)| acc.saturating_add(i.spend.fee.value))
}

pub fn union_inputs_timelock_range(inputs: &Inputs) -> TimelockRange {
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
pub fn enumerate_signing_plans(inputs: &Inputs) -> alloc::vec::Vec<InputSigningPlan> {
    let pairs: alloc::vec::Vec<(NName, Input)> = inputs.p.tap();
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

pub fn sign_draft_with_paths(
    sp: &mut dyn RW,
    draft_path: &str,
    signer_paths: Vec<Vec<u32>>,
) -> Result<RawTransaction> {
    let mut raw = load_draft_as_raw(Path::new(draft_path))?;
    let bytes = std::fs::read(draft_path).with_context(|| format!("read {}", draft_path))?;
    let mut stack = NockStack::new(8 << 20, 0);

    let (mut atom, mut buf) = unsafe { IndirectAtom::new_raw_mut_bytes(&mut stack, bytes.len()) };
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.as_mut_ptr(), bytes.len());
    }
    let atom = unsafe { atom.normalize_as_atom() };

    let noun: Noun = cue(&mut stack, atom).map_err(|e| anyhow::anyhow!("cue failed: {e:?}"))?;
    let mut raw: RawTransaction = RawTransaction::from_noun(&noun)
        .map_err(|e| anyhow::anyhow!("RawTransaction::from_noun failed: {e:?}"))?;

    // cache pk per path
    let mut path_pks: Vec<(Vec<u32>, SchnorrPubkey)> = Vec::new();
    for path in signer_paths.iter() {
        let _req = Msg {
            v: PROTO_V1,
            id: 0x4100,
            msg: Frame::One(Request::GetCheetahPub { path: path.clone() }),
        };
        let resp: Msg<Response> = {
            let m = Msg {
                v: PROTO_V1,
                id: 0x4100,
                msg: Frame::One(Request::GetCheetahPub { path: path.clone() }),
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
    for (name, mut input) in raw.inputs.p.tap() {
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

            let msg5: [u64; 5] = input.spend.to_hash().values;
            let req = Msg {
                v: PROTO_V1,
                id: 0x4200,
                msg: Frame::One(Request::SignSpendHash {
                    path: path.clone(),
                    msg5,
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

    raw.inputs = Inputs { p: new_inputs };
    Ok(raw)
}

pub fn sig_hash_for_input(raw: &RawTransaction, name: &NName) -> [u64; 5] {
    // clone + strip sigs from the target spend
    let mut spend = raw.inputs.p.get(name).expect("input missing").spend.clone();
    spend.signature = None;

    // build a canonical noun for signing: [tx.timelock, name, spend]
    let mut slab: NounSlab = NounSlab::new();
    let n_timelock = raw.timelock_range.to_noun(&mut slab);
    let n_name = name.to_noun(&mut slab);
    let n_spend = spend.to_noun(&mut slab);
    let signing_n = T(&mut slab, &[n_timelock, n_name, n_spend]);

    Tip5Hasher::hash_noun_varlen(signing_n).unwrap().values // -> [u64;5]
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
