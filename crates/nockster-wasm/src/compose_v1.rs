use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use tx_types::pokenoun::{
    canonical_zmap_put, canonical_zset_put, cue, hash_noun_varlen, hash_ten_cell, jam, Arena, Noun,
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
    /// When the source notes are held under an m-of-n multisig lock, this
    /// supplies the lock so inputs are reconstructed as multisig (and change
    /// returns to the multisig). The composed `.psnt` then round-trips through
    /// each co-signer's device, which fills its own signature slot.
    #[serde(default)]
    pub source_multisig: Option<MultisigSourceInput>,
    /// When the source notes are held under an OR-composed lock (e.g. an HTLC),
    /// this supplies the full branch set and which branch to spend. The witness
    /// carries a full lock-merkle-proof for that branch.
    #[serde(default)]
    pub source_or_lock: Option<OrSourceInput>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MultisigSourceInput {
    pub m: u64,
    pub pkhs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrSourceInput {
    pub branches: Vec<LockBranch>,
    pub spend_branch: u64,
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
    /// Optional timelock: appends a `%tim` primitive (recipient AND time).
    #[serde(default)]
    pub timelock: Option<TimelockSpec>,
    /// Optional hashlock: appends a `%hax` primitive committing to these
    /// preimage-hash commitments (base58); the spender must reveal preimages.
    #[serde(default)]
    pub hashlock: Option<Vec<String>>,
    /// Burn: the spend-condition is `[%brn]` (unspendable); recipient ignored.
    #[serde(default)]
    pub burn: bool,
    /// OR-composed lock: any one branch can spend (HTLC / refund-after-timeout
    /// patterns). When set, the flat recipient/timelock/hashlock/burn fields
    /// above are ignored and the lock is the `%2/%4/%8/%16` tree of branches.
    #[serde(default)]
    pub or_branches: Option<Vec<LockBranch>>,
}

/// One branch of an OR-composed lock — a single spend-condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockBranch {
    #[serde(default)]
    pub recipient: Option<RecipientInput>,
    #[serde(default)]
    pub timelock: Option<TimelockSpec>,
    #[serde(default)]
    pub hashlock: Option<Vec<String>>,
    #[serde(default)]
    pub burn: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelockSpec {
    #[serde(default)]
    pub rel_min: Option<u64>,
    #[serde(default)]
    pub rel_max: Option<u64>,
    #[serde(default)]
    pub abs_min: Option<u64>,
    #[serde(default)]
    pub abs_max: Option<u64>,
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
    // Retained for parity with the hashing context; the `%hax` stub that read
    // it was replaced by real commitment-z-set hashing.
    #[allow(dead_code)]
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
        // `%hax` body is a z-set of preimage-commitment hashes, hashed exactly
        // like `%pkh`'s hash set (matches nockchain `hashable:hax`).
        let tag_digest = hash_noun_varlen(header, arena).map_err(|e| js_err(format!("{e:?}")))?;
        let h_digest = hash_zset_hashes(body, arena, ctx)?;
        return Ok(hash_ten_cell(tag_digest, h_digest).map_err(|e| js_err(format!("{e:?}")))?);
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
    /// Signers whose slots seed the fee-estimation placeholder map: the source
    /// pkh for single-sig, or the m expected signers for a multisig input.
    signer_pkhs: Vec<[u64; 5]>,
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

/// A `(unit @)`: `~` (atom 0) for None, `[~ v]` for Some.
fn unit_opt(arena: &mut Arena, v: Option<u64>) -> Noun {
    match v {
        Some(n) => {
            let z = arena.atom0();
            let nn = arena.alloc_atom_u64(n);
            build_tuple(arena, &[z, nn])
        }
        None => arena.atom0(),
    }
}

/// `[%tim [rel=[min max] abs=[min max]]]` with each bound a `(unit page-number)`.
fn build_tim_lock_full(
    arena: &mut Arena,
    rel_min: Option<u64>,
    rel_max: Option<u64>,
    abs_min: Option<u64>,
    abs_max: Option<u64>,
) -> Noun {
    let header = arena.alloc_atom_bytes(b"tim");
    let rmin = unit_opt(arena, rel_min);
    let rmax = unit_opt(arena, rel_max);
    let rel_range = build_tuple(arena, &[rmin, rmax]);
    let amin = unit_opt(arena, abs_min);
    let amax = unit_opt(arena, abs_max);
    let abs_range = build_tuple(arena, &[amin, amax]);
    let body = build_tuple(arena, &[rel_range, abs_range]);
    build_tuple(arena, &[header, body])
}

fn build_tim_lock(arena: &mut Arena, rel_min: u64) -> Noun {
    build_tim_lock_full(arena, Some(rel_min), None, None, None)
}

/// `[%hax (z-set hash)]` — commits to preimage hashes; the spender reveals the
/// preimages in the witness hax map.
fn build_hax_lock(arena: &mut Arena, commitments: &[[u64; 5]]) -> Result<Noun, JsValue> {
    let header = arena.alloc_atom_bytes(b"hax");
    let mut set = arena.atom0();
    for c in commitments {
        let hn = build_hash_noun(arena, *c);
        set = canonical_zset_put(arena, set, hn).map_err(|e| js_err(format!("zset: {e:?}")))?;
    }
    Ok(build_tuple(arena, &[header, set]))
}

/// `[%brn ~]` — unspendable (burn).
fn build_brn_lock(arena: &mut Arena) -> Noun {
    let header = arena.alloc_atom_bytes(b"brn");
    let null = arena.atom0();
    build_tuple(arena, &[header, null])
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

/// One placeholder signature slot per signer, so fee estimation reflects the
/// signed witness size (m slots for an m-of-n input) and every co-signer's
/// device has a slot to fill.
fn build_placeholder_pkh_map_multi(arena: &mut Arena, pkhs: &[[u64; 5]]) -> Result<Noun, JsValue> {
    let mut map = arena.atom0();
    for pkh in pkhs {
        let key = build_hash_noun(arena, *pkh);
        let value = placeholder_pkh_signature_value(arena);
        map = canonical_zmap_put(arena, map, key, value)
            .map_err(|e| js_err(format!("zmap: {e:?}")))?;
    }
    Ok(map)
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
    source_multisig: Option<&(u64, Vec<[u64; 5]>)>,
    source_or_lock: Option<&OrSourceInput>,
    ctx: &TxIdCtx,
    coinbase_rel_min: u64,
    bythos_phase: u64,
) -> Result<NoteSpec, JsValue> {
    let name_first = parse_b58_hash(&note.name_first)?;
    let name_last = parse_b58_hash(&note.name_last)?;
    let first_noun = build_hash_noun(arena, name_first);
    let last_noun = build_hash_noun(arena, name_last);
    let name_noun = build_tuple(arena, &[first_noun, last_noun, arena.atom0()]);

    // Reconstruct the note lock by matching its first name. An OR-composed
    // source spends one branch (custom full-lmp witness); a multisig source is
    // the m-of-n pkh lock; otherwise try a simple pkh then a coinbase lock.
    let (lock, lock_hash, signer_pkhs, witness_override) = if let Some(or) = source_or_lock {
        let (lock, root, witness, signers) =
            build_or_input(arena, &or.branches, or.spend_branch as usize, ctx)?;
        if compute_first_name_from_lock_hash(root, ctx)? != name_first {
            return Err(js_err("note name does not match the provided OR lock / branches"));
        }
        (lock, root, signers, Some(witness))
    } else if let Some((m, pkhs)) = source_multisig {
        let ms_lock = build_pkh_lock_multisig(arena, *m, pkhs)?;
        let ms_list = build_list(arena, &[ms_lock]);
        let ms_hash = hash_lock_primitives_list(ms_list, arena, ctx)?;
        if compute_first_name_from_lock_hash(ms_hash, ctx)? != name_first {
            return Err(js_err(
                "note name does not match the provided multisig lock (m/signers)",
            ));
        }
        let signers: Vec<[u64; 5]> = pkhs.iter().take(*m as usize).copied().collect();
        (ms_list, ms_hash, signers, None)
    } else {
        let simple_pkh = build_pkh_lock(arena, source_pkh)?;
        let simple_lock = build_list(arena, &[simple_pkh]);
        let simple_lock_hash = hash_lock_primitives_list(simple_lock, arena, ctx)?;
        let simple_first = compute_first_name_from_lock_hash(simple_lock_hash, ctx)?;
        let (lock, lock_hash) = if simple_first == name_first {
            (simple_lock, simple_lock_hash)
        } else {
            let coinbase_pkh = build_pkh_lock(arena, source_pkh)?;
            let coinbase_tim = build_tim_lock(arena, coinbase_rel_min);
            let coinbase_lock = build_list(arena, &[coinbase_pkh, coinbase_tim]);
            let coinbase_hash = hash_lock_primitives_list(coinbase_lock, arena, ctx)?;
            let coinbase_first = compute_first_name_from_lock_hash(coinbase_hash, ctx)?;
            if coinbase_first == name_first {
                (coinbase_lock, coinbase_hash)
            } else {
                return Err(js_err(
                    "unsupported note lock; provide a standard pkh or coinbase note",
                ));
            }
        };
        (lock, lock_hash, vec![source_pkh], None)
    };

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

    let witness_unsigned = match witness_override {
        Some(w) => w,
        None => build_witness_unsigned(arena, lock, lock_hash, note.origin_page, bythos_phase),
    };
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
        signer_pkhs,
    })
}

/// Build one spend-condition (AND-list of primitives) and a display string.
/// `[%brn]` if `burn`, else `[pkh, (tim)?, (hax)?]`.
fn build_spend_condition(
    arena: &mut Arena,
    recipient: Option<&RecipientInput>,
    timelock: Option<&TimelockSpec>,
    hashlock: Option<&[String]>,
    burn: bool,
    ctx: &TxIdCtx,
) -> Result<(Noun, Option<String>), JsValue> {
    if burn {
        let brn = build_brn_lock(arena);
        return Ok((build_list(arena, &[brn]), None));
    }
    let recipient = recipient.ok_or_else(|| js_err("spend-condition needs a recipient or burn"))?;
    let (pkh, recipient_b58) = recipient_lock_and_display_address(arena, recipient, ctx)?;
    let mut prims = vec![pkh];
    if let Some(tl) = timelock {
        if tl.rel_min.is_none()
            && tl.rel_max.is_none()
            && tl.abs_min.is_none()
            && tl.abs_max.is_none()
        {
            return Err(js_err("timelock requires at least one bound"));
        }
        prims.push(build_tim_lock_full(
            arena, tl.rel_min, tl.rel_max, tl.abs_min, tl.abs_max,
        ));
    }
    if let Some(hashes) = hashlock {
        if hashes.is_empty() {
            return Err(js_err("hashlock requires at least one commitment"));
        }
        let mut digests: Vec<[u64; 5]> = Vec::with_capacity(hashes.len());
        for h in hashes {
            digests.push(parse_b58_hash(h)?);
        }
        digests.sort();
        digests.dedup();
        prims.push(build_hax_lock(arena, &digests)?);
    }
    Ok((build_list(arena, &prims), Some(recipient_b58)))
}

fn build_output_seed_spec(
    arena: &mut Arena,
    output: &OutputInput,
    ctx: &TxIdCtx,
) -> Result<SeedSpec, JsValue> {
    // OR-composed lock (any branch can spend), else a single spend-condition.
    let (lock, lock_hash, recipient_b58) = if let Some(branches) = &output.or_branches {
        let (lock, root) = build_or_lock(arena, branches, ctx)?;
        (lock, root, hash_to_b58(root))
    } else {
        let (sc, display) = build_spend_condition(
            arena,
            Some(&output.recipient),
            output.timelock.as_ref(),
            output.hashlock.as_deref(),
            output.burn,
            ctx,
        )?;
        let hash = hash_lock_primitives_list(sc, arena, ctx)?;
        let display = display.unwrap_or_else(|| hash_to_b58(hash));
        (sc, hash, display)
    };
    let (note_data, _) = build_note_data_with_lock(arena, lock)?;
    Ok(SeedSpec {
        lock_root: lock_hash,
        note_data,
        gift: output.amount,
        recipient_b58,
    })
}

/// Build an OR-composed lock from branch spend-conditions, mirroring
/// nockchain `++lock`: pad to the nearest power of two (≤16) with `[%brn]`
/// fillers, build the balanced `%2/%4/%8/%16` tree, and hash it
/// (`hash-leaf(size) :: balanced-hash-pair(leaf-hashes)`).
///
/// Validated against nockchain's golden `%2`/`%4` lock-root vectors
/// (`or_composed_lock_roots_match_nockchain_vectors`).
fn build_or_lock(
    arena: &mut Arena,
    branches: &[LockBranch],
    ctx: &TxIdCtx,
) -> Result<(Noun, [u64; 5]), JsValue> {
    if branches.is_empty() {
        return Err(js_err("OR lock requires at least one branch"));
    }
    let mut scs: Vec<Noun> = Vec::with_capacity(branches.len());
    for b in branches {
        let (sc, _display) = build_spend_condition(
            arena,
            b.recipient.as_ref(),
            b.timelock.as_ref(),
            b.hashlock.as_deref(),
            b.burn,
            ctx,
        )?;
        scs.push(sc);
    }
    // nearest power of two >= len, capped at 16.
    let len = scs.len();
    let mut size = 1usize;
    while size < len {
        size <<= 1;
    }
    if size > 16 {
        return Err(js_err("OR lock supports at most 16 branches"));
    }
    // Pad with `[%brn]` filler spend-conditions.
    while scs.len() < size {
        let brn = build_brn_lock(arena);
        scs.push(build_list(arena, &[brn]));
    }
    let leaf_hashes: Vec<[u64; 5]> = scs
        .iter()
        .map(|sc| hash_lock_primitives_list(*sc, arena, ctx))
        .collect::<Result<_, _>>()?;
    let lock = build_lock_tree_noun(arena, &scs, size);
    let root = hash_lock_tree_root(arena, &leaf_hashes, size)?;
    Ok((lock, root))
}

/// The pkh signer hashes a branch needs (for fee/placeholder sizing).
fn branch_signer_pkhs(branch: &LockBranch) -> Result<Vec<[u64; 5]>, JsValue> {
    if branch.burn {
        return Ok(Vec::new());
    }
    match &branch.recipient {
        Some(RecipientInput::Pkh(b58)) => Ok(vec![parse_b58_hash(b58)?]),
        Some(RecipientInput::Multisig { m, pkhs }) => {
            let mut digests: Vec<[u64; 5]> = Vec::new();
            for p in pkhs {
                digests.push(parse_b58_hash(p)?);
            }
            digests.sort();
            digests.dedup();
            Ok(digests.into_iter().take(*m as usize).collect())
        }
        None => Ok(Vec::new()),
    }
}

/// Reconstruct an OR-composed input lock and build the unsigned witness that
/// spends it via the chosen branch — a full lock-merkle-proof
/// (`[%full spend-condition axis [root path]]`) plus placeholder signature
/// slots for that branch's signers. Returns (lock, lock_root, witness, signers).
fn build_or_input(
    arena: &mut Arena,
    branches: &[LockBranch],
    spend_branch: usize,
    ctx: &TxIdCtx,
) -> Result<(Noun, [u64; 5], Noun, Vec<[u64; 5]>), JsValue> {
    if branches.is_empty() {
        return Err(js_err("OR input requires at least one branch"));
    }
    if spend_branch >= branches.len() {
        return Err(js_err("spend_branch out of range"));
    }
    let mut scs: Vec<Noun> = Vec::new();
    let mut signers: Vec<Vec<[u64; 5]>> = Vec::new();
    for b in branches {
        let (sc, _disp) = build_spend_condition(
            arena,
            b.recipient.as_ref(),
            b.timelock.as_ref(),
            b.hashlock.as_deref(),
            b.burn,
            ctx,
        )?;
        scs.push(sc);
        signers.push(branch_signer_pkhs(b)?);
    }
    let mut size = 1usize;
    while size < scs.len() {
        size <<= 1;
    }
    if size > 16 {
        return Err(js_err("OR input supports at most 16 branches"));
    }
    while scs.len() < size {
        let brn = build_brn_lock(arena);
        scs.push(build_list(arena, &[brn]));
        signers.push(Vec::new());
    }
    let leaf_hashes: Vec<[u64; 5]> = scs
        .iter()
        .map(|sc| hash_lock_primitives_list(*sc, arena, ctx))
        .collect::<Result<_, _>>()?;
    let lock = build_lock_tree_noun(arena, &scs, size);
    let root = hash_lock_tree_root(arena, &leaf_hashes, size)?;

    // Prove the chosen leaf. leaf_number is 1-indexed (spend_branch + 1) and
    // the hashable index adds 1 more because the tag occupies hashable leaf 1.
    let tree = or_hashable_tree(arena, &leaf_hashes, size);
    let (proof_root, path, axis) = htree_prove(&tree, spend_branch as u64 + 2)?;
    if proof_root != root {
        return Err(js_err("internal: OR proof root mismatch"));
    }

    // Full lock-merkle-proof witness for the chosen branch.
    let version = arena.alloc_atom_bytes(LOCK_MERKLE_PROOF_FULL_TAG);
    let chosen_sc = scs[spend_branch];
    let axis_noun = arena.alloc_atom_u64(axis);
    let root_noun = build_hash_noun(arena, root);
    let path_nouns: Vec<Noun> = path.iter().map(|h| build_hash_noun(arena, *h)).collect();
    let path_noun = if path_nouns.is_empty() {
        arena.atom0()
    } else {
        build_list(arena, &path_nouns)
    };
    let merkle_proof = build_tuple(arena, &[root_noun, path_noun]);
    let lmp = build_tuple(arena, &[version, chosen_sc, axis_noun, merkle_proof]);
    let pkh_map = build_placeholder_pkh_map_multi(arena, &signers[spend_branch])?;
    let hax = arena.atom0();
    let tim = arena.atom0();
    let witness = build_tuple(arena, &[lmp, pkh_map, hax, tim]);
    Ok((lock, root, witness, signers[spend_branch].clone()))
}

/// Tree noun per nockchain `++build`: leaf is the spend-condition; `%2` keeps
/// both children whole; higher tags re-tag the stripped child payloads.
fn build_lock_tree_noun(arena: &mut Arena, leaves: &[Noun], size: usize) -> Noun {
    if size == 1 {
        return leaves[0];
    }
    let half = size / 2;
    let left = build_lock_tree_noun(arena, &leaves[..half], half);
    let right = build_lock_tree_noun(arena, &leaves[half..], half);
    let tag = arena.alloc_atom_u64(size as u64);
    if size == 2 {
        build_tuple(arena, &[tag, left, right])
    } else {
        // `[%size +.left +.right]`: strip the child tags (take their tails).
        let lp = match left {
            Noun::Cell(id) => arena.cell(id).tail,
            _ => left,
        };
        let rp = match right {
            Noun::Cell(id) => arena.cell(id).tail,
            _ => right,
        };
        build_tuple(arena, &[tag, lp, rp])
    }
}

/// Lock-root per nockchain `Lock::hash`: a single leaf hashes to its
/// spend-condition hash; a tree hashes to `hash-pair(leaf(size),
/// balanced-hash-pair(leaf-hashes))`.
fn hash_lock_tree_root(
    arena: &mut Arena,
    leaf_hashes: &[[u64; 5]],
    size: usize,
) -> Result<[u64; 5], JsValue> {
    if size == 1 {
        return Ok(leaf_hashes[0]);
    }
    let tag_atom = arena.alloc_atom_u64(size as u64);
    let tag_digest = hash_noun_varlen(tag_atom, arena).map_err(|e| js_err(format!("{e:?}")))?;
    let payload = hash_balanced_pairs(leaf_hashes)?;
    Ok(hash_ten_cell(tag_digest, payload).map_err(|e| js_err(format!("{e:?}")))?)
}

fn hash_balanced_pairs(hashes: &[[u64; 5]]) -> Result<[u64; 5], JsValue> {
    if hashes.len() == 1 {
        return Ok(hashes[0]);
    }
    let half = hashes.len() / 2;
    let left = hash_balanced_pairs(&hashes[..half])?;
    let right = hash_balanced_pairs(&hashes[half..])?;
    Ok(hash_ten_cell(left, right).map_err(|e| js_err(format!("{e:?}")))?)
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

#[derive(Debug, Clone)]
struct SpendPlan {
    note_index: usize,
    accepted: Vec<SeedSpec>,
    refund_gift: u64,
    fee: u64,
}

#[derive(Debug)]
struct CandidatePlan {
    plans: Vec<SpendPlan>,
    fee_breakdown: FeeBreakdown,
    selected_assets: u64,
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

fn build_initial_plans_for_order(
    notes: &[NoteSpec],
    note_order: &[usize],
    seeds: &[SeedSpec],
) -> Result<Option<Vec<SpendPlan>>, JsValue> {
    if note_order.is_empty() {
        return Ok(None);
    }

    let max_capacity = note_order
        .iter()
        .filter_map(|idx| notes.get(*idx).map(|note| note.capacity))
        .max()
        .unwrap_or(0);
    if max_capacity == 0 {
        return Ok(None);
    }

    let mut remaining_seeds = split_seeds_to_capacity(seeds.to_vec(), max_capacity)?;
    remaining_seeds.sort_by(|a, b| b.gift.cmp(&a.gift));
    let mut plans = Vec::<SpendPlan>::new();

    for note_index in note_order {
        if remaining_seeds.is_empty() {
            break;
        }
        let Some(note) = notes.get(*note_index) else {
            return Ok(None);
        };
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
            note_index: *note_index,
            accepted,
            refund_gift: remaining_assets,
            fee: 0,
        });
    }

    if remaining_seeds.is_empty() {
        Ok(Some(plans))
    } else {
        Ok(None)
    }
}

fn settle_candidate_fees(
    arena: &mut Arena,
    notes: &[NoteSpec],
    mut plans: Vec<SpendPlan>,
    // Retained for signature symmetry; per-note signers now size the fee
    // placeholder map (see build_candidate_plan), so this is unused.
    _source_pkh: [u64; 5],
    refund_lock_root: [u64; 5],
    refund_note_data: Noun,
    refund_recipient_b58: &str,
    current_height: u64,
    fee_policy: FeePolicy,
) -> Result<Option<CandidatePlan>, JsValue> {
    if plans.is_empty() {
        return Ok(None);
    }

    let mut fee_spends_map = arena.atom0();
    for plan in &plans {
        let note = &notes[plan.note_index];
        let placeholder_map = build_placeholder_pkh_map_multi(arena, &note.signer_pkhs)?;
        let fee_witness = witness_with_placeholder(note.witness_unsigned, placeholder_map, arena)?;
        let built = build_spend_from_plan(
            arena,
            note,
            plan,
            fee_witness,
            refund_lock_root,
            refund_note_data,
            refund_recipient_b58,
        )?;
        fee_spends_map = canonical_zmap_put(arena, fee_spends_map, note.name_noun, built.spend)
            .map_err(|e| js_err(format!("zmap: {e:?}")))?;
    }

    let fee_breakdown =
        calculate_transaction_fee(arena, fee_spends_map, current_height, fee_policy)?;
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
    if fee_capacity < fee_breakdown.minimum_fee {
        return Ok(None);
    }

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

    let selected_assets = plans
        .iter()
        .map(|plan| notes[plan.note_index].assets)
        .sum::<u64>();
    Ok(Some(CandidatePlan {
        plans,
        fee_breakdown,
        selected_assets,
    }))
}

fn candidate_better(candidate: &CandidatePlan, best: Option<&CandidatePlan>) -> bool {
    let Some(best) = best else {
        return true;
    };
    candidate
        .fee_breakdown
        .minimum_fee
        .cmp(&best.fee_breakdown.minimum_fee)
        .then_with(|| candidate.plans.len().cmp(&best.plans.len()))
        .then_with(|| candidate.selected_assets.cmp(&best.selected_assets))
        == core::cmp::Ordering::Less
}

fn push_candidate_order(orders: &mut Vec<Vec<usize>>, order: Vec<usize>) {
    if order.is_empty() {
        return;
    }
    if orders.iter().any(|existing| existing == &order) {
        return;
    }
    orders.push(order);
}

fn collect_combinations_for_count(
    sorted_indices: &[usize],
    notes: &[NoteSpec],
    target: u64,
    count: usize,
    start: usize,
    current: &mut Vec<usize>,
    current_sum: u64,
    out: &mut Vec<Vec<usize>>,
    limit: usize,
) {
    if out.len() >= limit {
        return;
    }
    if current.len() == count {
        if current_sum >= target {
            out.push(current.clone());
        }
        return;
    }

    let remaining_needed = count.saturating_sub(current.len());
    if sorted_indices.len().saturating_sub(start) < remaining_needed {
        return;
    }

    for pos in start..sorted_indices.len() {
        if out.len() >= limit {
            return;
        }
        let idx = sorted_indices[pos];
        current.push(idx);
        collect_combinations_for_count(
            sorted_indices,
            notes,
            target,
            count,
            pos + 1,
            current,
            current_sum.saturating_add(notes[idx].assets),
            out,
            limit,
        );
        current.pop();
    }
}

fn candidate_note_orders(
    notes: &[NoteSpec],
    target: u64,
    fee_policy: FeePolicy,
) -> Vec<Vec<usize>> {
    const MAX_COMBINATION_COUNT: usize = 6;
    const MAX_COMBINATIONS_PER_COUNT: usize = 160;

    let mut asc: Vec<usize> = (0..notes.len()).collect();
    asc.sort_by(|a, b| {
        notes[*a]
            .assets
            .cmp(&notes[*b].assets)
            .then_with(|| notes[*a].origin_page.cmp(&notes[*b].origin_page))
    });
    let mut desc = asc.clone();
    desc.reverse();

    let mut orders = Vec::<Vec<usize>>::new();
    for idx in &asc {
        push_candidate_order(&mut orders, vec![*idx]);
    }

    let mut sum = 0u64;
    let mut order = Vec::new();
    for idx in &asc {
        order.push(*idx);
        sum = sum.saturating_add(notes[*idx].assets);
        if sum >= target {
            push_candidate_order(&mut orders, order.clone());
        }
    }

    sum = 0;
    order.clear();
    for idx in &desc {
        order.push(*idx);
        sum = sum.saturating_add(notes[*idx].assets);
        if sum >= target {
            push_candidate_order(&mut orders, order.clone());
            break;
        }
    }

    let combination_target = target.saturating_add(fee_policy.min_fee);
    let max_count = notes.len().min(MAX_COMBINATION_COUNT);
    for count in 2..=max_count {
        let mut combos = Vec::new();
        collect_combinations_for_count(
            &asc,
            notes,
            combination_target,
            count,
            0,
            &mut Vec::new(),
            0,
            &mut combos,
            MAX_COMBINATIONS_PER_COUNT,
        );
        for combo in combos {
            let mut by_large = combo.clone();
            by_large.sort_by(|a, b| {
                notes[*b]
                    .capacity
                    .cmp(&notes[*a].capacity)
                    .then_with(|| notes[*a].origin_page.cmp(&notes[*b].origin_page))
            });
            push_candidate_order(&mut orders, by_large);

            let mut by_small = combo;
            by_small.sort_by(|a, b| {
                notes[*a]
                    .assets
                    .cmp(&notes[*b].assets)
                    .then_with(|| notes[*a].origin_page.cmp(&notes[*b].origin_page))
            });
            push_candidate_order(&mut orders, by_small);
        }
    }

    push_candidate_order(&mut orders, desc);
    orders
}

fn choose_best_candidate_plan(
    arena: &mut Arena,
    notes: &[NoteSpec],
    seeds: &[SeedSpec],
    output_total: u64,
    source_pkh: [u64; 5],
    refund_lock_root: [u64; 5],
    refund_note_data: Noun,
    refund_recipient_b58: &str,
    current_height: u64,
    fee_policy: FeePolicy,
) -> Result<CandidatePlan, JsValue> {
    let mut best: Option<CandidatePlan> = None;
    for order in candidate_note_orders(notes, output_total, fee_policy) {
        let Some(plans) = build_initial_plans_for_order(notes, &order, seeds)? else {
            continue;
        };
        let Some(candidate) = settle_candidate_fees(
            arena,
            notes,
            plans,
            source_pkh,
            refund_lock_root,
            refund_note_data,
            refund_recipient_b58,
            current_height,
            fee_policy,
        )?
        else {
            continue;
        };

        if candidate_better(&candidate, best.as_ref()) {
            best = Some(candidate);
        }
    }

    best.ok_or_else(|| js_err("insufficient funds to cover requested outputs and fee"))
}

// ---- lock-merkle-proof (spending from an OR-composed lock) ----------------
//
// Mirrors nockchain `++prove-hashable-by-index` / `++verify-merk-proof`
// (hoon/common/ztd/three.hoon). The hashable of an OR lock is
// `[leaf+tag, balanced-payload]`; a leaf's `axis` is its Nock tree address and
// the `path` is the sibling digests leaf→root.

/// Abstract hashable tree node used for proof construction.
enum HTree {
    Leaf([u64; 5]),
    Node(Box<HTree>, Box<HTree>),
}

fn htree_node_digest(t: &HTree) -> [u64; 5] {
    match t {
        HTree::Leaf(d) => *d,
        HTree::Node(p, q) => {
            hash_ten_cell(htree_node_digest(p), htree_node_digest(q)).unwrap_or([0; 5])
        }
    }
}

fn htree_leaf_count(t: &HTree) -> u64 {
    match t {
        HTree::Leaf(_) => 1,
        HTree::Node(p, q) => htree_leaf_count(p) + htree_leaf_count(q),
    }
}

/// Hoon `++peg`: replace the leading 1 of `b` with `a` (a → high bits).
fn peg(a: u64, b: u64) -> u64 {
    if b == 1 {
        return a;
    }
    let d = (64 - b.leading_zeros() as u64) - 1; // dec(xeb(b))
    (a << d) + (b - (1u64 << d))
}

/// `++go`: returns (root, path, axis) for the 1-indexed leaf `i`.
fn htree_prove(t: &HTree, i: u64) -> Result<([u64; 5], Vec<[u64; 5]>, u64), JsValue> {
    match t {
        HTree::Leaf(d) => Ok((*d, Vec::new(), 1)),
        HTree::Node(p, q) => {
            let lc = htree_leaf_count(p);
            if i <= lc {
                let (root, mut path, axis) = htree_prove(p, i)?;
                let sib = htree_node_digest(q);
                let new_root = hash_ten_cell(root, sib).map_err(|e| js_err(format!("{e:?}")))?;
                path.push(sib);
                Ok((new_root, path, peg(2, axis)))
            } else {
                let (root, mut path, axis) = htree_prove(q, i - lc)?;
                let sib = htree_node_digest(p);
                let new_root = hash_ten_cell(sib, root).map_err(|e| js_err(format!("{e:?}")))?;
                path.push(sib);
                Ok((new_root, path, peg(3, axis)))
            }
        }
    }
}

/// `++verify-merk-proof`: recompute the root from leaf + axis + path. Used by
/// tests as the correctness oracle (the chain runs the equivalent verifier).
#[cfg(test)]
fn merk_verify_root(
    leaf: [u64; 5],
    axis: u64,
    path: &[[u64; 5]],
) -> Result<Option<[u64; 5]>, JsValue> {
    let mut axis = axis;
    let mut leaf = leaf;
    let mut idx = 0usize;
    if axis == 0 {
        return Ok(None);
    }
    loop {
        if axis == 1 {
            return Ok(if idx == path.len() { Some(leaf) } else { None });
        }
        if idx >= path.len() {
            return Ok(None);
        }
        let sib = path[idx];
        if axis == 2 {
            let r = hash_ten_cell(leaf, sib).map_err(|e| js_err(format!("{e:?}")))?;
            return Ok(if idx + 1 == path.len() { Some(r) } else { None });
        }
        if axis == 3 {
            let r = hash_ten_cell(sib, leaf).map_err(|e| js_err(format!("{e:?}")))?;
            return Ok(if idx + 1 == path.len() { Some(r) } else { None });
        }
        if axis % 2 == 0 {
            leaf = hash_ten_cell(leaf, sib).map_err(|e| js_err(format!("{e:?}")))?;
            axis /= 2;
        } else {
            leaf = hash_ten_cell(sib, leaf).map_err(|e| js_err(format!("{e:?}")))?;
            axis = (axis - 1) / 2;
        }
        idx += 1;
    }
}

/// Build the hashable tree for an OR lock from its (already power-of-two,
/// padded) leaf spend-condition hashes: `[leaf+tag, balanced-payload]`.
fn or_hashable_tree(arena: &mut Arena, leaf_hashes: &[[u64; 5]], size: usize) -> HTree {
    fn balanced(hs: &[[u64; 5]]) -> HTree {
        if hs.len() == 1 {
            HTree::Leaf(hs[0])
        } else {
            let half = hs.len() / 2;
            HTree::Node(
                Box::new(balanced(&hs[..half])),
                Box::new(balanced(&hs[half..])),
            )
        }
    }
    let tag_atom = arena.alloc_atom_u64(size as u64);
    let tag_digest = hash_noun_varlen(tag_atom, arena).unwrap_or([0; 5]);
    HTree::Node(Box::new(HTree::Leaf(tag_digest)), Box::new(balanced(leaf_hashes)))
}

/// True if a pkh-signature-map value is an all-zero placeholder slot.
fn is_placeholder_sig(value: Noun, arena: &Arena) -> bool {
    fn all_zero(noun: Noun, arena: &Arena, count: usize) -> bool {
        let mut cur = noun;
        for _ in 0..count.saturating_sub(1) {
            let Some((head, tail)) = (match cur {
                Noun::Cell(id) => {
                    let c = arena.cell(id);
                    Some((c.head, c.tail))
                }
                _ => None,
            }) else {
                return false;
            };
            if noun_atom_u64(head, arena) != Some(0) {
                return false;
            }
            cur = tail;
        }
        noun_atom_u64(cur, arena) == Some(0)
    }
    let Some((pk, sig)) = tuple2(value, arena) else {
        return true;
    };
    let Some((x, y, _inf)) = tuple3(pk, arena) else {
        return true;
    };
    if !all_zero(x, arena, 6) || !all_zero(y, arena, 6) {
        return false;
    }
    let Some((chal, s)) = tuple2(sig, arena) else {
        return true;
    };
    all_zero(chal, arena, 8) && all_zero(s, arena, 8)
}

/// Union the real (non-placeholder) entries of two pkh-signature z-maps,
/// preferring real signatures from either side.
fn merge_pkh_maps(arena: &mut Arena, base: Noun, extra: Noun) -> Result<Noun, JsValue> {
    let mut out = base;
    let mut stack = vec![extra];
    while let Some(map) = stack.pop() {
        if map == arena.atom0() {
            continue;
        }
        let Some((node, left, right)) = decompose_map(map, arena) else {
            continue;
        };
        if let Some((key, value)) = decompose_pair(node, arena) {
            if !is_placeholder_sig(value, arena) {
                out = canonical_zmap_put(arena, out, key, value)
                    .map_err(|e| js_err(format!("zmap: {e:?}")))?;
            }
        }
        stack.push(left);
        stack.push(right);
    }
    Ok(out)
}

/// Merge the witness signatures of one spend (`[%1 [witness seeds fee]]`),
/// taking seeds/fee from `base` and unioning the pkh-signature maps.
fn merge_spend(arena: &mut Arena, base: Noun, extra: Noun) -> Result<Noun, JsValue> {
    let (tag, body) = tuple2(base, arena).ok_or_else(|| js_err("malformed spend"))?;
    let (witness, seeds, fee) = tuple3(body, arena).ok_or_else(|| js_err("malformed spend body"))?;
    let (lmp, pkh, hax, tim) =
        tuple4(witness, arena).ok_or_else(|| js_err("malformed witness"))?;
    // Find the same spend's witness pkh map in `extra`.
    let extra_pkh = tuple2(extra, arena)
        .and_then(|(_t, b)| tuple3(b, arena))
        .and_then(|(w, _s, _f)| tuple4(w, arena))
        .map(|(_lmp, p, _h, _t)| p)
        .unwrap_or_else(|| arena.atom0());
    let merged_pkh = merge_pkh_maps(arena, pkh, extra_pkh)?;
    let merged_witness = build_tuple(arena, &[lmp, merged_pkh, hax, tim]);
    let merged_body = build_tuple(arena, &[merged_witness, seeds, fee]);
    Ok(build_tuple(arena, &[tag, merged_body]))
}

/// Merge two wallet z-maps (`name -> spend`/`name -> witness`) keyed by name.
fn collect_map_entries(map: Noun, arena: &Arena, out: &mut Vec<(Noun, Noun)>) {
    if map == arena.atom0() || out.len() >= 256 {
        return;
    }
    let Some((node, left, right)) = decompose_map(map, arena) else {
        return;
    };
    if let Some(pair) = decompose_pair(node, arena) {
        out.push(pair);
    }
    collect_map_entries(left, arena, out);
    collect_map_entries(right, arena, out);
}

/// Combine two partially-signed v1 wallet transactions (`.psnt`) for the same
/// transaction by unioning their per-input signature maps — the parallel
/// multisig co-signing step.
///
/// VALIDATION: structurally tested (synthetic signatures); confirm against a
/// real device-signed multisig `.psnt` before relying on it for funds.
#[wasm_bindgen]
pub fn merge_signed_tx(a: &[u8], b: &[u8]) -> Result<Vec<u8>, JsValue> {
    let mut arena = Arena::new();
    let ta = cue(a, &mut arena).map_err(|_| js_err("first tx is not a valid jam"))?;
    let tb = cue(b, &mut arena).map_err(|_| js_err("second tx is not a valid jam"))?;
    let (tag, name, spends_a, display, wd_a) =
        tuple5(ta, &arena).ok_or_else(|| js_err("first tx is not a v1 wallet tx"))?;
    let (_tb_tag, _tb_name, spends_b, _tb_display, wd_b) =
        tuple5(tb, &arena).ok_or_else(|| js_err("second tx is not a v1 wallet tx"))?;

    // Index b's spends by name (b58) for matching.
    let mut b_spends = Vec::new();
    collect_map_entries(spends_b, &arena, &mut b_spends);
    let find_b = |name_noun: Noun, arena: &Arena| -> Noun {
        let want = parse_hash(name_noun, arena);
        for (k, v) in &b_spends {
            if parse_hash(*k, arena) == want {
                return *v;
            }
        }
        arena.atom0()
    };

    // Rebuild the spends map with merged signatures.
    let mut a_spends = Vec::new();
    collect_map_entries(spends_a, &arena, &mut a_spends);
    let mut merged_spends = arena.atom0();
    for (name_noun, spend_a) in a_spends {
        let extra = find_b(name_noun, &arena);
        let merged = if extra == arena.atom0() {
            spend_a
        } else {
            merge_spend(&mut arena, spend_a, extra)?
        };
        merged_spends = canonical_zmap_put(&mut arena, merged_spends, name_noun, merged)
            .map_err(|e| js_err(format!("zmap: {e:?}")))?;
    }

    // Rebuild the witness-data map (`[%1 map]`) the same way.
    let (wd_tag, wd_map_a) = tuple2(wd_a, &arena).ok_or_else(|| js_err("malformed witness-data"))?;
    let wd_map_b = tuple2(wd_b, &arena).map(|(_t, m)| m).unwrap_or_else(|| arena.atom0());
    let mut b_wits = Vec::new();
    collect_map_entries(wd_map_b, &arena, &mut b_wits);
    let mut a_wits = Vec::new();
    collect_map_entries(wd_map_a, &arena, &mut a_wits);
    let mut merged_wd_map = arena.atom0();
    for (name_noun, wit_a) in a_wits {
        let want = parse_hash(name_noun, &arena);
        let extra_pkh = b_wits
            .iter()
            .find(|(k, _)| parse_hash(*k, &arena) == want)
            .and_then(|(_, w)| tuple4(*w, &arena))
            .map(|(_lmp, p, _h, _t)| p)
            .unwrap_or_else(|| arena.atom0());
        let merged_wit = if let Some((lmp, pkh, hax, tim)) = tuple4(wit_a, &arena) {
            let mp = merge_pkh_maps(&mut arena, pkh, extra_pkh)?;
            build_tuple(&mut arena, &[lmp, mp, hax, tim])
        } else {
            wit_a
        };
        merged_wd_map = canonical_zmap_put(&mut arena, merged_wd_map, name_noun, merged_wit)
            .map_err(|e| js_err(format!("zmap: {e:?}")))?;
    }
    let merged_wd = build_tuple(&mut arena, &[wd_tag, merged_wd_map]);

    let merged = build_tuple(&mut arena, &[tag, name, merged_spends, display, merged_wd]);
    Ok(jam(merged, &arena))
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

    // Parse an optional multisig source lock (sorted, deduped digests).
    let source_multisig: Option<(u64, Vec<[u64; 5]>)> = match &input.source_multisig {
        Some(ms) => {
            if ms.m == 0 {
                return Err(js_err("multisig source requires m >= 1"));
            }
            let mut digests: Vec<[u64; 5]> = Vec::with_capacity(ms.pkhs.len());
            for pkh_b58 in &ms.pkhs {
                digests.push(parse_b58_hash(pkh_b58)?);
            }
            digests.sort();
            digests.dedup();
            if (ms.m as usize) > digests.len() {
                return Err(js_err("multisig source requires m <= unique signers"));
            }
            Some((ms.m, digests))
        }
        None => None,
    };

    // Change returns to the same lock the inputs use: the multisig lock for a
    // multisig source, else the source pkh.
    let (refund_lock, refund_recipient_b58) = if let Some((m, pkhs)) = &source_multisig {
        let ms_lock = build_pkh_lock_multisig(&mut arena, *m, pkhs)?;
        let ms_list = build_list(&mut arena, &[ms_lock]);
        let root = hash_lock_primitives_list(ms_list, &arena, &ctx)?;
        (ms_list, hash_to_b58(root))
    } else {
        let refund_pkh = build_pkh_lock(&mut arena, source_pkh)?;
        (build_list(&mut arena, &[refund_pkh]), input.source_pkh.clone())
    };
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
                source_multisig.as_ref(),
                input.source_or_lock.as_ref(),
                &ctx,
                coinbase_rel_min,
                fee_policy.bythos_phase,
            )
        })
        .collect::<Result<_, _>>()?;

    // Keep a deterministic base order. Candidate generation will choose and
    // reorder subsets by fee score.
    notes.sort_by(|a, b| {
        a.assets
            .cmp(&b.assets)
            .then_with(|| a.origin_page.cmp(&b.origin_page))
    });

    // Build output seeds (largest first).
    let mut seeds: Vec<SeedSpec> = Vec::new();
    let mut output_total = 0u64;
    for output in &input.outputs {
        if output.amount == 0 {
            return Err(js_err("output amount must be > 0"));
        }
        output_total = output_total.saturating_add(output.amount);
        let seed = build_output_seed_spec(&mut arena, output, &ctx)?;
        seeds.push(seed);
    }
    seeds.sort_by(|a, b| b.gift.cmp(&a.gift));

    let best_plan = choose_best_candidate_plan(
        &mut arena,
        notes.as_slice(),
        seeds.as_slice(),
        output_total,
        source_pkh,
        refund_lock_root,
        refund_note_data,
        &refund_recipient_b58,
        current_height,
        fee_policy,
    )?;
    let plans = best_plan.plans;
    let final_fee_breakdown = best_plan.fee_breakdown;

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
            &refund_recipient_b58,
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
                timelock: None,
                hashlock: None,
                burn: false,
                or_branches: None,
            }],
            coinbase_rel_min: None,
            current_height: Some(BYTHOS_PHASE),
            bythos_phase: None,
            base_fee: None,
            input_fee_divisor: None,
            min_fee: None,
            source_multisig: None,
            source_or_lock: None,
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
    fn wallet_tx_v1_multisig_source_reconstructs_multisig_lock() {
        // Three synthetic signer pkhs and a 2-of-3 lock.
        let m = 2u64;
        let signer_b58: Vec<String> = [[1u64, 0, 0, 0, 0], [2, 0, 0, 0, 0], [3, 0, 0, 0, 0]]
            .iter()
            .map(|d| hash_to_b58(*d))
            .collect();

        let mut arena = Arena::new();
        let null_digest = hash_noun_varlen(arena.atom0(), &arena).unwrap();
        let fake_atom = arena.alloc_atom_bytes(b"fake");
        let fake_digest = hash_noun_varlen(fake_atom, &arena).unwrap();
        let version_atom = arena.alloc_atom_u64(1);
        let version_digest = hash_noun_varlen(version_atom, &arena).unwrap();
        let ctx = TxIdCtx {
            null_digest,
            fake_digest,
            version_digest,
        };

        // Derive the note's first name from the 2-of-3 multisig lock so the
        // input reconstruction must match.
        let mut digests: Vec<[u64; 5]> =
            signer_b58.iter().map(|b| parse_b58_hash(b).unwrap()).collect();
        digests.sort();
        digests.dedup();
        let ms_lock = build_pkh_lock_multisig(&mut arena, m, &digests).unwrap();
        let ms_list = build_list(&mut arena, &[ms_lock]);
        let ms_root = hash_lock_primitives_list(ms_list, &arena, &ctx).unwrap();
        let note_first = compute_first_name_from_lock_hash(ms_root, &ctx).unwrap();

        // Compose succeeds only if the multisig lock reconstruction matches the
        // note name (proves source_multisig threading + lock build).
        let composed = compose_tx_v1_unsigned_inner(ComposeTxV1Input {
            source_pkh: signer_b58[0].clone(),
            notes: vec![NoteInput {
                name_first: hash_to_b58(note_first),
                name_last: signer_b58[0].clone(),
                origin_page: 1,
                assets: 100_000_000,
                version: 1,
            }],
            outputs: vec![OutputInput {
                recipient: RecipientInput::Pkh(signer_b58[0].clone()),
                amount: 65_536,
                alias: None,
                timelock: None,
                hashlock: None,
                burn: false,
                or_branches: None,
            }],
            coinbase_rel_min: None,
            current_height: Some(BYTHOS_PHASE),
            bythos_phase: None,
            base_fee: None,
            input_fee_divisor: None,
            min_fee: None,
            source_multisig: Some(MultisigSourceInput {
                m,
                pkhs: signer_b58.clone(),
            }),
            source_or_lock: None,
        })
        .expect("compose multisig");

        // The input-display lock must be the m-of-n multisig (m == 2).
        let mut decode_arena = Arena::new();
        let tx_noun =
            tx_types::pokenoun::cue(&composed.wallet_jam, &mut decode_arena).expect("cue");
        let (_tag, _name, _spends, display, _wd) = tuple5(tx_noun, &decode_arena).expect("tuple5");
        let (inputs, _outputs) = tuple2(display, &decode_arena).expect("display");
        let (_in_tag, in_map) = tuple2(inputs, &decode_arena).expect("inputs");
        let lock_list = first_zmap_value(in_map, &decode_arena);
        let (pkh_prim, _rest) = uncons(lock_list, &decode_arena).expect("lock list head");
        let (_header, body) = tuple2(pkh_prim, &decode_arena).expect("pkh prim");
        let (m_noun, _set) = tuple2(body, &decode_arena).expect("pkh body");
        assert_eq!(noun_atom_u64(m_noun, &decode_arena), Some(2));
    }

    #[test]
    fn wallet_tx_v1_timelock_output_changes_lock_root() {
        let source_pkh_b58 = "9yPePjfWAdUnzaQKyxcRXKRa5PpUzKKEwtpECBZsUYt9Jd7egSDEWoV";
        let source_pkh = parse_b58_hash(source_pkh_b58).unwrap();

        let mut arena = Arena::new();
        let null_digest = hash_noun_varlen(arena.atom0(), &arena).unwrap();
        let fake_atom = arena.alloc_atom_bytes(b"fake");
        let fake_digest = hash_noun_varlen(fake_atom, &arena).unwrap();
        let version_atom = arena.alloc_atom_u64(1);
        let version_digest = hash_noun_varlen(version_atom, &arena).unwrap();
        let ctx = TxIdCtx {
            null_digest,
            fake_digest,
            version_digest,
        };
        let simple_pkh = build_pkh_lock(&mut arena, source_pkh).unwrap();
        let simple_lock = build_list(&mut arena, &[simple_pkh]);
        let simple_lock_hash = hash_lock_primitives_list(simple_lock, &arena, &ctx).unwrap();
        let note_first = compute_first_name_from_lock_hash(simple_lock_hash, &ctx).unwrap();

        let make = |timelock: Option<TimelockSpec>| {
            compose_tx_v1_unsigned_inner(ComposeTxV1Input {
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
                    timelock,
                    hashlock: None,
                    burn: false,
                    or_branches: None,
                }],
                coinbase_rel_min: None,
                current_height: Some(BYTHOS_PHASE),
                bythos_phase: None,
                base_fee: None,
                input_fee_divisor: None,
                min_fee: None,
                source_multisig: None,
                source_or_lock: None,
            })
            .expect("compose")
        };

        let plain = make(None);
        let timelocked = make(Some(TimelockSpec {
            rel_min: None,
            rel_max: None,
            abs_min: Some(120_000),
            abs_max: None,
        }));
        // The timelocked output's lock (pkh + tim) hashes differently, so the
        // whole transaction id changes — proving the %tim primitive is in the
        // output's spend-condition. (The no-bounds rejection path returns a
        // JsValue error, which only runs under wasm, so it isn't asserted here.)
        assert_ne!(plain.tx_id, timelocked.tx_id);
    }

    #[test]
    fn wallet_tx_v1_hashlock_and_burn_outputs() {
        let source_pkh_b58 = "9yPePjfWAdUnzaQKyxcRXKRa5PpUzKKEwtpECBZsUYt9Jd7egSDEWoV";
        let source_pkh = parse_b58_hash(source_pkh_b58).unwrap();

        let mut arena = Arena::new();
        let null_digest = hash_noun_varlen(arena.atom0(), &arena).unwrap();
        let fake_atom = arena.alloc_atom_bytes(b"fake");
        let fake_digest = hash_noun_varlen(fake_atom, &arena).unwrap();
        let version_atom = arena.alloc_atom_u64(1);
        let version_digest = hash_noun_varlen(version_atom, &arena).unwrap();
        let ctx = TxIdCtx {
            null_digest,
            fake_digest,
            version_digest,
        };
        let simple_pkh = build_pkh_lock(&mut arena, source_pkh).unwrap();
        let simple_lock = build_list(&mut arena, &[simple_pkh]);
        let simple_lock_hash = hash_lock_primitives_list(simple_lock, &arena, &ctx).unwrap();
        let note_first = compute_first_name_from_lock_hash(simple_lock_hash, &ctx).unwrap();

        let commit_a = hash_to_b58([7u64, 0, 0, 0, 0]);
        let commit_b = hash_to_b58([8u64, 0, 0, 0, 0]);
        let make = |hashlock: Option<Vec<String>>, burn: bool| {
            compose_tx_v1_unsigned_inner(ComposeTxV1Input {
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
                    timelock: None,
                    hashlock,
                    burn,
                    or_branches: None,
                }],
                coinbase_rel_min: None,
                current_height: Some(BYTHOS_PHASE),
                bythos_phase: None,
                base_fee: None,
                input_fee_divisor: None,
                min_fee: None,
                source_multisig: None,
                source_or_lock: None,
            })
            .expect("compose")
        };

        let plain = make(None, false);
        let hax_a = make(Some(vec![commit_a.clone()]), false);
        let hax_b = make(Some(vec![commit_b.clone()]), false);
        let burned = make(None, true);

        // Hashlock changes the lock, and — critically — DIFFERENT commitments
        // produce DIFFERENT tx-ids. This is the regression guard for the old
        // `%hax` stub that hashed a fixed placeholder instead of the z-set.
        assert_ne!(plain.tx_id, hax_a.tx_id);
        assert_ne!(hax_a.tx_id, hax_b.tx_id);
        // Burn is also a distinct lock.
        assert_ne!(plain.tx_id, burned.tx_id);
    }

    // Build a TxIdCtx for tests.
    fn test_ctx(arena: &mut Arena) -> TxIdCtx {
        let null_digest = hash_noun_varlen(arena.atom0(), arena).unwrap();
        let fake_atom = arena.alloc_atom_bytes(b"fake");
        let fake_digest = hash_noun_varlen(fake_atom, arena).unwrap();
        let version_atom = arena.alloc_atom_u64(1);
        let version_digest = hash_noun_varlen(version_atom, arena).unwrap();
        TxIdCtx {
            null_digest,
            fake_digest,
            version_digest,
        }
    }

    /// Golden lock-root vectors copied verbatim from nockchain-wallet's own
    /// tests (`crates/nockchain-wallet/src/tests.rs`,
    /// `planner_recipient_outputs_match_hoon_lock_root_vectors`). Proves our
    /// pkh / multisig lock hashing matches the canonical wallet exactly —
    /// which also validates the per-leaf hashing OR-composed locks reuse.
    #[test]
    fn lock_roots_match_nockchain_wallet_vectors() {
        const ADDRESS_A: &str = "9yPePjfWAdUnzaQKyxcRXKRa5PpUzKKEwtpECBZsUYt9Jd7egSDEWoV";
        const ADDRESS_B: &str = "9phXGACnW4238oqgvn2gpwaUjG3RAqcxq2Ash2vaKp8KjzSd3MQ56Jt";
        const EXPECTED_PKH_ROOT: &str = "DKrgXqE8bXR1uBZ3t4vU13m2KquGCDbnn1PeoPL7dxSHTucGPFDPt53";
        const EXPECTED_MULTISIG_2_OF_2: &str =
            "4eMAT3BuhLPjYFronoYJ9RSLVSgveCL3nQB7RHSLZzjBTiYCxEzkzEH";

        let mut arena = Arena::new();
        let ctx = test_ctx(&mut arena);
        let a = parse_b58_hash(ADDRESS_A).unwrap();
        let b = parse_b58_hash(ADDRESS_B).unwrap();

        // 1-of-1 P2PKH lock root.
        let pkh = build_pkh_lock(&mut arena, a).unwrap();
        let lock = build_list(&mut arena, &[pkh]);
        let root = hash_lock_primitives_list(lock, &arena, &ctx).unwrap();
        assert_eq!(hash_to_b58(root), EXPECTED_PKH_ROOT);

        // 2-of-2 multisig lock root.
        let mut digests = vec![a, b];
        digests.sort();
        digests.dedup();
        let ms = build_pkh_lock_multisig(&mut arena, 2, &digests).unwrap();
        let ms_lock = build_list(&mut arena, &[ms]);
        let ms_root = hash_lock_primitives_list(ms_lock, &arena, &ctx).unwrap();
        assert_eq!(hash_to_b58(ms_root), EXPECTED_MULTISIG_2_OF_2);
    }

    #[test]
    fn inspect_tx_builds_tree_for_composed_draft() {
        let source = "9yPePjfWAdUnzaQKyxcRXKRa5PpUzKKEwtpECBZsUYt9Jd7egSDEWoV";
        let source_pkh = parse_b58_hash(source).unwrap();
        let mut arena = Arena::new();
        let ctx = test_ctx(&mut arena);
        let pkh = build_pkh_lock(&mut arena, source_pkh).unwrap();
        let lock = build_list(&mut arena, &[pkh]);
        let note_first =
            compute_first_name_from_lock_hash(hash_lock_primitives_list(lock, &arena, &ctx).unwrap(), &ctx)
                .unwrap();

        let composed = compose_tx_v1_unsigned_inner(ComposeTxV1Input {
            source_pkh: source.to_string(),
            notes: vec![NoteInput {
                name_first: hash_to_b58(note_first),
                name_last: source.to_string(),
                origin_page: 1,
                assets: 100_000_000,
                version: 1,
            }],
            outputs: vec![OutputInput {
                recipient: RecipientInput::Pkh(source.to_string()),
                amount: 65_536,
                alias: None,
                timelock: Some(TimelockSpec {
                    rel_min: None,
                    rel_max: None,
                    abs_min: Some(99_000),
                    abs_max: None,
                }),
                hashlock: None,
                burn: false,
                or_branches: None,
            }],
            coinbase_rel_min: None,
            current_height: Some(BYTHOS_PHASE),
            bythos_phase: None,
            base_fee: None,
            input_fee_divisor: None,
            min_fee: None,
            source_multisig: None,
            source_or_lock: None,
        })
        .expect("compose");

        // The typed inspector decodes the composed draft into a labeled tree.
        let tree = crate::review_v1::inspect_tx(&composed.wallet_jam).expect("inspect");
        assert_eq!(tree.label, "transaction");
        assert!(!tree.children.is_empty());
        // The composed jam serializes (round-trips through the inspector) and
        // surfaces the timelock somewhere in the tree.
        fn has_label(n: &crate::review_v1::TxTreeNode, want: &str) -> bool {
            n.label == want || n.children.iter().any(|c| has_label(c, want))
        }
        assert!(has_label(&tree, "timelock"));

        // Merging the signed tx with itself must produce a structurally valid
        // re-jammed wallet tx (no corruption; signature union is exercised).
        let merged = merge_signed_tx(&composed.wallet_jam, &composed.wallet_jam).expect("merge");
        let mut merge_arena = Arena::new();
        let merged_noun = cue(&merged, &mut merge_arena).expect("cue merged");
        assert!(tuple5(merged_noun, &merge_arena).is_some());
        let merged_tree = crate::review_v1::inspect_tx(&merged).expect("inspect merged");
        assert_eq!(merged_tree.label, "transaction");
    }

    /// Golden vectors for timelock, hashlock, and OR-composed (`%2`/`%4`)
    /// lock roots, copied from nockchain's `lock_hash_matches_known_hoon_vectors`
    /// (`crates/nockchain-types/.../tx.rs`). Closes the OR-composed fund-safety
    /// gap: the tree hashing now matches the canonical source on real vectors.
    #[test]
    fn or_composed_lock_roots_match_nockchain_vectors() {
        const ADDRESS_A: &str = "9yPePjfWAdUnzaQKyxcRXKRa5PpUzKKEwtpECBZsUYt9Jd7egSDEWoV";
        const ADDRESS_B: &str = "9phXGACnW4238oqgvn2gpwaUjG3RAqcxq2Ash2vaKp8KjzSd3MQ56Jt";
        const EXPECTED_TIM: &str = "66FLtgznHvE7v4Fi4wZ6aA9EzsPD6pfaL3qL85apJuiBF8unRKXVsor";
        const EXPECTED_HAX: &str = "4kwz3RMCacfRXY3ydNoQ1tsUKuzaBEzGSpX9GpSWf8T3Rj24Ucuj6v4";
        const EXPECTED_V2: &str = "e3qeUqDf6ZTkayiiQDpKpax6RqXMBAMRLtrppvL41EdyJYFj743ZKB";
        const EXPECTED_V4: &str = "6ezbUN1ozEvZi9TUGVN1pY2TcCJc5KWoCzjj519ihE6LGupvJpnysjo";

        let mut arena = Arena::new();
        let ctx = test_ctx(&mut arena);
        let a = parse_b58_hash(ADDRESS_A).unwrap();
        let b = parse_b58_hash(ADDRESS_B).unwrap();

        // Single-condition timelock: [tim(rel 3..10, abs 20..)].
        let tim = build_tim_lock_full(&mut arena, Some(3), Some(10), Some(20), None);
        let tim_sc = build_list(&mut arena, &[tim]);
        let tim_root = hash_lock_primitives_list(tim_sc, &arena, &ctx).unwrap();
        assert_eq!(hash_to_b58(tim_root), EXPECTED_TIM);

        // Single-condition hashlock over {A, B}.
        let hax = build_hax_lock(&mut arena, &{
            let mut v = vec![a, b];
            v.sort();
            v.dedup();
            v
        })
        .unwrap();
        let hax_sc = build_list(&mut arena, &[hax]);
        let hax_root = hash_lock_primitives_list(hax_sc, &arena, &ctx).unwrap();
        assert_eq!(hash_to_b58(hax_root), EXPECTED_HAX);

        // Spend-condition hashes for the tree leaves.
        let pkh_a = build_pkh_lock(&mut arena, a).unwrap();
        let sc_pkh = build_list(&mut arena, &[pkh_a]);
        let h_pkh = hash_lock_primitives_list(sc_pkh, &arena, &ctx).unwrap();
        let h_tim = tim_root;
        let h_hax = hax_root;
        let brn = build_brn_lock(&mut arena);
        let sc_brn = build_list(&mut arena, &[brn]);
        let h_brn = hash_lock_primitives_list(sc_brn, &arena, &ctx).unwrap();

        // %2 tree: [pkh(1,[A]), tim].
        let v2_root = hash_lock_tree_root(&mut arena, &[h_pkh, h_tim], 2).unwrap();
        assert_eq!(hash_to_b58(v2_root), EXPECTED_V2);

        // %4 tree: V4{ V2{pkh, tim}, V2{hax, burn} } → leaves [pkh, tim, hax, burn].
        let v4_root = hash_lock_tree_root(&mut arena, &[h_pkh, h_tim, h_hax, h_brn], 4).unwrap();
        assert_eq!(hash_to_b58(v4_root), EXPECTED_V4);
    }

    /// For every leaf of an OR lock, the prover's (axis, path) must verify back
    /// to the lock root via the chain's verifier — and that root must equal the
    /// wallet-validated `hash_lock_tree_root`. This is the fund-safety oracle
    /// for spending OR-composed inputs (HTLC claim/refund).
    /// Spend from an OR-composed (HTLC-shaped) input: branch 0 = recipient +
    /// preimage, branch 1 = refund pkh + timelock. Compose spending branch 0
    /// and confirm the input witness is a full lock-merkle-proof that verifies
    /// back to the input's lock root.
    #[test]
    fn compose_spends_or_composed_input_branch() {
        let a = "9yPePjfWAdUnzaQKyxcRXKRa5PpUzKKEwtpECBZsUYt9Jd7egSDEWoV";
        let b = "9phXGACnW4238oqgvn2gpwaUjG3RAqcxq2Ash2vaKp8KjzSd3MQ56Jt";
        let commit = hash_to_b58([123u64, 0, 0, 0, 0]);

        let branches = vec![
            LockBranch {
                recipient: Some(RecipientInput::Pkh(a.to_string())),
                timelock: None,
                hashlock: Some(vec![commit.clone()]),
                burn: false,
            },
            LockBranch {
                recipient: Some(RecipientInput::Pkh(b.to_string())),
                timelock: Some(TimelockSpec {
                    rel_min: None,
                    rel_max: None,
                    abs_min: Some(80_000),
                    abs_max: None,
                }),
                hashlock: None,
                burn: false,
            },
        ];

        // Derive the input note name from the OR lock root.
        let mut arena = Arena::new();
        let ctx = test_ctx(&mut arena);
        let (_lock, root, _wit, _signers) = build_or_input(&mut arena, &branches, 0, &ctx).unwrap();
        let note_first = compute_first_name_from_lock_hash(root, &ctx).unwrap();

        let composed = compose_tx_v1_unsigned_inner(ComposeTxV1Input {
            source_pkh: a.to_string(),
            notes: vec![NoteInput {
                name_first: hash_to_b58(note_first),
                name_last: a.to_string(),
                origin_page: 1,
                assets: 100_000_000,
                version: 1,
            }],
            outputs: vec![OutputInput {
                recipient: RecipientInput::Pkh(a.to_string()),
                amount: 65_536,
                alias: None,
                timelock: None,
                hashlock: None,
                burn: false,
                or_branches: None,
            }],
            coinbase_rel_min: None,
            current_height: Some(BYTHOS_PHASE),
            bythos_phase: None,
            base_fee: None,
            input_fee_divisor: None,
            min_fee: None,
            source_multisig: None,
            source_or_lock: Some(OrSourceInput {
                branches: branches.clone(),
                spend_branch: 0,
            }),
        })
        .expect("compose OR-input spend");

        // Decode the witness's lock-merkle-proof and verify it against the root.
        let mut da = Arena::new();
        let tx = cue(&composed.wallet_jam, &mut da).unwrap();
        let (_t, _n, spends, _d, _w) = tuple5(tx, &da).unwrap();
        let (node, _l, _r) = decompose_map(spends, &da).unwrap();
        let (_name, spend) = decompose_pair(node, &da).unwrap();
        let (_tag, body) = tuple2(spend, &da).unwrap();
        let (witness, _seeds, _fee) = tuple3(body, &da).unwrap();
        let (lmp, _pkh, _hax, _tim) = tuple4(witness, &da).unwrap();
        // Full lmp: [%full spend-condition axis [root path]].
        let (ver, sc, axis_noun, merk) = tuple4(lmp, &da).unwrap();
        assert!(da.atom_eq_bytes(
            match ver {
                Noun::Atom(id) => id,
                _ => panic!("version not atom"),
            },
            LOCK_MERKLE_PROOF_FULL_TAG
        ));
        let axis = noun_atom_u64(axis_noun, &da).unwrap();
        let (root_noun, path_noun) = tuple2(merk, &da).unwrap();
        let proof_root = parse_hash(root_noun, &da).unwrap();
        assert_eq!(proof_root, root);
        // Collect path hashes and verify the spend-condition hashes to the root.
        let mut path = Vec::new();
        let mut cur = path_noun;
        while cur != da.atom0() {
            let (h, t) = uncons(cur, &da).unwrap();
            path.push(parse_hash(h, &da).unwrap());
            cur = t;
        }
        let leaf_hash = hash_lock_primitives_list(sc, &da, &ctx).unwrap();
        let verified = merk_verify_root(leaf_hash, axis, &path).unwrap();
        assert_eq!(verified, Some(root));
    }

    #[test]
    fn lock_merkle_proof_round_trips_to_canonical_root() {
        let mut arena = Arena::new();
        let ctx = test_ctx(&mut arena);
        let a = parse_b58_hash("9yPePjfWAdUnzaQKyxcRXKRa5PpUzKKEwtpECBZsUYt9Jd7egSDEWoV").unwrap();
        let b = parse_b58_hash("9phXGACnW4238oqgvn2gpwaUjG3RAqcxq2Ash2vaKp8KjzSd3MQ56Jt").unwrap();

        // Build leaf spend-condition hashes for a few tree shapes.
        let sc_hash = |arena: &mut Arena, prim: Noun, ctx: &TxIdCtx| -> [u64; 5] {
            let sc = build_list(arena, &[prim]);
            hash_lock_primitives_list(sc, arena, ctx).unwrap()
        };

        for size in [2usize, 4] {
            // Distinct leaves: pkh(A), tim, [hax, burn padding...] as needed.
            let mut leaves = Vec::new();
            let pkh = build_pkh_lock(&mut arena, a);
            leaves.push(sc_hash(&mut arena, pkh.unwrap(), &ctx));
            let tim = build_tim_lock_full(&mut arena, Some(3), Some(10), Some(20), None);
            leaves.push(sc_hash(&mut arena, tim, &ctx));
            while leaves.len() < size {
                let hax = build_hax_lock(&mut arena, &[a, b]).unwrap();
                leaves.push(sc_hash(&mut arena, hax, &ctx));
            }

            let expected_root = hash_lock_tree_root(&mut arena, &leaves, size).unwrap();
            let tree = or_hashable_tree(&mut arena, &leaves, size);

            for leaf_number in 1..=size as u64 {
                // hashable index = leaf_number + 1 (the tag occupies leaf 1).
                let (prove_root, path, axis) = htree_prove(&tree, leaf_number + 1).unwrap();
                assert_eq!(prove_root, expected_root, "prove root (size {size}, leaf {leaf_number})");
                // The chain verifier reconstructs the same root from this leaf's
                // spend-condition hash + the proof.
                let leaf_hash = leaves[(leaf_number - 1) as usize];
                let verified = merk_verify_root(leaf_hash, axis, &path).unwrap();
                assert_eq!(
                    verified,
                    Some(expected_root),
                    "verify (size {size}, leaf {leaf_number}, axis {axis})"
                );
            }
        }
    }

    #[test]
    fn or_lock_root_matches_spec_formula() {
        let a = "9yPePjfWAdUnzaQKyxcRXKRa5PpUzKKEwtpECBZsUYt9Jd7egSDEWoV";
        let b = hash_to_b58([42u64, 0, 0, 0, 0]);

        let mut arena = Arena::new();
        let null_digest = hash_noun_varlen(arena.atom0(), &arena).unwrap();
        let fake_atom = arena.alloc_atom_bytes(b"fake");
        let fake_digest = hash_noun_varlen(fake_atom, &arena).unwrap();
        let version_atom = arena.alloc_atom_u64(1);
        let version_digest = hash_noun_varlen(version_atom, &arena).unwrap();
        let ctx = TxIdCtx {
            null_digest,
            fake_digest,
            version_digest,
        };

        // Two branches: a plain pkh and a timelocked pkh.
        let branches = vec![
            LockBranch {
                recipient: Some(RecipientInput::Pkh(a.to_string())),
                timelock: None,
                hashlock: None,
                burn: false,
            },
            LockBranch {
                recipient: Some(RecipientInput::Pkh(b.clone())),
                timelock: Some(TimelockSpec {
                    rel_min: None,
                    rel_max: None,
                    abs_min: Some(50_000),
                    abs_max: None,
                }),
                hashlock: None,
                burn: false,
            },
        ];
        let (_lock, root) = build_or_lock(&mut arena, &branches, &ctx).unwrap();

        // Recompute the expected root independently: each branch's
        // spend-condition hash, then hash-pair(leaf(2), hash-pair(h_a, h_b)).
        let (sc_a, _) = build_spend_condition(
            &mut arena,
            Some(&RecipientInput::Pkh(a.to_string())),
            None,
            None,
            false,
            &ctx,
        )
        .unwrap();
        let (sc_b, _) = build_spend_condition(
            &mut arena,
            Some(&RecipientInput::Pkh(b.clone())),
            Some(&TimelockSpec {
                rel_min: None,
                rel_max: None,
                abs_min: Some(50_000),
                abs_max: None,
            }),
            None,
            false,
            &ctx,
        )
        .unwrap();
        let h_a = hash_lock_primitives_list(sc_a, &arena, &ctx).unwrap();
        let h_b = hash_lock_primitives_list(sc_b, &arena, &ctx).unwrap();
        let tag2 = arena.alloc_atom_u64(2);
        let tag2_digest = hash_noun_varlen(tag2, &arena).unwrap();
        let pair = hash_ten_cell(h_a, h_b).unwrap();
        let expected = hash_ten_cell(tag2_digest, pair).unwrap();
        assert_eq!(root, expected);

        // Three branches pad to a %4 tree (≠ the 2-branch root).
        let three = vec![
            branches[0].clone(),
            branches[1].clone(),
            LockBranch {
                recipient: None,
                timelock: None,
                hashlock: None,
                burn: true,
            },
        ];
        let (_l3, root3) = build_or_lock(&mut arena, &three, &ctx).unwrap();
        assert_ne!(root, root3);
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
                timelock: None,
                hashlock: None,
                burn: false,
                or_branches: None,
            }],
            coinbase_rel_min: None,
            current_height: Some(BYTHOS_PHASE),
            bythos_phase: None,
            base_fee: None,
            input_fee_divisor: None,
            min_fee: None,
            source_multisig: None,
            source_or_lock: None,
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
                timelock: None,
                hashlock: None,
                burn: false,
                or_branches: None,
            }],
            coinbase_rel_min: None,
            current_height: Some(BYTHOS_PHASE),
            bythos_phase: None,
            base_fee: None,
            input_fee_divisor: None,
            min_fee: None,
            source_multisig: None,
            source_or_lock: None,
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
