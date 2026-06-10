//! Host-side draft review: decode a jammed v1 draft/`.psnt` and surface the
//! same facts the device shows on its review screen — recipients, gifts,
//! refund/change, bridge deposits, output lock primitives, and per-input
//! multisig coordination state.
//!
//! This mirrors `nockster_core::draft_sign::tx_v1`'s *structural* parsing
//! against `tx_types::pokenoun` (the same codec `compose_v1` uses), avoiding a
//! `nockster-core` dependency that would collide on the `tx-types` std/no_std
//! feature split. The heavy lock-root *verification* (Tip5 hashing) is the
//! device's job and is intentionally not duplicated here — a host preview
//! cannot be a trust anchor anyway.

use serde::Serialize;
use tx_types::pokenoun::{cue, Arena, Noun};
use tx_types::transaction_types::Hash;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PrimitiveView {
    Pkh {
        m: u64,
        n: u64,
    },
    Timelock {
        abs_min: Option<u64>,
        abs_max: Option<u64>,
        rel_min: Option<u64>,
        rel_max: Option<u64>,
    },
    Hax {
        n: u64,
    },
    Burn,
}

#[derive(Serialize)]
pub struct OutputView {
    pub recipient_b58: String,
    pub gift: u64,
    pub is_refund: bool,
    pub bridge_evm_addr: Option<String>,
    /// True if this output is a Base bridge *withdrawal* (`bridge-w` note-data).
    pub bridge_withdrawal: bool,
    /// For an OR-composed lock (`%2/%4/%8/%16` tree), the padded branch count
    /// (2/4/8/16); `None` for a single spend-condition.
    pub or_lock: Option<u64>,
    /// Output lock primitives parsed from `note_data`. The device additionally
    /// verifies these hash to the committed lock-root; this host view does not.
    pub lock: Option<Vec<PrimitiveView>>,
}

#[derive(Serialize)]
pub struct MultisigInputView {
    pub m: u64,
    pub n: u64,
    pub present: u64,
    pub we_authorized: bool,
    pub we_signed: bool,
}

#[derive(Serialize)]
pub struct ReviewView {
    pub outputs: Vec<OutputView>,
    pub input_count: u32,
    pub external_total: u64,
    pub refund_total: u64,
    pub fee_total: u64,
    pub multisig_inputs: Vec<MultisigInputView>,
}

// ---- structural noun helpers (mirror tx_v1) --------------------------------

fn uncons(noun: Noun, arena: &Arena) -> Option<(Noun, Noun)> {
    match noun {
        Noun::Cell(id) => {
            let cell = arena.cell(id);
            Some((cell.head, cell.tail))
        }
        _ => None,
    }
}

fn tuple2(noun: Noun, arena: &Arena) -> Option<(Noun, Noun)> {
    uncons(noun, arena)
}

fn tuple3(noun: Noun, arena: &Arena) -> Option<(Noun, Noun, Noun)> {
    let (a, bc) = uncons(noun, arena)?;
    let (b, c) = uncons(bc, arena)?;
    Some((a, b, c))
}

fn tuple4(noun: Noun, arena: &Arena) -> Option<(Noun, Noun, Noun, Noun)> {
    let (a, bcd) = uncons(noun, arena)?;
    let (b, cd) = uncons(bcd, arena)?;
    let (c, d) = uncons(cd, arena)?;
    Some((a, b, c, d))
}

fn tuple5(noun: Noun, arena: &Arena) -> Option<(Noun, Noun, Noun, Noun, Noun)> {
    let (a, rest) = uncons(noun, arena)?;
    let (b, c, de) = tuple3(rest, arena)?;
    let (d, e) = uncons(de, arena)?;
    Some((a, b, c, d, e))
}

fn noun_atom_u64(noun: Noun, arena: &Arena) -> Option<u64> {
    match noun {
        Noun::Atom(id) => arena.atom_u64(id),
        _ => None,
    }
}

