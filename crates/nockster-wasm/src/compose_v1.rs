use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use tx_types::pokenoun::{
    canonical_zmap_put, canonical_zset_put, hash_noun_varlen, hash_ten_cell, jam, Arena, Noun,
};

const BYTHOS_PHASE: u64 = 54_000;
const BASE_FEE: u64 = 1 << 14;
const INPUT_FEE_DIVISOR: u64 = 4;
const MIN_FEE: u64 = 256;
const DEFAULT_COINBASE_REL_MIN: u64 = 100;
const LOCK_MERKLE_PROOF_FULL_TAG: &[u8] = b"full";

// tx-types constant:
// Hash::from_b58("6mhCSwJQDvbkbiPAUNjetJtVoo1VLtEhmEYoU4hmdGd6ep1F6ayaV4A")
const LOCK_MERKLE_AXIS_HASH: [u64; 5] = [
    1988594973584463658,
    8158631336633700141,
    2161567007650232260,
    460329990575991155,
    8368574252173164961,
];

fn js_err(msg: impl AsRef<str>) -> JsValue {
    JsValue::from_str(msg.as_ref())
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComposeTxV1Input {
    pub source_pkh: String,
    pub notes: Vec<NoteInput>,
    pub outputs: Vec<OutputInput>,
    #[serde(default)]
    pub coinbase_rel_min: Option<u64>,
    #[serde(default)]
    pub current_height: Option<u64>,
    #[serde(default)]
    pub bythos_phase: Option<u64>,
    #[serde(default)]
    pub base_fee: Option<u64>,
    #[serde(default)]
    pub input_fee_divisor: Option<u64>,
    #[serde(default)]
    pub min_fee: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NoteInput {
    pub name_first: String,
    pub name_last: String,
    pub origin_page: u64,
    pub assets: u64,
    #[serde(default = "default_note_version")]
    pub version: u64,
}

fn default_note_version() -> u64 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RecipientInput {
    Pkh(String),
    Multisig { m: u64, pkhs: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputInput {
    pub recipient: RecipientInput,
    pub amount: u64,
    #[serde(default)]
    pub alias: Option<String>,
}

#[wasm_bindgen]
pub struct ComposedTransactionV1 {
    tx_id: String,
    raw_jam: Vec<u8>,
    wallet_jam: Vec<u8>,
    summary_json: String,
}

#[wasm_bindgen]
impl ComposedTransactionV1 {
    #[wasm_bindgen(getter)]
    pub fn tx_id(&self) -> String {
        self.tx_id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn raw_jam(&self) -> Vec<u8> {
        self.raw_jam.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn wallet_jam(&self) -> Vec<u8> {
        self.wallet_jam.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn summary_json(&self) -> String {
        self.summary_json.clone()
    }
}

#[derive(Debug, Clone, Copy)]
struct TxIdCtx {
    null_digest: [u64; 5],
    fake_digest: [u64; 5],
    version_digest: [u64; 5],
}

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
    let (a, bcde) = uncons(noun, arena)?;
    let (b, cde) = uncons(bcde, arena)?;
    let (c, de) = uncons(cde, arena)?;
    let (d, e) = uncons(de, arena)?;
    Some((a, b, c, d, e))
}

fn noun_atom_u64(noun: Noun, arena: &Arena) -> Option<u64> {
    match noun {
        Noun::Atom(id) => arena.atom_u64(id),
        _ => None,
    }
}

fn build_tuple(arena: &mut Arena, elems: &[Noun]) -> Noun {
    if elems.is_empty() {
        return arena.atom0();
    }
    let mut res = *elems.last().unwrap();
    for &n in elems[..elems.len() - 1].iter().rev() {
        res = arena.alloc_cell(n, res);
    }
    res
}

fn build_list(arena: &mut Arena, elems: &[Noun]) -> Noun {
    let mut out = arena.atom0();
    for &elem in elems.iter().rev() {
        out = arena.alloc_cell(elem, out);
    }
    out
}

fn build_hash_noun(arena: &mut Arena, digest: [u64; 5]) -> Noun {
    let elems = [
        arena.alloc_atom_u64(digest[0]),
        arena.alloc_atom_u64(digest[1]),
        arena.alloc_atom_u64(digest[2]),
        arena.alloc_atom_u64(digest[3]),
        arena.alloc_atom_u64(digest[4]),
    ];
    build_tuple(arena, &elems)
}

fn decompose_lock_merkle_proof(
    lmp: Noun,
    arena: &Arena,
) -> Result<(Option<Noun>, Noun, Noun, Noun), JsValue> {
    let (head, tail) = uncons(lmp, arena).ok_or_else(|| js_err("malformed lock merkle proof"))?;

    let is_full = match head {
        Noun::Atom(atom) => arena.atom_eq_bytes(atom, LOCK_MERKLE_PROOF_FULL_TAG),
        _ => false,
    };

    if is_full {
        let (spend_condition, axis, merkle_proof) =
            tuple3(tail, arena).ok_or_else(|| js_err("malformed lock merkle proof"))?;
        return Ok((Some(head), spend_condition, axis, merkle_proof));
    }

    let (axis, merkle_proof) =
        tuple2(tail, arena).ok_or_else(|| js_err("malformed lock merkle proof"))?;
    Ok((None, head, axis, merkle_proof))
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

fn nouns_equal(a: Noun, b: Noun, arena: &Arena) -> bool {
    match (a, b) {
        (Noun::Atom(x), Noun::Atom(y)) => arena.atom_bytes(x) == arena.atom_bytes(y),
        (Noun::Cell(ac), Noun::Cell(bc)) => {
            let a_cell = arena.cell(ac);
            let b_cell = arena.cell(bc);
            nouns_equal(a_cell.head, b_cell.head, arena)
                && nouns_equal(a_cell.tail, b_cell.tail, arena)
        }
        _ => false,
    }
}

fn hashable_noun_digest(noun: Noun, arena: &Arena) -> Result<[u64; 5], JsValue> {
    match noun {
        Noun::Atom(_) => Ok(hash_noun_varlen(noun, arena).map_err(|e| js_err(format!("{e:?}")))?),
        Noun::Cell(id) => {
            let cell = arena.cell(id);
            let lh = hashable_noun_digest(cell.head, arena)?;
            let rh = hashable_noun_digest(cell.tail, arena)?;
            Ok(hash_ten_cell(lh, rh).map_err(|e| js_err(format!("{e:?}")))?)
        }
    }
}

fn hash_note_data(map: Noun, arena: &Arena) -> Result<[u64; 5], JsValue> {
    // note-data hashable: empty map hashes as leaf+0
    if map == arena.atom0() {
        return Ok(hash_noun_varlen(arena.atom0(), arena).map_err(|e| js_err(format!("{e:?}")))?);
    }

    let (node, left, right) =
        decompose_map(map, arena).ok_or_else(|| js_err("malformed note-data"))?;
    let (key, value) =
        decompose_pair(node, arena).ok_or_else(|| js_err("malformed note-data node"))?;

    let key_digest = hash_noun_varlen(key, arena).map_err(|e| js_err(format!("{e:?}")))?;
    let value_digest = hashable_noun_digest(value, arena)?;
    let node_digest =
        hash_ten_cell(key_digest, value_digest).map_err(|e| js_err(format!("{e:?}")))?;

    let left_digest = hash_note_data(left, arena)?;
    let right_digest = hash_note_data(right, arena)?;
    let children_digest =
        hash_ten_cell(left_digest, right_digest).map_err(|e| js_err(format!("{e:?}")))?;
    Ok(hash_ten_cell(node_digest, children_digest).map_err(|e| js_err(format!("{e:?}")))?)
}

fn hash_nname_hashable(noun: Noun, arena: &Arena) -> Result<[u64; 5], JsValue> {
    let (first_noun, rest) = uncons(noun, arena).ok_or_else(|| js_err("malformed nname"))?;
    let (second_noun, end) = uncons(rest, arena).ok_or_else(|| js_err("malformed nname"))?;
    let first = parse_hash(first_noun, arena).ok_or_else(|| js_err("malformed nname hash"))?;
    let second = parse_hash(second_noun, arena).ok_or_else(|| js_err("malformed nname hash"))?;
    let end_digest = hash_noun_varlen(end, arena).map_err(|e| js_err(format!("{e:?}")))?;
    let tail = hash_ten_cell(second, end_digest).map_err(|e| js_err(format!("{e:?}")))?;
    Ok(hash_ten_cell(first, tail).map_err(|e| js_err(format!("{e:?}")))?)
}

fn hash_zset_hashes(set: Noun, arena: &Arena, ctx: &TxIdCtx) -> Result<[u64; 5], JsValue> {
    if set == arena.atom0() {
        return Ok(ctx.null_digest);
    }
    let (value, left, right) = tuple3(set, arena).ok_or_else(|| js_err("malformed zset"))?;
    let value_digest = parse_hash(value, arena).ok_or_else(|| js_err("malformed hash in zset"))?;
    let left_digest = hash_zset_hashes(left, arena, ctx)?;
    let right_digest = hash_zset_hashes(right, arena, ctx)?;
    let children_digest =
        hash_ten_cell(left_digest, right_digest).map_err(|e| js_err(format!("{e:?}")))?;
    Ok(hash_ten_cell(value_digest, children_digest).map_err(|e| js_err(format!("{e:?}")))?)
}

fn hash_optional_leaf(opt: Noun, arena: &Arena, ctx: &TxIdCtx) -> Result<[u64; 5], JsValue> {
    if opt == arena.atom0() {
        return Ok(ctx.null_digest);
    }
    let (tag, value) = tuple2(opt, arena).ok_or_else(|| js_err("malformed option"))?;
    if tag != arena.atom0() {
        return Err(js_err("malformed option tag"));
    }
    let value_digest = hash_noun_varlen(value, arena).map_err(|e| js_err(format!("{e:?}")))?;
    Ok(hash_ten_cell(ctx.null_digest, value_digest).map_err(|e| js_err(format!("{e:?}")))?)
}

fn hash_timelock_range(range: Noun, arena: &Arena, ctx: &TxIdCtx) -> Result<[u64; 5], JsValue> {
    let (min, max) = tuple2(range, arena).ok_or_else(|| js_err("malformed timelock range"))?;
    let min_digest = hash_optional_leaf(min, arena, ctx)?;
    let max_digest = hash_optional_leaf(max, arena, ctx)?;
    Ok(hash_ten_cell(min_digest, max_digest).map_err(|e| js_err(format!("{e:?}")))?)
}

fn hash_lock_primitive_hashable(
    lp: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], JsValue> {
    let (header, body) = tuple2(lp, arena).ok_or_else(|| js_err("malformed lock primitive"))?;
    let Noun::Atom(header_id) = header else {
        return Err(js_err("malformed lock primitive header"));
    };

    if arena.atom_eq_bytes(header_id, b"pkh") {
        let (m, h) = tuple2(body, arena).ok_or_else(|| js_err("malformed pkh lock"))?;
        let tag_digest = hash_noun_varlen(header, arena).map_err(|e| js_err(format!("{e:?}")))?;
        let m_digest = hash_noun_varlen(m, arena).map_err(|e| js_err(format!("{e:?}")))?;
        let h_digest = hash_zset_hashes(h, arena, ctx)?;
        let inner = hash_ten_cell(m_digest, h_digest).map_err(|e| js_err(format!("{e:?}")))?;
        return Ok(hash_ten_cell(tag_digest, inner).map_err(|e| js_err(format!("{e:?}")))?);
    }

    if arena.atom_eq_bytes(header_id, b"tim") {
        let (rel, abs) = tuple2(body, arena).ok_or_else(|| js_err("malformed tim lock"))?;
        let tag_digest = hash_noun_varlen(header, arena).map_err(|e| js_err(format!("{e:?}")))?;
        let rel_digest = hash_timelock_range(rel, arena, ctx)?;
        let abs_digest = hash_timelock_range(abs, arena, ctx)?;
        let inner = hash_ten_cell(rel_digest, abs_digest).map_err(|e| js_err(format!("{e:?}")))?;
        return Ok(hash_ten_cell(tag_digest, inner).map_err(|e| js_err(format!("{e:?}")))?);
    }

    if arena.atom_eq_bytes(header_id, b"hax") {
        let tag_digest = hash_noun_varlen(header, arena).map_err(|e| js_err(format!("{e:?}")))?;
        return Ok(
            hash_ten_cell(tag_digest, ctx.fake_digest).map_err(|e| js_err(format!("{e:?}")))?
        );
    }

    if arena.atom_eq_bytes(header_id, b"brn") {
        let tag_digest = hash_noun_varlen(header, arena).map_err(|e| js_err(format!("{e:?}")))?;
        return Ok(
            hash_ten_cell(tag_digest, ctx.null_digest).map_err(|e| js_err(format!("{e:?}")))?
        );
    }

    Err(js_err("unsupported lock primitive"))
}

fn hash_lock_primitives_list(
    prims: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], JsValue> {
    if prims == arena.atom0() {
        return Ok(ctx.null_digest);
    }
    let (head, tail) =
        uncons(prims, arena).ok_or_else(|| js_err("malformed lock primitive list"))?;
    let head_digest = hash_lock_primitive_hashable(head, arena, ctx)?;
    let tail_digest = hash_lock_primitives_list(tail, arena, ctx)?;
    Ok(hash_ten_cell(head_digest, tail_digest).map_err(|e| js_err(format!("{e:?}")))?)
}

fn hash_merkle_proof_hashable(
    merk: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], JsValue> {
    let (root_noun, path) = tuple2(merk, arena).ok_or_else(|| js_err("malformed merkle proof"))?;
    let root = parse_hash(root_noun, arena).ok_or_else(|| js_err("malformed merkle root"))?;
    let path_digest = hash_hash_list_hashes(path, arena, ctx)?;
    Ok(hash_ten_cell(root, path_digest).map_err(|e| js_err(format!("{e:?}")))?)
}

fn hash_hash_list_hashes(list: Noun, arena: &Arena, ctx: &TxIdCtx) -> Result<[u64; 5], JsValue> {
    if list == arena.atom0() {
        return Ok(ctx.null_digest);
    }
    let (head, tail) = uncons(list, arena).ok_or_else(|| js_err("malformed hash list"))?;
    let head_digest = parse_hash(head, arena).ok_or_else(|| js_err("malformed hash in list"))?;
    let tail_digest = hash_hash_list_hashes(tail, arena, ctx)?;
    Ok(hash_ten_cell(head_digest, tail_digest).map_err(|e| js_err(format!("{e:?}")))?)
}

fn hash_lock_merkle_proof_hashable(
    lmp: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], JsValue> {
    let (version, spend_condition, axis, merk_proof) = decompose_lock_merkle_proof(lmp, arena)?;
    let spend_condition_hash = hash_lock_primitives_list(spend_condition, arena, ctx)?;
    let merk_digest = hash_merkle_proof_hashable(merk_proof, arena, ctx)?;

    match version {
        Some(version) => {
            let version_digest =
                hash_noun_varlen(version, arena).map_err(|e| js_err(format!("{e:?}")))?;
            let axis_digest =
                hash_noun_varlen(axis, arena).map_err(|e| js_err(format!("{e:?}")))?;
            let inner =
                hash_ten_cell(axis_digest, merk_digest).map_err(|e| js_err(format!("{e:?}")))?;
            let inner =
                hash_ten_cell(spend_condition_hash, inner).map_err(|e| js_err(format!("{e:?}")))?;
            Ok(hash_ten_cell(version_digest, inner).map_err(|e| js_err(format!("{e:?}")))?)
        }
        None => {
            let axis_digest = LOCK_MERKLE_AXIS_HASH;
            let inner =
                hash_ten_cell(axis_digest, merk_digest).map_err(|e| js_err(format!("{e:?}")))?;
            Ok(hash_ten_cell(spend_condition_hash, inner).map_err(|e| js_err(format!("{e:?}")))?)
        }
    }
}

fn hash_hax_map_hashable(map: Noun, arena: &Arena, ctx: &TxIdCtx) -> Result<[u64; 5], JsValue> {
    if map == arena.atom0() {
        return Ok(ctx.null_digest);
    }
    let (node, left, right) =
        decompose_map(map, arena).ok_or_else(|| js_err("malformed hax map"))?;
    let (key, value) =
        decompose_pair(node, arena).ok_or_else(|| js_err("malformed hax map node"))?;

    let key_digest = parse_hash(key, arena).ok_or_else(|| js_err("malformed hax key"))?;
    let value_digest = hashable_noun_digest(value, arena)?;
    let node_digest =
        hash_ten_cell(key_digest, value_digest).map_err(|e| js_err(format!("{e:?}")))?;

    let left_digest = hash_hax_map_hashable(left, arena, ctx)?;
    let right_digest = hash_hax_map_hashable(right, arena, ctx)?;
    let children_digest =
        hash_ten_cell(left_digest, right_digest).map_err(|e| js_err(format!("{e:?}")))?;
    Ok(hash_ten_cell(node_digest, children_digest).map_err(|e| js_err(format!("{e:?}")))?)
}

fn hash_pkh_signature_value_hashable(value: Noun, arena: &Arena) -> Result<[u64; 5], JsValue> {
    let (pk, sig) = tuple2(value, arena).ok_or_else(|| js_err("malformed pkh signature value"))?;
    let pk_digest = hash_noun_varlen(pk, arena).map_err(|e| js_err(format!("{e:?}")))?;
    let sig_digest = hash_noun_varlen(sig, arena).map_err(|e| js_err(format!("{e:?}")))?;
    Ok(hash_ten_cell(pk_digest, sig_digest).map_err(|e| js_err(format!("{e:?}")))?)
}

fn hash_pkh_signature_map_hashable(
    map: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], JsValue> {
    if map == arena.atom0() {
        return Ok(ctx.null_digest);
    }
    let (node, left, right) =
        decompose_map(map, arena).ok_or_else(|| js_err("malformed pkh map"))?;
    let (key, value) =
        decompose_pair(node, arena).ok_or_else(|| js_err("malformed pkh map node"))?;

    let key_digest = parse_hash(key, arena).ok_or_else(|| js_err("malformed pkh key"))?;
    let value_digest = hash_pkh_signature_value_hashable(value, arena)?;
    let node_digest =
        hash_ten_cell(key_digest, value_digest).map_err(|e| js_err(format!("{e:?}")))?;

    let left_digest = hash_pkh_signature_map_hashable(left, arena, ctx)?;
    let right_digest = hash_pkh_signature_map_hashable(right, arena, ctx)?;
    let children_digest =
        hash_ten_cell(left_digest, right_digest).map_err(|e| js_err(format!("{e:?}")))?;
    Ok(hash_ten_cell(node_digest, children_digest).map_err(|e| js_err(format!("{e:?}")))?)
}

fn hash_seed_regular_hashable(seed: Noun, arena: &Arena) -> Result<[u64; 5], JsValue> {
    let (_output_source, lock_root_noun, note_data_noun, gift_noun, parent_hash_noun) =
        tuple5(seed, arena).ok_or_else(|| js_err("malformed seed"))?;

    let lock_root =
        parse_hash(lock_root_noun, arena).ok_or_else(|| js_err("malformed seed lock-root"))?;
    let note_data_hash = hash_note_data(note_data_noun, arena)?;
    let gift_digest = hash_noun_varlen(gift_noun, arena).map_err(|e| js_err(format!("{e:?}")))?;
    let parent_hash =
        parse_hash(parent_hash_noun, arena).ok_or_else(|| js_err("malformed seed parent-hash"))?;

    let mut acc = parent_hash;
    acc = hash_ten_cell(gift_digest, acc).map_err(|e| js_err(format!("{e:?}")))?;
    acc = hash_ten_cell(note_data_hash, acc).map_err(|e| js_err(format!("{e:?}")))?;
    Ok(hash_ten_cell(lock_root, acc).map_err(|e| js_err(format!("{e:?}")))?)
}

fn hash_seeds_regular(seeds_zset: Noun, arena: &Arena, ctx: &TxIdCtx) -> Result<[u64; 5], JsValue> {
    if seeds_zset == arena.atom0() {
        return Ok(ctx.null_digest);
    }

    let (seed, lr) = uncons(seeds_zset, arena).ok_or_else(|| js_err("malformed seeds zset"))?;
    let (left, right) = uncons(lr, arena).ok_or_else(|| js_err("malformed seeds zset"))?;

    let node_digest = hash_seed_regular_hashable(seed, arena)?;
    let left_digest = hash_seeds_regular(left, arena, ctx)?;
    let right_digest = hash_seeds_regular(right, arena, ctx)?;
    let children_digest =
        hash_ten_cell(left_digest, right_digest).map_err(|e| js_err(format!("{e:?}")))?;
    Ok(hash_ten_cell(node_digest, children_digest).map_err(|e| js_err(format!("{e:?}")))?)
}

fn hash_witness_hashable(witness: Noun, arena: &Arena, ctx: &TxIdCtx) -> Result<[u64; 5], JsValue> {
    let (lmp, pkh_map, hax, tim) =
        tuple4(witness, arena).ok_or_else(|| js_err("malformed witness"))?;

    let lmp_hash = hash_lock_merkle_proof_hashable(lmp, arena, ctx)?;
    let pkh_hash = hash_pkh_signature_map_hashable(pkh_map, arena, ctx)?;
    let hax_hash = hash_hax_map_hashable(hax, arena, ctx)?;
    let tim_digest = hash_noun_varlen(tim, arena).map_err(|e| js_err(format!("{e:?}")))?;

    let mut acc = tim_digest;
    acc = hash_ten_cell(hax_hash, acc).map_err(|e| js_err(format!("{e:?}")))?;
    acc = hash_ten_cell(pkh_hash, acc).map_err(|e| js_err(format!("{e:?}")))?;
    Ok(hash_ten_cell(lmp_hash, acc).map_err(|e| js_err(format!("{e:?}")))?)
}

fn hash_spend_v1_hashable(spend: Noun, arena: &Arena, ctx: &TxIdCtx) -> Result<[u64; 5], JsValue> {
    let (ver_noun, body_noun) = tuple2(spend, arena).ok_or_else(|| js_err("malformed spend"))?;
    if noun_atom_u64(ver_noun, arena) != Some(1) {
        return Err(js_err("unsupported spend version"));
    }

    let (witness, seeds, fee) =
        tuple3(body_noun, arena).ok_or_else(|| js_err("malformed spend body"))?;
    let witness_digest = hash_witness_hashable(witness, arena, ctx)?;
    let seeds_digest = hash_seeds_regular(seeds, arena, ctx)?;
    let fee_digest = hash_noun_varlen(fee, arena).map_err(|e| js_err(format!("{e:?}")))?;

    let inner = hash_ten_cell(seeds_digest, fee_digest).map_err(|e| js_err(format!("{e:?}")))?;
    let body_digest = hash_ten_cell(witness_digest, inner).map_err(|e| js_err(format!("{e:?}")))?;
    let ver_digest = hash_noun_varlen(ver_noun, arena).map_err(|e| js_err(format!("{e:?}")))?;
    Ok(hash_ten_cell(ver_digest, body_digest).map_err(|e| js_err(format!("{e:?}")))?)
}

fn hash_spends_hashable(spends: Noun, arena: &Arena, ctx: &TxIdCtx) -> Result<[u64; 5], JsValue> {
    if spends == arena.atom0() {
        return Ok(ctx.null_digest);
    }

    let (node, left, right) =
        decompose_map(spends, arena).ok_or_else(|| js_err("malformed spends"))?;
    let (name_noun, spend_noun) =
        decompose_pair(node, arena).ok_or_else(|| js_err("malformed spends node"))?;

    let name_digest = hash_nname_hashable(name_noun, arena)?;
    let spend_digest = hash_spend_v1_hashable(spend_noun, arena, ctx)?;
    let node_digest =
        hash_ten_cell(name_digest, spend_digest).map_err(|e| js_err(format!("{e:?}")))?;

    let left_digest = hash_spends_hashable(left, arena, ctx)?;
    let right_digest = hash_spends_hashable(right, arena, ctx)?;
    let children_digest =
        hash_ten_cell(left_digest, right_digest).map_err(|e| js_err(format!("{e:?}")))?;
    Ok(hash_ten_cell(node_digest, children_digest).map_err(|e| js_err(format!("{e:?}")))?)
}

fn compute_tx_id_v1(spends: Noun, arena: &Arena, ctx: &TxIdCtx) -> Result<[u64; 5], JsValue> {
    let spends_digest = hash_spends_hashable(spends, arena, ctx)?;
    Ok(hash_ten_cell(ctx.version_digest, spends_digest).map_err(|e| js_err(format!("{e:?}")))?)
}

fn count_leaves(noun: Noun, arena: &Arena) -> u64 {
    match noun {
        Noun::Atom(_) => 1,
        Noun::Cell(id) => {
            let cell = arena.cell(id);
            count_leaves(cell.head, arena) + count_leaves(cell.tail, arena)
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FeeBreakdown {
    seed_words: u64,
    witness_words: u64,
    minimum_fee: u64,
}

#[derive(Debug, Clone, Copy)]
struct FeePolicy {
    bythos_phase: u64,
    base_fee: u64,
    input_fee_divisor: u64,
    min_fee: u64,
}

impl FeePolicy {
    fn from_input(input: &ComposeTxV1Input) -> Self {
        Self {
            bythos_phase: input.bythos_phase.unwrap_or(BYTHOS_PHASE),
            base_fee: input.base_fee.unwrap_or(BASE_FEE),
            input_fee_divisor: input.input_fee_divisor.unwrap_or(INPUT_FEE_DIVISOR),
            min_fee: input.min_fee.unwrap_or(MIN_FEE),
        }
    }
}

fn merge_note_data_into(arena: &mut Arena, src: Noun, dst: &mut Noun) -> Result<(), JsValue> {
    if src == arena.atom0() {
        return Ok(());
    }
    let (node, left, right) =
        decompose_map(src, arena).ok_or_else(|| js_err("malformed note-data map"))?;
    let (key, value) =
        decompose_pair(node, arena).ok_or_else(|| js_err("malformed note-data node"))?;
    *dst =
        canonical_zmap_put(arena, *dst, key, value).map_err(|e| js_err(format!("zmap: {e:?}")))?;
    merge_note_data_into(arena, left, dst)?;
    merge_note_data_into(arena, right, dst)
}

fn collect_seed_note_data(
    arena: &mut Arena,
    seeds: Noun,
    merged: &mut Vec<([u64; 5], Noun)>,
    legacy_words: &mut u64,
    bythos_active: bool,
) -> Result<(), JsValue> {
    if seeds == arena.atom0() {
        return Ok(());
    }
    let (seed, lr) = uncons(seeds, arena).ok_or_else(|| js_err("malformed seeds set"))?;
    let (left, right) = uncons(lr, arena).ok_or_else(|| js_err("malformed seeds children"))?;
    let (_output_source, lock_root_noun, note_data, _gift, _parent_hash) =
        tuple5(seed, arena).ok_or_else(|| js_err("malformed seed"))?;

    if bythos_active {
        let lock_root =
            parse_hash(lock_root_noun, arena).ok_or_else(|| js_err("malformed seed lock root"))?;
        if let Some((_root, data)) = merged.iter_mut().find(|(root, _)| *root == lock_root) {
            merge_note_data_into(arena, note_data, data)?;
        } else {
            let mut data = arena.atom0();
            merge_note_data_into(arena, note_data, &mut data)?;
            merged.push((lock_root, data));
        }
    } else {
        *legacy_words = legacy_words.saturating_add(count_leaves(note_data, arena));
    }

    collect_seed_note_data(arena, left, merged, legacy_words, bythos_active)?;
    collect_seed_note_data(arena, right, merged, legacy_words, bythos_active)
}

fn collect_spend_words(
    arena: &mut Arena,
    spends: Noun,
    merged: &mut Vec<([u64; 5], Noun)>,
    legacy_seed_words: &mut u64,
    witness_words: &mut u64,
    bythos_active: bool,
) -> Result<(), JsValue> {
    if spends == arena.atom0() {
        return Ok(());
    }

    let (node, left, right) =
        decompose_map(spends, arena).ok_or_else(|| js_err("malformed spends"))?;
    let (_name, spend) =
        decompose_pair(node, arena).ok_or_else(|| js_err("malformed spend entry"))?;
    let (version, body) = tuple2(spend, arena).ok_or_else(|| js_err("malformed spend"))?;
    if noun_atom_u64(version, arena) != Some(1) {
        return Err(js_err("composer fee calculation only supports V1 spends"));
    }
    let (witness, seeds, _fee) =
        tuple3(body, arena).ok_or_else(|| js_err("malformed spend body"))?;
    *witness_words = witness_words.saturating_add(count_leaves(witness, arena));
    collect_seed_note_data(arena, seeds, merged, legacy_seed_words, bythos_active)?;

    collect_spend_words(
        arena,
        left,
        merged,
        legacy_seed_words,
        witness_words,
        bythos_active,
    )?;
    collect_spend_words(
        arena,
        right,
        merged,
        legacy_seed_words,
        witness_words,
        bythos_active,
    )
}

fn calculate_transaction_fee(
    arena: &mut Arena,
    spends: Noun,
    current_height: u64,
    fee_policy: FeePolicy,
) -> Result<FeeBreakdown, JsValue> {
    let bythos_active = current_height >= fee_policy.bythos_phase;
    let mut merged = Vec::<([u64; 5], Noun)>::new();
    let mut legacy_seed_words = 0u64;
    let mut witness_words = 0u64;
    collect_spend_words(
        arena,
        spends,
        &mut merged,
        &mut legacy_seed_words,
        &mut witness_words,
        bythos_active,
    )?;
    let merged_seed_words = merged
        .iter()
        .map(|(_root, note_data)| count_leaves(*note_data, arena))
        .sum::<u64>();
    let seed_words = legacy_seed_words.saturating_add(merged_seed_words);
    let effective_base_fee = if bythos_active {
        fee_policy.base_fee
    } else {
        fee_policy.base_fee.saturating_mul(2)
    };
    let witness_divisor = if bythos_active {
        fee_policy.input_fee_divisor.max(1)
    } else {
        1
    };
    let seed_fee = seed_words.saturating_mul(effective_base_fee);
    let witness_fee = witness_words
        .saturating_mul(effective_base_fee)
        .saturating_div(witness_divisor);
    let minimum_fee = seed_fee.saturating_add(witness_fee).max(fee_policy.min_fee);

    Ok(FeeBreakdown {
        seed_words,
        witness_words,
        minimum_fee,
    })
}

fn parse_b58_hash(s: &str) -> Result<[u64; 5], JsValue> {
    let h = tx_types::transaction_types::Hash::from_b58(s)
        .map_err(|e| js_err(format!("invalid base58 hash: {e}")))?;
    Ok(h.values)
}

fn hash_to_b58(digest: [u64; 5]) -> String {
    tx_types::transaction_types::Hash { values: digest }.to_b58()
}

fn recipient_lock_and_display_address(
    arena: &mut Arena,
    recipient: &RecipientInput,
    ctx: &TxIdCtx,
) -> Result<(Noun, String), JsValue> {
    match recipient {
        RecipientInput::Pkh(b58) => {
            let digest = parse_b58_hash(b58)?;
            Ok((build_pkh_lock(arena, digest)?, b58.to_string()))
        }
        RecipientInput::Multisig { m, pkhs } => {
            if *m == 0 {
                return Err(js_err("multisig requires m >= 1"));
            }
            if pkhs.is_empty() {
                return Err(js_err("multisig requires at least 1 signer"));
            }
            let mut digests: Vec<[u64; 5]> = Vec::with_capacity(pkhs.len());
            for pkh_b58 in pkhs {
                digests.push(parse_b58_hash(pkh_b58)?);
            }
            digests.sort();
            digests.dedup();
            if (*m as usize) > digests.len() {
                return Err(js_err("multisig requires m <= number of unique signers"));
            }

            let lock = build_pkh_lock_multisig(arena, *m, &digests)?;
            let lock_list = build_list(arena, &[lock]);
            let lock_root = hash_lock_primitives_list(lock_list, arena, ctx)?;
            Ok((lock, hash_to_b58(lock_root)))
        }
    }
}

#[derive(Debug, Clone)]
struct SeedSpec {
    lock_root: [u64; 5],
    note_data: Noun,
    gift: u64,
    recipient_b58: String,
}

#[derive(Debug, Clone)]
struct NoteSpec {
    name_first: [u64; 5],
    name_last: [u64; 5],
    name_noun: Noun,
    origin_page: u64,
    assets: u64,
    lock: Noun,
    note_hash: [u64; 5],
    witness_unsigned: Noun,
    capacity: u64,
}

fn build_pkh_lock(arena: &mut Arena, pkh: [u64; 5]) -> Result<Noun, JsValue> {
    build_pkh_lock_multisig(arena, 1, &[pkh])
}

fn build_pkh_lock_multisig(
    arena: &mut Arena,
    m_required: u64,
    pkhs: &[[u64; 5]],
) -> Result<Noun, JsValue> {
    let header = arena.alloc_atom_bytes(b"pkh");
    if m_required == 0 {
        return Err(js_err("pkh lock requires m >= 1"));
    }
    if pkhs.is_empty() {
        return Err(js_err("pkh lock requires at least 1 signer"));
    }
    if (m_required as usize) > pkhs.len() {
        return Err(js_err("pkh lock requires m <= number of signers"));
    }
    let m = arena.alloc_atom_u64(m_required);

    let mut set = arena.atom0();
    for digest in pkhs {
        let hash_noun = build_hash_noun(arena, *digest);
        set = canonical_zset_put(arena, set, hash_noun)
            .map_err(|e| js_err(format!("zset: {e:?}")))?;
    }

    let body = build_tuple(arena, &[m, set]);
    Ok(build_tuple(arena, &[header, body]))
}

fn build_tim_lock(arena: &mut Arena, rel_min: u64) -> Noun {
    let header = arena.alloc_atom_bytes(b"tim");

    let rel_min_noun = arena.alloc_atom_u64(rel_min);
    let min = build_tuple(arena, &[arena.atom0(), rel_min_noun]);
    let max = arena.atom0();
    let rel_range = build_tuple(arena, &[min, max]);

    let abs_range = build_tuple(arena, &[arena.atom0(), arena.atom0()]);
    let body = build_tuple(arena, &[rel_range, abs_range]);
    build_tuple(arena, &[header, body])
}

fn build_note_data_with_lock(arena: &mut Arena, lock: Noun) -> Result<(Noun, u64), JsValue> {
    let key_lock = arena.alloc_atom_bytes(b"lock");
    let lock_data = build_tuple(arena, &[arena.atom0(), lock]);
    let map = canonical_zmap_put(arena, arena.atom0(), key_lock, lock_data)
        .map_err(|e| js_err(format!("zmap: {e:?}")))?;
    Ok((map, 1))
}

fn compute_first_name_from_lock_hash(
    lock_hash: [u64; 5],
    ctx: &TxIdCtx,
) -> Result<[u64; 5], JsValue> {
    Ok(hash_ten_cell(ctx.null_digest, lock_hash).map_err(|e| js_err(format!("{e:?}")))?)
}

fn build_witness_unsigned(
    arena: &mut Arena,
    lock: Noun,
    lock_hash: [u64; 5],
    origin_page: u64,
    bythos_phase: u64,
) -> Noun {
    let axis = arena.alloc_atom_u64(1);
    let root = build_hash_noun(arena, lock_hash);
    let path = arena.atom0();
    let merkle_proof = build_tuple(arena, &[root, path]);
    let lmp = if origin_page >= bythos_phase {
        let version = arena.alloc_atom_bytes(LOCK_MERKLE_PROOF_FULL_TAG);
        build_tuple(arena, &[version, lock, axis, merkle_proof])
    } else {
        build_tuple(arena, &[lock, axis, merkle_proof])
    };

    let pkh_map = arena.atom0();
    let hax_map = arena.atom0();
    let tim = arena.atom0();
    build_tuple(arena, &[lmp, pkh_map, hax_map, tim])
}

fn placeholder_pkh_signature_value(arena: &mut Arena) -> Noun {
    let zero6 = [
        arena.alloc_atom_u64(0),
        arena.alloc_atom_u64(0),
        arena.alloc_atom_u64(0),
        arena.alloc_atom_u64(0),
        arena.alloc_atom_u64(0),
        arena.alloc_atom_u64(0),
    ];
    let x = build_tuple(arena, &zero6);
    let y = build_tuple(arena, &zero6);
    let inf_false = arena.alloc_atom_u64(1);
    let pk = build_tuple(arena, &[x, y, inf_false]);

    let zero8 = [
        arena.alloc_atom_u64(0),
        arena.alloc_atom_u64(0),
        arena.alloc_atom_u64(0),
        arena.alloc_atom_u64(0),
        arena.alloc_atom_u64(0),
        arena.alloc_atom_u64(0),
        arena.alloc_atom_u64(0),
        arena.alloc_atom_u64(0),
    ];
    let chal = build_tuple(arena, &zero8);
    let sig = build_tuple(arena, &zero8);
    let schnorr_sig = build_tuple(arena, &[chal, sig]);
    build_tuple(arena, &[pk, schnorr_sig])
}

fn build_placeholder_pkh_map(arena: &mut Arena, pkh: [u64; 5]) -> Result<Noun, JsValue> {
    let key = build_hash_noun(arena, pkh);
    let value = placeholder_pkh_signature_value(arena);
    canonical_zmap_put(arena, arena.atom0(), key, value).map_err(|e| js_err(format!("zmap: {e:?}")))
}

fn witness_with_placeholder(
    witness_unsigned: Noun,
    placeholder_pkh_map: Noun,
    arena: &mut Arena,
) -> Result<Noun, JsValue> {
    let (lmp, _pkh, hax, tim) =
        tuple4(witness_unsigned, arena).ok_or_else(|| js_err("malformed witness"))?;
    Ok(build_tuple(arena, &[lmp, placeholder_pkh_map, hax, tim]))
}

fn build_note_spec(
    arena: &mut Arena,
    note: &NoteInput,
    source_pkh: [u64; 5],
    ctx: &TxIdCtx,
    coinbase_rel_min: u64,
    bythos_phase: u64,
) -> Result<NoteSpec, JsValue> {
    let name_first = parse_b58_hash(&note.name_first)?;
    let name_last = parse_b58_hash(&note.name_last)?;
    let first_noun = build_hash_noun(arena, name_first);
    let last_noun = build_hash_noun(arena, name_last);
    let name_noun = build_tuple(arena, &[first_noun, last_noun, arena.atom0()]);

    // Attempt to reconstruct the note lock by matching its first name.
    let simple_pkh = build_pkh_lock(arena, source_pkh)?;
    let simple_lock = build_list(arena, &[simple_pkh]);
    let simple_lock_hash = hash_lock_primitives_list(simple_lock, arena, ctx)?;
    let simple_first = compute_first_name_from_lock_hash(simple_lock_hash, ctx)?;

    let lock = if simple_first == name_first {
        simple_lock
    } else {
        let coinbase_pkh = build_pkh_lock(arena, source_pkh)?;
        let coinbase_tim = build_tim_lock(arena, coinbase_rel_min);
        let coinbase_lock = build_list(arena, &[coinbase_pkh, coinbase_tim]);
        let coinbase_hash = hash_lock_primitives_list(coinbase_lock, arena, ctx)?;
        let coinbase_first = compute_first_name_from_lock_hash(coinbase_hash, ctx)?;
        if coinbase_first == name_first {
            coinbase_lock
        } else {
            return Err(js_err(
                "unsupported note lock; provide a standard pkh or coinbase note",
            ));
        }
    };

    let lock_hash = hash_lock_primitives_list(lock, arena, ctx)?;
    let (note_data, _) = build_note_data_with_lock(arena, lock)?;
    let note_data_hash = hash_note_data(note_data, arena)?;
    let name_hash = hash_nname_hashable(name_noun, arena)?;

    let version_noun = arena.alloc_atom_u64(note.version);
    let origin_noun = arena.alloc_atom_u64(note.origin_page);
    let assets_noun = arena.alloc_atom_u64(note.assets);
    let version_digest =
        hash_noun_varlen(version_noun, arena).map_err(|e| js_err(format!("{e:?}")))?;
    let origin_digest =
        hash_noun_varlen(origin_noun, arena).map_err(|e| js_err(format!("{e:?}")))?;
    let assets_digest =
        hash_noun_varlen(assets_noun, arena).map_err(|e| js_err(format!("{e:?}")))?;

    let mut note_hash = assets_digest;
    note_hash = hash_ten_cell(note_data_hash, note_hash).map_err(|e| js_err(format!("{e:?}")))?;
    note_hash = hash_ten_cell(name_hash, note_hash).map_err(|e| js_err(format!("{e:?}")))?;
    note_hash = hash_ten_cell(origin_digest, note_hash).map_err(|e| js_err(format!("{e:?}")))?;
    note_hash = hash_ten_cell(version_digest, note_hash).map_err(|e| js_err(format!("{e:?}")))?;

    let witness_unsigned =
        build_witness_unsigned(arena, lock, lock_hash, note.origin_page, bythos_phase);
    Ok(NoteSpec {
        name_first,
        name_last,
        name_noun,
        origin_page: note.origin_page,
        assets: note.assets,
        lock,
        note_hash,
        witness_unsigned,
        capacity: note.assets,
    })
}

fn build_output_seed_spec(
    arena: &mut Arena,
    recipient: &RecipientInput,
    gift: u64,
    ctx: &TxIdCtx,
) -> Result<SeedSpec, JsValue> {
    let (pkh, recipient_b58) = recipient_lock_and_display_address(arena, recipient, ctx)?;
    let lock = build_list(arena, &[pkh]);
    let lock_hash = hash_lock_primitives_list(lock, arena, ctx)?;
    let (note_data, _) = build_note_data_with_lock(arena, lock)?;
    Ok(SeedSpec {
        lock_root: lock_hash,
        note_data,
        gift,
        recipient_b58,
    })
}

#[wasm_bindgen]
pub fn compose_tx_v1_recipient_address(recipient: JsValue) -> Result<String, JsValue> {
    let recipient: RecipientInput = serde_wasm_bindgen::from_value(recipient)
        .map_err(|e| js_err(format!("bad recipient: {e}")))?;

    match recipient {
        RecipientInput::Pkh(b58) => Ok(b58),
        recipient @ RecipientInput::Multisig { .. } => {
            let mut arena = Arena::new();
            let null_digest =
                hash_noun_varlen(arena.atom0(), &arena).map_err(|e| js_err(format!("{e:?}")))?;
            let fake_atom = arena.alloc_atom_bytes(b"fake");
            let fake_digest =
                hash_noun_varlen(fake_atom, &arena).map_err(|e| js_err(format!("{e:?}")))?;
            let version_atom = arena.alloc_atom_u64(1);
            let version_digest =
                hash_noun_varlen(version_atom, &arena).map_err(|e| js_err(format!("{e:?}")))?;
            let ctx = TxIdCtx {
                null_digest,
                fake_digest,
                version_digest,
            };

            let (_lock, addr) = recipient_lock_and_display_address(&mut arena, &recipient, &ctx)?;
            Ok(addr)
        }
    }
}

fn split_seeds_to_capacity(
    mut seeds: Vec<SeedSpec>,
    capacity: u64,
) -> Result<Vec<SeedSpec>, JsValue> {
    let mut processed = Vec::new();

    for seed in seeds.drain(..) {
        if seed.gift <= capacity {
            processed.push(seed);
            continue;
        }

        let mut remaining = seed.gift;
        while remaining > 0 {
            let chunk_amount = remaining.min(capacity);
            if chunk_amount == 0 {
                return Err(js_err("cannot split seed to zero-capacity note"));
            }

            processed.push(SeedSpec {
                gift: chunk_amount,
                ..seed.clone()
            });
            remaining = remaining.saturating_sub(chunk_amount);
        }
    }

    Ok(processed)
}

fn consolidate_seeds(seeds: Vec<SeedSpec>, arena: &Arena) -> Vec<SeedSpec> {
    let mut out: Vec<SeedSpec> = Vec::new();
    'outer: for seed in seeds {
        for existing in out.iter_mut() {
            if existing.lock_root == seed.lock_root
                && nouns_equal(existing.note_data, seed.note_data, arena)
            {
                existing.gift = existing.gift.saturating_add(seed.gift);
                continue 'outer;
            }
        }
        out.push(seed);
    }
    out
}

fn build_seed_noun(arena: &mut Arena, seed: &SeedSpec, parent_hash: [u64; 5]) -> Noun {
    let output_source = arena.atom0();
    let lock_root = build_hash_noun(arena, seed.lock_root);
    let gift = arena.alloc_atom_u64(seed.gift);
    let parent_hash = build_hash_noun(arena, parent_hash);
    build_tuple(
        arena,
        &[output_source, lock_root, seed.note_data, gift, parent_hash],
    )
}

#[derive(Debug)]
struct SpendPlan {
    note_index: usize,
    accepted: Vec<SeedSpec>,
    refund_gift: u64,
    fee: u64,
}

#[derive(Debug)]
struct BuiltSpend {
    spend: Noun,
    witness: Noun,
}

fn build_spend_from_plan(
    arena: &mut Arena,
    note: &NoteSpec,
    plan: &SpendPlan,
    witness: Noun,
    refund_lock_root: [u64; 5],
    refund_note_data: Noun,
    refund_recipient_b58: &str,
) -> Result<BuiltSpend, JsValue> {
    let mut accepted = plan.accepted.clone();

    if plan.refund_gift > 0 {
        let refund_seed = SeedSpec {
            lock_root: refund_lock_root,
            note_data: refund_note_data,
            gift: plan.refund_gift,
            recipient_b58: refund_recipient_b58.to_string(),
        };
        accepted.push(refund_seed);
    }

    if accepted.is_empty() {
        return Err(js_err("spend would have no output seeds"));
    }

    let mut tally = note
        .assets
        .checked_sub(plan.fee)
        .ok_or_else(|| js_err("fee tally underflow"))?;
    for seed in &accepted {
        tally = tally
            .checked_sub(seed.gift)
            .ok_or_else(|| js_err("input/output mismatch: gifts exceed assets"))?;
    }
    if tally != 0 {
        return Err(js_err("input/output mismatch: does not tally"));
    }

    let consolidated = consolidate_seeds(accepted, arena);

    let mut seed_set = arena.atom0();
    for seed in consolidated.iter() {
        let seed_noun = build_seed_noun(arena, seed, note.note_hash);
        seed_set = canonical_zset_put(arena, seed_set, seed_noun)
            .map_err(|e| js_err(format!("zset: {e:?}")))?;
    }

    let fee_noun = arena.alloc_atom_u64(plan.fee);
    let spend_body = build_tuple(arena, &[witness, seed_set, fee_noun]);
    let spend_version = arena.alloc_atom_u64(1);
    let spend = build_tuple(arena, &[spend_version, spend_body]);

    Ok(BuiltSpend { spend, witness })
}

#[wasm_bindgen]
pub fn compose_tx_v1_unsigned(input: JsValue) -> Result<ComposedTransactionV1, JsValue> {
    let input: ComposeTxV1Input =
        serde_wasm_bindgen::from_value(input).map_err(|e| js_err(format!("bad input: {e}")))?;
    compose_tx_v1_unsigned_inner(input)
}

fn compose_tx_v1_unsigned_inner(input: ComposeTxV1Input) -> Result<ComposedTransactionV1, JsValue> {
    if input.outputs.is_empty() {
        return Err(js_err("at least one output is required"));
    }
    if input.notes.is_empty() {
        return Err(js_err("at least one input note is required"));
    }

    let coinbase_rel_min = input.coinbase_rel_min.unwrap_or(DEFAULT_COINBASE_REL_MIN);
    let fee_policy = FeePolicy::from_input(&input);
    let current_height = input.current_height.unwrap_or(fee_policy.bythos_phase);
    let source_pkh = parse_b58_hash(&input.source_pkh)?;

    let mut arena = Arena::new();
    let null_digest =
        hash_noun_varlen(arena.atom0(), &arena).map_err(|e| js_err(format!("{e:?}")))?;
    let fake_atom = arena.alloc_atom_bytes(b"fake");
    let fake_digest = hash_noun_varlen(fake_atom, &arena).map_err(|e| js_err(format!("{e:?}")))?;
    let version_atom = arena.alloc_atom_u64(1);
    let version_digest =
        hash_noun_varlen(version_atom, &arena).map_err(|e| js_err(format!("{e:?}")))?;
    let ctx = TxIdCtx {
        null_digest,
        fake_digest,
        version_digest,
    };

    let refund_pkh = build_pkh_lock(&mut arena, source_pkh)?;
    let refund_lock = build_list(&mut arena, &[refund_pkh]);
    let refund_lock_root = hash_lock_primitives_list(refund_lock, &arena, &ctx)?;
    let (refund_note_data, _) = build_note_data_with_lock(&mut arena, refund_lock)?;

    // Parse + normalize notes.
    let mut notes: Vec<NoteSpec> = input
        .notes
        .iter()
        .map(|note| {
            build_note_spec(
                &mut arena,
                note,
                source_pkh,
                &ctx,
                coinbase_rel_min,
                fee_policy.bythos_phase,
            )
        })
        .collect::<Result<_, _>>()?;

    // Sort notes: largest first, then smallest origin_page.
    notes.sort_by(|a, b| {
        b.assets
            .cmp(&a.assets)
            .then_with(|| a.origin_page.cmp(&b.origin_page))
    });

    // Build output seeds (largest first).
    let mut seeds: Vec<SeedSpec> = Vec::new();
    for output in &input.outputs {
        if output.amount == 0 {
            return Err(js_err("output amount must be > 0"));
        }
        let seed = build_output_seed_spec(&mut arena, &output.recipient, output.amount, &ctx)?;
        seeds.push(seed);
    }
    seeds.sort_by(|a, b| b.gift.cmp(&a.gift));

    let max_capacity = notes.iter().map(|n| n.capacity).max().unwrap_or(0);
    let mut processed: Vec<SeedSpec> = Vec::new();
    for seed in seeds {
        if seed.gift <= max_capacity {
            processed.push(seed);
        } else {
            // Split large seed across multiple notes.
            processed.extend(split_seeds_to_capacity(vec![seed], max_capacity)?);
        }
    }
    processed.sort_by(|a, b| b.gift.cmp(&a.gift));

    let mut remaining_seeds = processed;
    let mut plans = Vec::<SpendPlan>::new();

    for (note_index, note) in notes.iter().enumerate() {
        if remaining_seeds.is_empty() {
            break;
        }
        remaining_seeds = split_seeds_to_capacity(remaining_seeds, note.capacity)?;
        remaining_seeds.sort_by(|a, b| b.gift.cmp(&a.gift));

        let mut accepted = Vec::new();
        let mut leftover = Vec::new();
        let mut remaining_assets = note.assets;
        for seed in remaining_seeds {
            if remaining_assets >= seed.gift {
                remaining_assets -= seed.gift;
                accepted.push(seed);
            } else {
                leftover.push(seed);
            }
        }
        remaining_seeds = leftover;
        plans.push(SpendPlan {
            note_index,
            accepted,
            refund_gift: remaining_assets,
            fee: 0,
        });
    }

    if !remaining_seeds.is_empty() {
        return Err(js_err("insufficient funds to cover requested outputs"));
    }

    let final_fee_breakdown = loop {
        let mut fee_spends_map = arena.atom0();
        for plan in &plans {
            let note = &notes[plan.note_index];
            let placeholder_map = build_placeholder_pkh_map(&mut arena, source_pkh)?;
            let fee_witness =
                witness_with_placeholder(note.witness_unsigned, placeholder_map, &mut arena)?;
            let built = build_spend_from_plan(
                &mut arena,
                note,
                plan,
                fee_witness,
                refund_lock_root,
                refund_note_data,
                &input.source_pkh,
            )?;
            fee_spends_map =
                canonical_zmap_put(&mut arena, fee_spends_map, note.name_noun, built.spend)
                    .map_err(|e| js_err(format!("zmap: {e:?}")))?;
        }

        let fee_breakdown =
            calculate_transaction_fee(&mut arena, fee_spends_map, current_height, fee_policy)?;
        let fee_capacity = plans
            .iter()
            .map(|plan| {
                if plan.accepted.is_empty() {
                    plan.refund_gift.saturating_sub(1)
                } else {
                    plan.refund_gift
                }
            })
            .sum::<u64>();

        if fee_capacity >= fee_breakdown.minimum_fee {
            let mut remaining_fee = fee_breakdown.minimum_fee;
            for plan in plans.iter_mut().rev() {
                if remaining_fee == 0 {
                    plan.fee = 0;
                    continue;
                }
                let available = if plan.accepted.is_empty() {
                    plan.refund_gift.saturating_sub(1)
                } else {
                    plan.refund_gift
                };
                let take = available.min(remaining_fee);
                plan.refund_gift -= take;
                plan.fee = take;
                remaining_fee -= take;
            }
            debug_assert_eq!(remaining_fee, 0);
            break fee_breakdown;
        }

        let next_note_index = plans.len();
        let Some(note) = notes.get(next_note_index) else {
            return Err(js_err("insufficient funds to cover transaction fee"));
        };
        plans.push(SpendPlan {
            note_index: next_note_index,
            accepted: Vec::new(),
            refund_gift: note.assets,
            fee: 0,
        });
    };

    let mut spends_map = arena.atom0();
    let mut witness_data_map = arena.atom0();
    let mut inputs_display_map = arena.atom0();
    let mut summary_inputs = Vec::new();
    let mut summary_spends = Vec::new();
    let mut total_fees = 0u64;

    for plan in &plans {
        let note = &notes[plan.note_index];
        let built = build_spend_from_plan(
            &mut arena,
            note,
            plan,
            note.witness_unsigned,
            refund_lock_root,
            refund_note_data,
            &input.source_pkh,
        )?;

        spends_map = canonical_zmap_put(&mut arena, spends_map, note.name_noun, built.spend)
            .map_err(|e| js_err(format!("zmap: {e:?}")))?;

        witness_data_map =
            canonical_zmap_put(&mut arena, witness_data_map, note.name_noun, built.witness)
                .map_err(|e| js_err(format!("zmap: {e:?}")))?;
        inputs_display_map =
            canonical_zmap_put(&mut arena, inputs_display_map, note.name_noun, note.lock)
                .map_err(|e| js_err(format!("zmap: {e:?}")))?;

        total_fees = total_fees.saturating_add(plan.fee);

        summary_inputs.push(serde_json::json!({
            "name_first": hash_to_b58(note.name_first),
            "name_last": hash_to_b58(note.name_last),
            "assets": note.assets,
            "capacity": note.capacity,
        }));

        summary_spends.push(serde_json::json!({
            "input": format!("{} {}", hash_to_b58(note.name_first), hash_to_b58(note.name_last)),
            "fee": plan.fee,
            "refund": plan.refund_gift,
            "seeds": plan.accepted.iter().map(|s| serde_json::json!({
                "recipient": s.recipient_b58,
                "gift": s.gift,
            })).collect::<Vec<_>>(),
        }));
    }

    let tx_id = compute_tx_id_v1(spends_map, &arena, &ctx)?;
    let tx_id_b58 = hash_to_b58(tx_id);
    let id_noun = build_hash_noun(&mut arena, tx_id);
    let raw_tx = build_tuple(&mut arena, &[version_atom, id_noun, spends_map]);
    let raw_jam = jam(raw_tx, &arena);

    let wallet_name = arena.alloc_atom_bytes(tx_id_b58.as_bytes());
    let v1_tag = arena.alloc_atom_u64(1);
    let empty = arena.atom0();
    let input_display = build_tuple(&mut arena, &[v1_tag, inputs_display_map]);
    let display = build_tuple(&mut arena, &[input_display, empty]);
    let witness_data = build_tuple(&mut arena, &[v1_tag, witness_data_map]);
    let wallet_tx = build_tuple(
        &mut arena,
        &[v1_tag, wallet_name, spends_map, display, witness_data],
    );
    let wallet_jam = jam(wallet_tx, &arena);

    let summary = serde_json::json!({
        "tx_id": tx_id_b58,
        "source_pkh": input.source_pkh,
        "outputs": input.outputs,
        "inputs_used": summary_inputs,
        "spends": summary_spends,
        "total_fees": total_fees,
        "minimum_fee": final_fee_breakdown.minimum_fee,
        "seed_words": final_fee_breakdown.seed_words,
        "witness_words": final_fee_breakdown.witness_words,
        "coinbase_rel_min": coinbase_rel_min,
        "current_height": current_height,
        "bythos_phase": fee_policy.bythos_phase,
        "base_fee": fee_policy.base_fee,
        "input_fee_divisor": fee_policy.input_fee_divisor,
        "min_fee": fee_policy.min_fee,
    });

    Ok(ComposedTransactionV1 {
        tx_id: tx_id_b58,
        raw_jam,
        wallet_jam,
        summary_json: summary.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_zmap_value(map: Noun, arena: &Arena) -> Noun {
        let (node, _left, _right) = decompose_map(map, arena).expect("zmap");
        let (_key, value) = decompose_pair(node, arena).expect("zmap node");
        value
    }

    fn count_zmap_nodes(map: Noun, arena: &Arena) -> u64 {
        if map == arena.atom0() {
            return 0;
        }
        let Some((_, left, right)) = decompose_map(map, arena) else {
            return 0;
        };
        1 + count_zmap_nodes(left, arena) + count_zmap_nodes(right, arena)
    }

    #[test]
    fn wallet_tx_v1_populates_witness_data_and_inputs_display() {
        let source_pkh_b58 = "9yPePjfWAdUnzaQKyxcRXKRa5PpUzKKEwtpECBZsUYt9Jd7egSDEWoV";

        let source_pkh = parse_b58_hash(source_pkh_b58).expect("source pkh");

        let mut arena = Arena::new();
        let null_digest = hash_noun_varlen(arena.atom0(), &arena).expect("null digest");
        let fake_atom = arena.alloc_atom_bytes(b"fake");
        let fake_digest = hash_noun_varlen(fake_atom, &arena).expect("fake digest");
        let version_atom = arena.alloc_atom_u64(1);
        let version_digest = hash_noun_varlen(version_atom, &arena).expect("version digest");
        let ctx = TxIdCtx {
            null_digest,
            fake_digest,
            version_digest,
        };

        let simple_pkh = build_pkh_lock(&mut arena, source_pkh).expect("lock primitive");
        let simple_lock = build_list(&mut arena, &[simple_pkh]);
        let simple_lock_hash =
            hash_lock_primitives_list(simple_lock, &arena, &ctx).expect("lock hash");
        let note_first =
            compute_first_name_from_lock_hash(simple_lock_hash, &ctx).expect("first name");

        let composed = compose_tx_v1_unsigned_inner(ComposeTxV1Input {
            source_pkh: source_pkh_b58.to_string(),
            notes: vec![NoteInput {
                name_first: hash_to_b58(note_first),
                name_last: source_pkh_b58.to_string(),
                origin_page: 1,
                assets: 100_000_000,
                version: 1,
            }],
            outputs: vec![OutputInput {
                recipient: RecipientInput::Pkh(source_pkh_b58.to_string()),
                amount: 65_536,
                alias: None,
            }],
            coinbase_rel_min: None,
            current_height: Some(BYTHOS_PHASE),
            bythos_phase: None,
            base_fee: None,
            input_fee_divisor: None,
            min_fee: None,
        })
        .expect("compose");

        let mut decode_arena = Arena::new();
        let tx_noun = tx_types::pokenoun::cue(&composed.wallet_jam, &mut decode_arena)
            .expect("cue wallet tx");

        let (tag, _name, spends, display, witness_data) =
            tuple5(tx_noun, &decode_arena).expect("wallet tx tuple5");
        assert_eq!(noun_atom_u64(tag, &decode_arena), Some(1));

        let (inputs, _outputs) = tuple2(display, &decode_arena).expect("display tuple2");
        let (in_tag, in_map) = tuple2(inputs, &decode_arena).expect("input-display tuple2");
        assert_eq!(noun_atom_u64(in_tag, &decode_arena), Some(1));

        let (wd_tag, wd_map) = tuple2(witness_data, &decode_arena).expect("witness-data tuple2");
        assert_eq!(noun_atom_u64(wd_tag, &decode_arena), Some(1));

        let spend_count = count_zmap_nodes(spends, &decode_arena);
        assert!(spend_count > 0);
        assert_eq!(spend_count, count_zmap_nodes(in_map, &decode_arena));
        assert_eq!(spend_count, count_zmap_nodes(wd_map, &decode_arena));

        let witness = first_zmap_value(wd_map, &decode_arena);
        let (lmp, _pkh, _hax, _tim) = tuple4(witness, &decode_arena).expect("witness tuple4");
        let (version, _spend_condition, axis, _proof) =
            decompose_lock_merkle_proof(lmp, &decode_arena).expect("stub lmp");
        assert!(version.is_none());
        assert_eq!(noun_atom_u64(axis, &decode_arena), Some(1));
    }

    #[test]
    fn wallet_tx_v1_uses_full_lock_merkle_proof_for_bythos_notes() {
        let source_pkh_b58 = "9yPePjfWAdUnzaQKyxcRXKRa5PpUzKKEwtpECBZsUYt9Jd7egSDEWoV";
        let source_pkh = parse_b58_hash(source_pkh_b58).expect("source pkh");

        let mut arena = Arena::new();
        let null_digest = hash_noun_varlen(arena.atom0(), &arena).expect("null digest");
        let fake_atom = arena.alloc_atom_bytes(b"fake");
        let fake_digest = hash_noun_varlen(fake_atom, &arena).expect("fake digest");
        let version_atom = arena.alloc_atom_u64(1);
        let version_digest = hash_noun_varlen(version_atom, &arena).expect("version digest");
        let ctx = TxIdCtx {
            null_digest,
            fake_digest,
            version_digest,
        };

        let simple_pkh = build_pkh_lock(&mut arena, source_pkh).expect("lock primitive");
        let simple_lock = build_list(&mut arena, &[simple_pkh]);
        let simple_lock_hash =
            hash_lock_primitives_list(simple_lock, &arena, &ctx).expect("lock hash");
        let note_first =
            compute_first_name_from_lock_hash(simple_lock_hash, &ctx).expect("first name");

        let composed = compose_tx_v1_unsigned_inner(ComposeTxV1Input {
            source_pkh: source_pkh_b58.to_string(),
            notes: vec![NoteInput {
                name_first: hash_to_b58(note_first),
                name_last: source_pkh_b58.to_string(),
                origin_page: BYTHOS_PHASE,
                assets: 100_000_000,
                version: 1,
            }],
            outputs: vec![OutputInput {
                recipient: RecipientInput::Pkh(source_pkh_b58.to_string()),
                amount: 65_536,
                alias: None,
            }],
            coinbase_rel_min: None,
            current_height: Some(BYTHOS_PHASE),
            bythos_phase: None,
            base_fee: None,
            input_fee_divisor: None,
            min_fee: None,
        })
        .expect("compose");

        let mut decode_arena = Arena::new();
        let tx_noun = tx_types::pokenoun::cue(&composed.wallet_jam, &mut decode_arena)
            .expect("cue wallet tx");
        let (_tag, _name, _spends, _display, witness_data) =
            tuple5(tx_noun, &decode_arena).expect("wallet tx tuple5");
        let (_wd_tag, wd_map) = tuple2(witness_data, &decode_arena).expect("witness-data tuple2");

        let witness = first_zmap_value(wd_map, &decode_arena);
        let (lmp, _pkh, _hax, _tim) = tuple4(witness, &decode_arena).expect("witness tuple4");
        let (version, _spend_condition, axis, _proof) =
            decompose_lock_merkle_proof(lmp, &decode_arena).expect("full lmp");

        let version = version.expect("full lmp version tag");
        match version {
            Noun::Atom(atom) => {
                assert!(decode_arena.atom_eq_bytes(atom, LOCK_MERKLE_PROOF_FULL_TAG))
            }
            _ => panic!("expected full lmp version atom"),
        }
        assert_eq!(noun_atom_u64(axis, &decode_arena), Some(1));
    }

    #[test]
    fn wallet_tx_v1_allocates_exact_minimum_fee() {
        let source_pkh_b58 = "9yPePjfWAdUnzaQKyxcRXKRa5PpUzKKEwtpECBZsUYt9Jd7egSDEWoV";
        let source_pkh = parse_b58_hash(source_pkh_b58).expect("source pkh");

        let mut arena = Arena::new();
        let null_digest = hash_noun_varlen(arena.atom0(), &arena).expect("null digest");
        let fake_atom = arena.alloc_atom_bytes(b"fake");
        let fake_digest = hash_noun_varlen(fake_atom, &arena).expect("fake digest");
        let version_atom = arena.alloc_atom_u64(1);
        let version_digest = hash_noun_varlen(version_atom, &arena).expect("version digest");
        let ctx = TxIdCtx {
            null_digest,
            fake_digest,
            version_digest,
        };

        let simple_pkh = build_pkh_lock(&mut arena, source_pkh).expect("lock primitive");
        let simple_lock = build_list(&mut arena, &[simple_pkh]);
        let simple_lock_hash =
            hash_lock_primitives_list(simple_lock, &arena, &ctx).expect("lock hash");
        let note_first =
            compute_first_name_from_lock_hash(simple_lock_hash, &ctx).expect("first name");

        let input_assets = 100_000_000;
        let output_amount = 65_536;
        let composed = compose_tx_v1_unsigned_inner(ComposeTxV1Input {
            source_pkh: source_pkh_b58.to_string(),
            notes: vec![NoteInput {
                name_first: hash_to_b58(note_first),
                name_last: source_pkh_b58.to_string(),
                origin_page: BYTHOS_PHASE,
                assets: input_assets,
                version: 1,
            }],
            outputs: vec![OutputInput {
                recipient: RecipientInput::Pkh(source_pkh_b58.to_string()),
                amount: output_amount,
                alias: None,
            }],
            coinbase_rel_min: None,
            current_height: Some(BYTHOS_PHASE),
            bythos_phase: None,
            base_fee: None,
            input_fee_divisor: None,
            min_fee: None,
        })
        .expect("compose");

        let summary: serde_json::Value =
            serde_json::from_str(&composed.summary_json).expect("summary json");
        let total_fees = summary["total_fees"].as_u64().expect("total_fees");
        let minimum_fee = summary["minimum_fee"].as_u64().expect("minimum_fee");
        let refund = summary["spends"][0]["refund"].as_u64().expect("refund");

        assert_eq!(total_fees, minimum_fee);
        assert_eq!(refund, input_assets - output_amount - total_fees);

        let mut decode_arena = Arena::new();
        let tx_noun = tx_types::pokenoun::cue(&composed.wallet_jam, &mut decode_arena)
            .expect("cue wallet tx");
        let (_tag, _name, spends, _display, _witness_data) =
            tuple5(tx_noun, &decode_arena).expect("wallet tx tuple5");
        let spend = first_zmap_value(spends, &decode_arena);
        let (version, body) = tuple2(spend, &decode_arena).expect("spend tuple2");
        assert_eq!(noun_atom_u64(version, &decode_arena), Some(1));
        let (_witness, _seeds, fee) = tuple3(body, &decode_arena).expect("spend body");
        assert_eq!(noun_atom_u64(fee, &decode_arena), Some(total_fees));
    }
}