fn atom_eq_bytes(noun: Noun, bytes: &[u8], arena: &Arena) -> bool {
    match noun {
        Noun::Atom(id) => arena.atom_bytes(id) == bytes,
        _ => false,
    }
}

fn parse_hash(noun: Noun, arena: &Arena) -> Option<[u64; 5]> {
    let (a0, rest) = uncons(noun, arena)?;
    let (a1, rest) = uncons(rest, arena)?;
    let (a2, rest) = uncons(rest, arena)?;
    let (a3, a4) = uncons(rest, arena)?;
    Some([
        noun_atom_u64(a0, arena)?,
        noun_atom_u64(a1, arena)?,
        noun_atom_u64(a2, arena)?,
        noun_atom_u64(a3, arena)?,
        noun_atom_u64(a4, arena)?,
    ])
}

fn decompose_map(map: Noun, arena: &Arena) -> Option<(Noun, Noun, Noun)> {
    let (node, tail) = uncons(map, arena)?;
    if let Some((left, right)) = uncons(tail, arena) {
        Some((node, left, right))
    } else {
        Some((node, tail, arena.atom0()))
    }
}

fn decompose_pair(pair: Noun, arena: &Arena) -> Option<(Noun, Noun)> {
    uncons(pair, arena)
}

fn decompose_lock_merkle_proof(lmp: Noun, arena: &Arena) -> Option<(Noun, Noun)> {
    // Returns (spend_condition, _) — we only need the spend condition.
    let (head, tail) = uncons(lmp, arena)?;
    if atom_eq_bytes(head, b"full", arena) {
        let (spend_condition, _axis, _merk) = tuple3(tail, arena)?;
        Some((spend_condition, tail))
    } else {
        Some((head, tail))
    }
}

fn note_data_find(map: Noun, arena: &Arena, key: &[u8]) -> Option<Noun> {
    if map == arena.atom0() {
        return None;
    }
    let (node, left, right) = decompose_map(map, arena)?;
    let (k, v) = decompose_pair(node, arena)?;
    if atom_eq_bytes(k, key, arena) {
        return Some(v);
    }
    note_data_find(left, arena, key).or_else(|| note_data_find(right, arena, key))
}

fn zset_count(set: Noun, arena: &Arena, limit: u64) -> u64 {
    if limit == 0 || set == arena.atom0() {
        return 0;
    }
    let Some((_value, lr)) = uncons(set, arena) else {
        return 0;
    };
    let Some((left, right)) = uncons(lr, arena) else {
        return 1;
    };
    let mut count = 1u64;
    count += zset_count(left, arena, limit - count);
    if count >= limit {
        return count;
    }
    count + zset_count(right, arena, limit - count)
}

fn parse_unit_u64(opt: Noun, arena: &Arena) -> Option<u64> {
    match opt {
        Noun::Atom(_) => None,
        Noun::Cell(_) => {
            let (_null, v) = uncons(opt, arena)?;
            noun_atom_u64(v, arena)
        }
    }
}

fn parse_timelock_range(range: Noun, arena: &Arena) -> Option<(Option<u64>, Option<u64>)> {
    let (min, max) = tuple2(range, arena)?;
    Some((parse_unit_u64(min, arena), parse_unit_u64(max, arena)))
}

fn parse_lock_primitive(lp: Noun, arena: &Arena) -> Option<PrimitiveView> {
    let (header, body) = tuple2(lp, arena)?;
    if atom_eq_bytes(header, b"pkh", arena) {
        let (m, h) = tuple2(body, arena)?;
        Some(PrimitiveView::Pkh {
            m: noun_atom_u64(m, arena)?,
            n: zset_count(h, arena, 64),
        })
    } else if atom_eq_bytes(header, b"tim", arena) {
        let (rel, abs) = tuple2(body, arena)?;
        let (rel_min, rel_max) = parse_timelock_range(rel, arena)?;
        let (abs_min, abs_max) = parse_timelock_range(abs, arena)?;
        Some(PrimitiveView::Timelock {
            abs_min,
            abs_max,
            rel_min,
            rel_max,
        })
    } else if atom_eq_bytes(header, b"hax", arena) {
        Some(PrimitiveView::Hax {
            n: zset_count(body, arena, 64),
        })
    } else if atom_eq_bytes(header, b"brn", arena) {
        Some(PrimitiveView::Burn)
    } else {
        None
    }
}

/// If the note_data lock is an OR-composed tree (`%2/%4/%8/%16`), return the
/// padded branch count; else `None` (single spend-condition).
fn parse_or_lock(note_data: Noun, arena: &Arena) -> Option<u64> {
    let lock_data = note_data_find(note_data, arena, b"lock")?;
    let (ver, sc) = tuple2(lock_data, arena)?;
    if noun_atom_u64(ver, arena) != Some(0) {
        return None;
    }
    // A tree's head is an atom tag (2/4/8/16); a single spend-condition's head
    // is a primitive cell.
    let (head, _tail) = uncons(sc, arena)?;
    match noun_atom_u64(head, arena) {
        Some(n @ (2 | 4 | 8 | 16)) => Some(n),
        _ => None,
    }
}

fn parse_lock(note_data: Noun, arena: &Arena) -> Option<Vec<PrimitiveView>> {
    let lock_data = note_data_find(note_data, arena, b"lock")?;
    let (ver, sc) = tuple2(lock_data, arena)?;
    if noun_atom_u64(ver, arena) != Some(0) {
        return None;
    }
    let mut prims = Vec::new();
    let mut cursor = sc;
    while cursor != arena.atom0() {
        let (head, tail) = uncons(cursor, arena)?;
        prims.push(parse_lock_primitive(head, arena)?);
        cursor = tail;
    }
    if prims.is_empty() {
        None
    } else {
        Some(prims)
    }
}

fn be20_mul_add(buf: &mut [u8; 20], mul: u64, add: u64) -> bool {
    let mut carry = add as u128;
    for byte in buf.iter_mut().rev() {
        let v = (*byte as u128) * (mul as u128) + carry;
        *byte = (v & 0xff) as u8;
        carry = v >> 8;
    }
    carry == 0
}

fn parse_bridge_evm(note_data: Noun, arena: &Arena) -> Option<String> {
    let value = note_data_find(note_data, arena, b"bridge")?;
    let (ver, rest) = tuple2(value, arena)?;
    if noun_atom_u64(ver, arena) != Some(0) {
        return None;
    }
    let (base_tag, abc) = tuple2(rest, arena)?;
    if !atom_eq_bytes(base_tag, b"base", arena) {
        return None;
    }
    let (a, bc) = tuple2(abc, arena)?;
    let (b, c) = tuple2(bc, arena)?;
    let a = noun_atom_u64(a, arena)?;
    let b = noun_atom_u64(b, arena)?;
    let c = noun_atom_u64(c, arena)?;
    // Goldilocks prime p = 2^64 - 2^32 + 1.
    let p = 0xFFFF_FFFF_0000_0001u64;
    let mut bytes = [0u8; 20];
    if !be20_mul_add(&mut bytes, 1, c)
        || !be20_mul_add(&mut bytes, p, b)
        || !be20_mul_add(&mut bytes, p, a)
    {
        return None;
    }
    let mut out = String::from("0x");
    for byte in &bytes {
        out.push(core::char::from_digit((byte >> 4) as u32, 16).unwrap());
        out.push(core::char::from_digit((byte & 0xf) as u32, 16).unwrap());
    }
    Some(out)
}

/// Single recipient pkh from the note_data "lock" (m-of-1 pkh case), else None.
fn seed_recipient_pkh(note_data: Noun, arena: &Arena) -> Option<[u64; 5]> {
    let lock_data = note_data_find(note_data, arena, b"lock")?;
    let (ver, lock) = tuple2(lock_data, arena)?;
    if noun_atom_u64(ver, arena) != Some(0) {
        return None;
    }
    let (prim, _rest) = uncons(lock, arena)?;
    let (header, body) = tuple2(prim, arena)?;
    if !atom_eq_bytes(header, b"pkh", arena) {
        return None;
    }
    let (_m, h_set) = tuple2(body, arena)?;
    if zset_count(h_set, arena, 2) != 1 {
        return None;
    }
    let (value, _lr) = uncons(h_set, arena)?;
    parse_hash(value, arena)
}

fn digest_to_b58(digest: [u64; 5]) -> String {
    Hash { values: digest }.to_b58()
}

// ---- multisig input parsing ------------------------------------------------

fn spend_condition_pkh_lock(spend_condition: Noun, arena: &Arena) -> Option<(u64, Noun)> {
    if spend_condition == arena.atom0() {
        return None;
    }
    let (head, tail) = uncons(spend_condition, arena)?;
    let (header, body) = tuple2(head, arena)?;
    if atom_eq_bytes(header, b"pkh", arena) {
        let (m, h_set) = tuple2(body, arena)?;
        let m_u64 = noun_atom_u64(m, arena)?;
        if m_u64 == 0 {
            return None;
        }
        return Some((m_u64, h_set));
    }
    spend_condition_pkh_lock(tail, arena)
}

fn zset_contains_hash(set: Noun, arena: &Arena, want: [u64; 5]) -> bool {
    if set == arena.atom0() {
        return false;
    }
    let Some((value, left, right)) = tuple3(set, arena) else {
        return false;
    };
    if parse_hash(value, arena) == Some(want) {
        return true;
    }
    zset_contains_hash(left, arena, want) || zset_contains_hash(right, arena, want)
}

fn zset_count_members(set: Noun, arena: &Arena, limit: u64) -> u64 {
    if limit == 0 || set == arena.atom0() {
        return 0;
    }
    let Some((_value, left, right)) = tuple3(set, arena) else {
        return 0;
    };
    let mut count = 1u64;
    count += zset_count_members(left, arena, limit - count);
    if count >= limit {
        return count;
    }
    count + zset_count_members(right, arena, limit - count)
}

fn tuple_all_u64_eq(noun: Noun, arena: &Arena, count: usize, want: u64) -> bool {
    if count == 0 {
        return noun == arena.atom0();
    }
    let mut cur = noun;
    for _ in 0..count.saturating_sub(1) {
        let Some((head, tail)) = uncons(cur, arena) else {
            return false;
        };
        if noun_atom_u64(head, arena) != Some(want) {
            return false;
        }
        cur = tail;
    }
    noun_atom_u64(cur, arena) == Some(want)
}

fn is_placeholder_sig(value: Noun, arena: &Arena) -> bool {
    let Some((pk, sig)) = tuple2(value, arena) else {
        return true;
    };
    let Some((x, y, _inf)) = tuple3(pk, arena) else {
        return true;
    };
    if !tuple_all_u64_eq(x, arena, 6, 0) || !tuple_all_u64_eq(y, arena, 6, 0) {
        return false;
    }
    let Some((chal, sig_s)) = tuple2(sig, arena) else {
        return true;
    };
    tuple_all_u64_eq(chal, arena, 8, 0) && tuple_all_u64_eq(sig_s, arena, 8, 0)
}

fn zmap_count_real_sigs(map: Noun, arena: &Arena, limit: u64) -> u64 {
    if limit == 0 || map == arena.atom0() {
        return 0;
    }
    let Some((node, left, right)) = decompose_map(map, arena) else {
        return 0;
    };
    let mut count = match decompose_pair(node, arena) {
        Some((_k, v)) if !is_placeholder_sig(v, arena) => 1,
        _ => 0,
    };
    if count >= limit {
        return count;
    }
    count += zmap_count_real_sigs(left, arena, limit - count);
    if count >= limit {
        return count;
    }
    count + zmap_count_real_sigs(right, arena, limit - count)
}

fn zmap_has_real_sig_for(map: Noun, arena: &Arena, want: [u64; 5]) -> bool {
    if map == arena.atom0() {
        return false;
    }
    let Some((node, left, right)) = decompose_map(map, arena) else {
        return false;
    };
    if let Some((key, value)) = decompose_pair(node, arena) {
        if parse_hash(key, arena) == Some(want) && !is_placeholder_sig(value, arena) {
            return true;
        }
    }
    zmap_has_real_sig_for(left, arena, want) || zmap_has_real_sig_for(right, arena, want)
}

// ---- walk ------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn walk_seeds(
    seeds: Noun,
    arena: &Arena,
    signer_pkh: [u64; 5],
    outputs: &mut Vec<OutputView>,
    refund_total: &mut u64,
    external_total: &mut u64,
) -> Option<()> {
    if seeds == arena.atom0() {
        return Some(());
    }
    let (seed, lr) = uncons(seeds, arena)?;
    let (left, right) = uncons(lr, arena)?;

    let (_src, lock_root, note_data, gift_noun, _parent) = tuple5(seed, arena)?;
    let gift = noun_atom_u64(gift_noun, arena)?;
    if gift != 0 {
        let lock_root_digest = parse_hash(lock_root, arena)?;
        let recipient = seed_recipient_pkh(note_data, arena).unwrap_or(lock_root_digest);
        let is_refund = recipient == signer_pkh;
        if is_refund {
            *refund_total = refund_total.checked_add(gift)?;
        } else {
            *external_total = external_total.checked_add(gift)?;
        }
        outputs.push(OutputView {
            recipient_b58: digest_to_b58(recipient),
            gift,
            is_refund,
            bridge_evm_addr: parse_bridge_evm(note_data, arena),
            bridge_withdrawal: note_data_find(note_data, arena, b"bridge-w").is_some(),
            or_lock: parse_or_lock(note_data, arena),
            lock: parse_lock(note_data, arena),
        });
    }

    walk_seeds(left, arena, signer_pkh, outputs, refund_total, external_total)?;
    walk_seeds(right, arena, signer_pkh, outputs, refund_total, external_total)
}

fn walk_spends(
    spends: Noun,
    arena: &Arena,
    signer_pkh: [u64; 5],
    view: &mut ReviewView,
) -> Option<()> {
    if spends == arena.atom0() {
        return Some(());
    }
    let (node, left, right) = decompose_map(spends, arena)?;
    let (_name, spend) = decompose_pair(node, arena)?;
    // spend = [tag body]: tag 1 = v1 witness, tag 0 = legacy v0 signature map.
    // Both share [_ seeds fee]; only the v1 witness carries multisig coordination.
    let (tag, body) = tuple2(spend, arena)?;
    let (first, seeds, fee) = tuple3(body, arena)?;
    view.input_count = view.input_count.checked_add(1)?;
    view.fee_total = view.fee_total.checked_add(noun_atom_u64(fee, arena)?)?;

    // Multisig coordination is only present in the v1 witness (first = witness).
    if noun_atom_u64(tag, arena) == Some(1) {
        if let Some((lmp, pkh_map, _hax, _tim)) = tuple4(first, arena) {
            if let Some((spend_condition, _)) = decompose_lock_merkle_proof(lmp, arena) {
                if let Some((m, allowed)) = spend_condition_pkh_lock(spend_condition, arena) {
                    let n = zset_count_members(allowed, arena, 64);
                    if n > 1 {
                        view.multisig_inputs.push(MultisigInputView {
                            m,
                            n,
                            present: zmap_count_real_sigs(pkh_map, arena, 64),
                            we_authorized: zset_contains_hash(allowed, arena, signer_pkh),
                            we_signed: zmap_has_real_sig_for(pkh_map, arena, signer_pkh),
                        });
                    }
                }
            }
        }
    }

    let mut refund = 0u64;
    let mut external = 0u64;
    walk_seeds(
        seeds,
        arena,
        signer_pkh,
        &mut view.outputs,
        &mut refund,
        &mut external,
    )?;
    view.refund_total = view.refund_total.checked_add(refund)?;
    view.external_total = view.external_total.checked_add(external)?;

    walk_spends(left, arena, signer_pkh, view)?;
    walk_spends(right, arena, signer_pkh, view)
}

/// Detect and return the spends z-map for the supported wrapper shapes.
fn find_spends(root: Noun, arena: &Arena) -> Option<Noun> {
    if let Some((ver, id, spends)) = tuple3(root, arena) {
        if noun_atom_u64(ver, arena) == Some(1) && parse_hash(id, arena).is_some() {
            return Some(spends);
        }
    }
    let (head, tail) = uncons(root, arena)?;
    if let Some((ver, id, spends)) = tuple3(head, arena) {
        if noun_atom_u64(ver, arena) == Some(1) && parse_hash(id, arena).is_some() {
            return Some(spends);
        }
        return None;
    }
    if matches!(head, Noun::Atom(_)) {
        if noun_atom_u64(head, arena) == Some(1) {
            let (_name, spends, _display, _witness) = tuple4(tail, arena)?;
            return Some(spends);
        }
        return Some(tail);
    }
    None
}

// ---- typed inspector tree -------------------------------------------------

/// A node in the human-meaningful transaction tree. `value` is rendered for
/// display (b58 hashes, readable amounts, lock labels); `children` recurse.
#[derive(Serialize)]
pub struct TxTreeNode {
    pub label: String,
    pub value: String,
    pub children: Vec<TxTreeNode>,
}

fn tnode(label: &str, value: impl Into<String>, children: Vec<TxTreeNode>) -> TxTreeNode {
    TxTreeNode {
        label: label.to_string(),
        value: value.into(),
        children,
    }
}

fn short_b58(s: &str) -> String {
    if s.len() <= 16 {
        s.to_string()
    } else {
        format!("{}…{}", &s[..8], &s[s.len() - 6..])
    }
}

fn fmt_amount(nicks: u64) -> String {
    const N: u64 = 1 << 16;
    if nicks >= N {
        let whole = nicks / N;
        let frac = ((nicks % N) * 100 + N / 2) / N;
        format!("{nicks} nicks ({whole}.{frac:02} ℕ)")
    } else {
        format!("{nicks} nicks")
    }
}

/// Collect all member hashes from a z-set of hashes.
fn zset_collect_hashes(set: Noun, arena: &Arena, out: &mut Vec<[u64; 5]>) {
    if set == arena.atom0() || out.len() >= 64 {
        return;
    }
    let Some((value, left, right)) = tuple3(set, arena) else {
        return;
    };
    if let Some(h) = parse_hash(value, arena) {
        out.push(h);
    }
    zset_collect_hashes(left, arena, out);
    zset_collect_hashes(right, arena, out);
}

/// One lock spend-condition's primitives → tree nodes (with real signer b58).
fn spend_condition_nodes(sc: Noun, arena: &Arena) -> Vec<TxTreeNode> {
    let mut nodes = Vec::new();
    let mut cursor = sc;
    let mut guard = 0;
    while cursor != arena.atom0() && guard < 64 {
        guard += 1;
        let Some((prim, tail)) = uncons(cursor, arena) else {
            break;
        };
        cursor = tail;
        let Some((header, body)) = tuple2(prim, arena) else {
            continue;
        };
        if atom_eq_bytes(header, b"pkh", arena) {
            if let Some((m, h)) = tuple2(body, arena) {
                let m = noun_atom_u64(m, arena).unwrap_or(0);
                let mut hashes = Vec::new();
                zset_collect_hashes(h, arena, &mut hashes);
                let n = hashes.len();
                let label = if n > m as usize {
                    format!("{m}-of-{n} multisig")
                } else {
                    "single-sig (p2pkh)".to_string()
                };
                let signers = hashes
                    .iter()
                    .map(|hh| tnode("signer", digest_to_b58(*hh), vec![]))
                    .collect();
                nodes.push(tnode("pkh", label, signers));
            }
        } else if atom_eq_bytes(header, b"tim", arena) {
            let mut bounds = Vec::new();
            if let Some((rel, abs)) = tuple2(body, arena) {
                if let Some((mn, mx)) = parse_timelock_range(rel, arena) {
                    if let Some(v) = mn {
                        bounds.push(tnode("rel min", format!("+{v}"), vec![]));
                    }
                    if let Some(v) = mx {
                        bounds.push(tnode("rel max", format!("+{v}"), vec![]));
                    }
                }
                if let Some((mn, mx)) = parse_timelock_range(abs, arena) {
                    if let Some(v) = mn {
                        bounds.push(tnode("abs min", format!("height ≥ {v}"), vec![]));
                    }
                    if let Some(v) = mx {
                        bounds.push(tnode("abs max", format!("height ≤ {v}"), vec![]));
                    }
                }
            }
            nodes.push(tnode("timelock", "spendable within bounds", bounds));
        } else if atom_eq_bytes(header, b"hax", arena) {
            let mut hashes = Vec::new();
            zset_collect_hashes(body, arena, &mut hashes);
            let commits = hashes
                .iter()
                .map(|hh| tnode("commitment", digest_to_b58(*hh), vec![]))
                .collect();
            nodes.push(tnode(
                "hashlock",
                format!("{} preimage commitment(s)", hashes.len()),
                commits,
            ));
        } else if atom_eq_bytes(header, b"brn", arena) {
            nodes.push(tnode("burn", "unspendable", vec![]));
        }
    }
    nodes
}

/// Lock node from note_data — single spend-condition or OR-composed tree.
fn lock_node(note_data: Noun, arena: &Arena) -> Option<TxTreeNode> {
    let lock_data = note_data_find(note_data, arena, b"lock")?;
    let (ver, sc) = tuple2(lock_data, arena)?;
    if noun_atom_u64(ver, arena) != Some(0) {
        return None;
    }
    if let Some(size) = parse_or_lock(note_data, arena) {
        return Some(tnode(
            "lock",
            format!("OR-composed ({size}-branch) — any branch can spend"),
            vec![],
        ));
    }
    let children = spend_condition_nodes(sc, arena);
    if children.is_empty() {
        return None;
    }
    Some(tnode("lock", "all conditions required", children))
}

fn seed_node(seed: Noun, arena: &Arena) -> Option<TxTreeNode> {
    let (_src, lock_root, note_data, gift_noun, _parent) = tuple5(seed, arena)?;
    let gift = noun_atom_u64(gift_noun, arena)?;
    let recipient = seed_recipient_pkh(note_data, arena)
        .or_else(|| parse_hash(lock_root, arena))
        .map(digest_to_b58)
        .unwrap_or_default();
    let mut children = vec![tnode("amount", fmt_amount(gift), vec![])];
    if let Some(addr) = parse_bridge_evm(note_data, arena) {
        children.push(tnode("bridge", format!("Base deposit → {addr}"), vec![]));
    }
    if note_data_find(note_data, arena, b"bridge-w").is_some() {
        children.push(tnode("bridge", "Base withdrawal", vec![]));
    }
    if let Some(lock) = lock_node(note_data, arena) {
        children.push(lock);
    }
    Some(tnode("output", short_b58(&recipient), children))
}

fn seeds_nodes(seeds: Noun, arena: &Arena, out: &mut Vec<TxTreeNode>) {
    if seeds == arena.atom0() || out.len() >= 64 {
        return;
    }
    let Some((seed, lr)) = uncons(seeds, arena) else {
        return;
    };
    let Some((left, right)) = uncons(lr, arena) else {
        return;
    };
    if let Some(n) = seed_node(seed, arena) {
        out.push(n);
    }
    seeds_nodes(left, arena, out);
    seeds_nodes(right, arena, out);
}

fn spend_node(name: Noun, spend: Noun, arena: &Arena) -> Option<TxTreeNode> {
    let (tag, body) = tuple2(spend, arena)?;
    let (first, seeds, fee) = tuple3(body, arena)?;
    let name_b58 = parse_hash(name, arena).map(digest_to_b58).unwrap_or_default();
    let mut children = vec![tnode("fee", fmt_amount(noun_atom_u64(fee, arena)?), vec![])];

    // Witness: spend-condition (lock branch being satisfied) + signatures.
    if noun_atom_u64(tag, arena) == Some(1) {
        if let Some((lmp, pkh_map, _hax, _tim)) = tuple4(first, arena) {
            if let Some((sc, _)) = decompose_lock_merkle_proof(lmp, arena) {
                let sc_nodes = spend_condition_nodes(sc, arena);
                if !sc_nodes.is_empty() {
                    children.push(tnode("unlock condition", "", sc_nodes));
                }
            }
            let present = zmap_count_real_sigs(pkh_map, arena, 64);
            if present > 0 {
                children.push(tnode("signatures", format!("{present} present"), vec![]));
            }
        }
    } else {
        children.push(tnode("legacy", "v0 signature spend", vec![]));
    }

    let mut seed_children = Vec::new();
    seeds_nodes(seeds, arena, &mut seed_children);
    children.push(tnode("outputs", format!("{}", seed_children.len()), seed_children));
    Some(tnode("input", short_b58(&name_b58), children))
}

fn spends_nodes(spends: Noun, arena: &Arena, out: &mut Vec<TxTreeNode>) {
    if spends == arena.atom0() || out.len() >= 64 {
        return;
    }
    let Some((node, left, right)) = decompose_map(spends, arena) else {
        return;
    };
    if let Some((name, spend)) = decompose_pair(node, arena) {
        if let Some(n) = spend_node(name, spend, arena) {
            out.push(n);
        }
    }
    spends_nodes(left, arena, out);
    spends_nodes(right, arena, out);
}

/// Build a human-meaningful typed tree for a jammed v1 transaction/draft.
pub fn inspect_tx(jam: &[u8]) -> Result<TxTreeNode, String> {
    let mut arena = Arena::new();
    let root = cue(jam, &mut arena).map_err(|_| "not a valid jammed noun".to_string())?;
    let spends = find_spends(root, &arena).ok_or("unsupported draft shape".to_string())?;
    let mut spend_children = Vec::new();
    spends_nodes(spends, &arena, &mut spend_children);
    let n = spend_children.len();
    Ok(tnode(
        "transaction",
        format!("v1 · {n} input(s)"),
        vec![tnode("inputs", format!("{n}"), spend_children)],
    ))
}

pub fn review(jam: &[u8], source_pkh_b58: &str) -> Result<ReviewView, String> {
    let signer_pkh = Hash::from_b58(source_pkh_b58)
        .map(|h| h.values)
        .unwrap_or([u64::MAX; 5]); // unparseable pkh: nothing will match as refund
    let mut arena = Arena::new();
    let root = cue(jam, &mut arena).map_err(|_| "not a valid jammed noun".to_string())?;
    let spends = find_spends(root, &arena).ok_or("unsupported draft shape".to_string())?;

    let mut view = ReviewView {
        outputs: Vec::new(),
        input_count: 0,
        external_total: 0,
        refund_total: 0,
        fee_total: 0,
        multisig_inputs: Vec::new(),
    };
    walk_spends(spends, &arena, signer_pkh, &mut view).ok_or("malformed draft".to_string())?;
    Ok(view)
}
