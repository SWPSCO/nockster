extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use crate::draft_sign::noun_codec::{cue, jam, Arena, CodecError, Noun};
use crate::draft_sign::tip5;
use crate::draft_sign::zmap::{self, ZMapError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignDraftError {
    Codec(CodecError),
    Tip5(tip5::Tip5Error),
    ZMap(ZMapError),
    Malformed,
    Unsupported,
}

impl From<CodecError> for SignDraftError {
    fn from(e: CodecError) -> Self {
        Self::Codec(e)
    }
}

impl From<tip5::Tip5Error> for SignDraftError {
    fn from(e: tip5::Tip5Error) -> Self {
        Self::Tip5(e)
    }
}

impl From<ZMapError> for SignDraftError {
    fn from(e: ZMapError) -> Self {
        Self::ZMap(e)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SignerConfig {
    /// Cheetah secret key (32-byte big-endian).
    pub sk_be: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftOutputV1 {
    /// Base58-encoded recipient PKH digest.
    pub recipient_b58: String,
    /// Gift amount in nicks.
    pub gift: u64,
    /// True if this output pays back to the signing key (refund/change).
    pub is_refund: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftReviewV1 {
    pub outputs: Vec<DraftOutputV1>,
    /// Number of v1 spends in the draft.
    pub input_count: u32,
    /// Number of distinct non-refund recipient outputs shown to the user.
    pub external_output_count: u32,
    /// Sum of non-refund output gifts in nicks.
    pub external_total: u64,
    /// Sum of signer refund/change outputs in nicks.
    pub refund_total: u64,
    /// Sum of v1 spend fees in nicks.
    pub fee_total: u64,
    /// Minimum valid post-Bythos fee in nicks for the reviewed draft.
    pub minimum_fee: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteTxIdV1 {
    /// Canonical base58 V1 transaction id computed from the spends map.
    pub name: String,
    /// Replacement jammed transaction bytes, present only when the wrapper/id
    /// in the input was stale.
    pub rewritten: Option<Vec<u8>>,
}

// tx-types constant:
// Hash::from_b58("6mhCSwJQDvbkbiPAUNjetJtVoo1VLtEhmEYoU4hmdGd6ep1F6ayaV4A")
const LOCK_MERKLE_AXIS_HASH: [u64; 5] = [
    1988594973584463658,
    8158631336633700141,
    2161567007650232260,
    460329990575991155,
    8368574252173164961,
];
const LOCK_MERKLE_PROOF_FULL_TAG: &[u8] = b"full";
const BASE_FEE: u64 = 1 << 14;
const INPUT_FEE_DIVISOR: u64 = 4;
const MIN_FEE: u64 = 256;

fn cheetah_pub_from_sk_tuple(sk_be: [u8; 32]) -> ([u64; 6], [u64; 6]) {
    #[cfg(feature = "std")]
    {
        let pk = crate::cheetah_pub_from_sk(sk_be);
        (pk[0], pk[1])
    }

    #[cfg(not(feature = "std"))]
    {
        crate::cheetah_pub_from_sk(sk_be)
    }
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
) -> Result<(Option<Noun>, Noun, Noun, Noun), SignDraftError> {
    let (head, tail) = uncons(lmp, arena).ok_or(SignDraftError::Malformed)?;

    if atom_eq_bytes(head, LOCK_MERKLE_PROOF_FULL_TAG, arena) {
        let (spend_condition, axis, merkle_proof) =
            tuple3(tail, arena).ok_or(SignDraftError::Malformed)?;
        return Ok((Some(head), spend_condition, axis, merkle_proof));
    }

    let (axis, merkle_proof) = tuple2(tail, arena).ok_or(SignDraftError::Malformed)?;
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

fn hashable_noun_digest(noun: Noun, arena: &Arena) -> Result<[u64; 5], SignDraftError> {
    match noun {
        Noun::Atom(_) => Ok(tip5::hash_noun_varlen(noun, arena)?),
        Noun::Cell(id) => {
            let cell = arena.cell(id);
            let lh = hashable_noun_digest(cell.head, arena)?;
            let rh = hashable_noun_digest(cell.tail, arena)?;
            Ok(tip5::hash_ten_cell(lh, rh)?)
        }
    }
}

fn count_leaves(noun: Noun, arena: &Arena) -> u64 {
    match noun {
        Noun::Atom(_) => 1,
        Noun::Cell(id) => {
            let cell = arena.cell(id);
            count_leaves(cell.head, arena).saturating_add(count_leaves(cell.tail, arena))
        }
    }
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

fn merge_note_data_into(
    arena: &mut Arena,
    src: Noun,
    dst: &mut Noun,
) -> Result<(), SignDraftError> {
    if src == arena.atom0() {
        return Ok(());
    }
    let (node, left, right) = decompose_map(src, arena).ok_or(SignDraftError::Malformed)?;
    let (key, value) = decompose_pair(node, arena).ok_or(SignDraftError::Malformed)?;
    *dst = zmap::canonical_zmap_put(arena, *dst, key, value)?;
    merge_note_data_into(arena, left, dst)?;
    merge_note_data_into(arena, right, dst)
}

fn collect_seed_note_data_for_fee(
    arena: &mut Arena,
    seeds: Noun,
    merged: &mut Vec<([u64; 5], Noun)>,
) -> Result<(), SignDraftError> {
    if seeds == arena.atom0() {
        return Ok(());
    }

    let (seed, lr) = uncons(seeds, arena).ok_or(SignDraftError::Malformed)?;
    let (left, right) = uncons(lr, arena).ok_or(SignDraftError::Malformed)?;
    let (_output_source, lock_root_noun, note_data, _gift, _parent_hash) =
        tuple5(seed, arena).ok_or(SignDraftError::Malformed)?;
    let lock_root = parse_hash(lock_root_noun, arena).ok_or(SignDraftError::Malformed)?;

    if let Some((_root, data)) = merged.iter_mut().find(|(root, _)| *root == lock_root) {
        merge_note_data_into(arena, note_data, data)?;
    } else {
        let mut data = arena.atom0();
        merge_note_data_into(arena, note_data, &mut data)?;
        merged.push((lock_root, data));
    }

    collect_seed_note_data_for_fee(arena, left, merged)?;
    collect_seed_note_data_for_fee(arena, right, merged)
}

fn collect_spend_words_for_fee(
    arena: &mut Arena,
    spends: Noun,
    merged: &mut Vec<([u64; 5], Noun)>,
    witness_words: &mut u64,
) -> Result<(), SignDraftError> {
    if spends == arena.atom0() {
        return Ok(());
    }

    let (node, left, right) = decompose_map(spends, arena).ok_or(SignDraftError::Malformed)?;
    let (_name, spend) = decompose_pair(node, arena).ok_or(SignDraftError::Malformed)?;
    let (ver, body) = tuple2(spend, arena).ok_or(SignDraftError::Malformed)?;
    if noun_atom_u64(ver, arena) != Some(1) {
        return Err(SignDraftError::Unsupported);
    }
    let (witness, seeds, _fee) = tuple3(body, arena).ok_or(SignDraftError::Malformed)?;
    *witness_words = witness_words.saturating_add(count_leaves(witness, arena));
    collect_seed_note_data_for_fee(arena, seeds, merged)?;

    collect_spend_words_for_fee(arena, left, merged, witness_words)?;
    collect_spend_words_for_fee(arena, right, merged, witness_words)
}

fn calculate_minimum_fee_v1(arena: &mut Arena, spends: Noun) -> Result<u64, SignDraftError> {
    let mut merged = Vec::<([u64; 5], Noun)>::new();
    let mut witness_words = 0u64;
    collect_spend_words_for_fee(arena, spends, &mut merged, &mut witness_words)?;
    let seed_words = merged
        .iter()
        .map(|(_root, note_data)| count_leaves(*note_data, arena))
        .sum::<u64>();
    let seed_fee = seed_words.saturating_mul(BASE_FEE);
    let witness_fee = witness_words
        .saturating_mul(BASE_FEE)
        .saturating_div(INPUT_FEE_DIVISOR);
    Ok(seed_fee.saturating_add(witness_fee).max(MIN_FEE))
}

fn hash_note_data(map: Noun, arena: &Arena) -> Result<[u64; 5], SignDraftError> {
    // note-data hashable: empty map hashes as leaf+0
    if map == arena.atom0() {
        return Ok(tip5::hash_noun_varlen(arena.atom0(), arena)?);
    }

    let (node, left, right) = decompose_map(map, arena).ok_or(SignDraftError::Malformed)?;
    let (key, value) = decompose_pair(node, arena).ok_or(SignDraftError::Malformed)?;

    let key_digest = tip5::hash_noun_varlen(key, arena)?;
    let value_digest = hashable_noun_digest(value, arena)?;
    let node_digest = tip5::hash_ten_cell(key_digest, value_digest)?;

    let left_digest = hash_note_data(left, arena)?;
    let right_digest = hash_note_data(right, arena)?;
    let children_digest = tip5::hash_ten_cell(left_digest, right_digest)?;
    Ok(tip5::hash_ten_cell(node_digest, children_digest)?)
}

fn hash_source_hashable(source: Noun, arena: &Arena) -> Result<[u64; 5], SignDraftError> {
    // Source noun: [p=is_hash is_coinbase=bool]
    let (p_noun, is_coinbase_noun) = tuple2(source, arena).ok_or(SignDraftError::Malformed)?;
    let p = parse_hash(p_noun, arena).ok_or(SignDraftError::Malformed)?;
    let is_coinbase = noun_atom_u64(is_coinbase_noun, arena).ok_or(SignDraftError::Malformed)?;
    if is_coinbase > 1 {
        return Err(SignDraftError::Malformed);
    }

    // Hashable(source) = [hash+p leaf+bool]
    let bool_digest = tip5::hash_noun_varlen(is_coinbase_noun, arena)?;
    Ok(tip5::hash_ten_cell(p, bool_digest)?)
}

fn hash_output_source_unit(output_source: Noun, arena: &Arena) -> Result<[u64; 5], SignDraftError> {
    // Option<Source> noun:
    // - None: 0
    // - Some: [0 source]
    if output_source == arena.atom0() {
        return Ok(tip5::hash_noun_varlen(arena.atom0(), arena)?);
    }
    let (tag, src) = tuple2(output_source, arena).ok_or(SignDraftError::Malformed)?;
    if noun_atom_u64(tag, arena) != Some(0) {
        return Err(SignDraftError::Malformed);
    }

    let null_digest = tip5::hash_noun_varlen(arena.atom0(), arena)?;
    let src_digest = hash_source_hashable(src, arena)?;
    Ok(tip5::hash_ten_cell(null_digest, src_digest)?)
}

fn hash_seed_sig_hashable(seed: Noun, arena: &Arena) -> Result<[u64; 5], SignDraftError> {
    let (output_source, lock_root_noun, note_data_noun, gift_noun, parent_hash_noun) =
        tuple5(seed, arena).ok_or(SignDraftError::Malformed)?;

    let d1 = hash_output_source_unit(output_source, arena)?;
    let d2 = parse_hash(lock_root_noun, arena).ok_or(SignDraftError::Malformed)?;
    let note_data_hash = hash_note_data(note_data_noun, arena)?;
    let d3 = note_data_hash;
    let d4 = tip5::hash_noun_varlen(gift_noun, arena)?;
    let d5 = parse_hash(parent_hash_noun, arena).ok_or(SignDraftError::Malformed)?;

    // cell-chain fold from the right: [d1 [d2 [d3 [d4 d5]]]]
    let mut acc = d5;
    acc = tip5::hash_ten_cell(d4, acc)?;
    acc = tip5::hash_ten_cell(d3, acc)?;
    acc = tip5::hash_ten_cell(d2, acc)?;
    acc = tip5::hash_ten_cell(d1, acc)?;
    Ok(acc)
}

fn hash_seeds_sig(seeds_zset: Noun, arena: &Arena) -> Result<[u64; 5], SignDraftError> {
    if seeds_zset == arena.atom0() {
        return Ok(tip5::hash_noun_varlen(arena.atom0(), arena)?);
    }

    let (seed, lr) = uncons(seeds_zset, arena).ok_or(SignDraftError::Malformed)?;
    let (left, right) = uncons(lr, arena).ok_or(SignDraftError::Malformed)?;

    let node_digest = hash_seed_sig_hashable(seed, arena)?;
    let left_digest = hash_seeds_sig(left, arena)?;
    let right_digest = hash_seeds_sig(right, arena)?;
    let children_digest = tip5::hash_ten_cell(left_digest, right_digest)?;
    Ok(tip5::hash_ten_cell(node_digest, children_digest)?)
}

fn spend_v1_sig_hash(spend_body: Noun, arena: &Arena) -> Result<[u64; 5], SignDraftError> {
    let (_witness, seeds, fee) = tuple3(spend_body, arena).ok_or(SignDraftError::Malformed)?;
    let seeds_digest = hash_seeds_sig(seeds, arena)?;
    let fee_digest = tip5::hash_noun_varlen(fee, arena)?;
    Ok(tip5::hash_ten_cell(seeds_digest, fee_digest)?)
}

fn sign_spend_v1(
    arena: &mut Arena,
    spend: Noun,
    pk_noun: Noun,
    pkh_key_noun: Noun,
    pkh_digest: [u64; 5],
    cfg: &SignerConfig,
    pk_coords: ([u64; 6], [u64; 6]),
) -> Result<Noun, SignDraftError> {
    let (ver_noun, body_noun) = tuple2(spend, arena).ok_or(SignDraftError::Malformed)?;
    if noun_atom_u64(ver_noun, arena) != Some(1) {
        return Ok(spend);
    }

    // SpendV1 body: [witness seeds fee]
    let (witness, seeds, fee) = tuple3(body_noun, arena).ok_or(SignDraftError::Malformed)?;
    let (lmp, pkh_map, hax, tim) = tuple4(witness, arena).ok_or(SignDraftError::Malformed)?;

    // Only sign if the input lock actually authorizes our pkh, and only if doing so does not
    // exceed the lock's m-of-n signature count (tx-engine-1.hoon requires exactly m signatures).
    let (_version, spend_condition, _axis, _merk) = decompose_lock_merkle_proof(lmp, arena)?;
    let Some((m_required, allowed_hashes)) = spend_condition_pkh_lock(spend_condition, arena)?
    else {
        return Ok(spend);
    };
    if !zset_contains_hash(allowed_hashes, arena, pkh_digest)? {
        return Ok(spend);
    }

    let msg5 = spend_v1_sig_hash(body_noun, arena)?;
    let hash = crate::Hash { values: msg5 };
    let (chal, sig) = crate::schnorr_sign_tx(cfg.sk_be, pk_coords, hash.values);

    let chal_elems = [
        arena.alloc_atom_u64(chal.values[0]),
        arena.alloc_atom_u64(chal.values[1]),
        arena.alloc_atom_u64(chal.values[2]),
        arena.alloc_atom_u64(chal.values[3]),
        arena.alloc_atom_u64(chal.values[4]),
        arena.alloc_atom_u64(chal.values[5]),
        arena.alloc_atom_u64(chal.values[6]),
        arena.alloc_atom_u64(chal.values[7]),
    ];
    let chal_noun = build_tuple(arena, &chal_elems);

    let sig_elems = [
        arena.alloc_atom_u64(sig.values[0]),
        arena.alloc_atom_u64(sig.values[1]),
        arena.alloc_atom_u64(sig.values[2]),
        arena.alloc_atom_u64(sig.values[3]),
        arena.alloc_atom_u64(sig.values[4]),
        arena.alloc_atom_u64(sig.values[5]),
        arena.alloc_atom_u64(sig.values[6]),
        arena.alloc_atom_u64(sig.values[7]),
    ];
    let sig_noun = build_tuple(arena, &sig_elems);

    // SchnorrSignature = [chal sig]
    let schnorr_sig = build_tuple(arena, &[chal_noun, sig_noun]);
    // PkhSignatureValue = [pk sig]
    let value_noun = build_tuple(arena, &[pk_noun, schnorr_sig]);

    // If our key is already present (including a fake placeholder signature used for fee sizing),
    // overwrite it unconditionally. Otherwise, ensure we don't exceed m signatures by evicting a
    // placeholder entry when the map is already full.
    let has_ours = zmap_contains_hash(pkh_map, arena, pkh_digest)?;
    let mut pkh_map_for_signing = pkh_map;
    if !has_ours {
        loop {
            let have = zmap_count_up_to(pkh_map_for_signing, arena, m_required.saturating_add(1))?;
            if have < m_required {
                break;
            }
            let Some(key) = zmap_find_replaceable_key(pkh_map_for_signing, arena)? else {
                return Ok(spend);
            };
            pkh_map_for_signing = zmap_remove_key(arena, pkh_map_for_signing, key)?;
        }
    }
    let new_pkh_map =
        zmap::canonical_zmap_put(arena, pkh_map_for_signing, pkh_key_noun, value_noun)?;
    let new_witness = build_tuple(arena, &[lmp, new_pkh_map, hax, tim]);
    let new_body = build_tuple(arena, &[new_witness, seeds, fee]);
    Ok(build_tuple(arena, &[ver_noun, new_body]))
}

fn sign_spends_map(
    arena: &mut Arena,
    spends: Noun,
    pk_noun: Noun,
    pkh_key_noun: Noun,
    pkh_digest: [u64; 5],
    cfg: &SignerConfig,
    pk_coords: ([u64; 6], [u64; 6]),
) -> Result<Noun, SignDraftError> {
    if spends == arena.atom0() {
        return Ok(spends);
    }

    let (node, left, right) = decompose_map(spends, arena).ok_or(SignDraftError::Malformed)?;
    let (key, spend) = decompose_pair(node, arena).ok_or(SignDraftError::Malformed)?;

    let new_left = sign_spends_map(
        arena,
        left,
        pk_noun,
        pkh_key_noun,
        pkh_digest,
        cfg,
        pk_coords,
    )?;
    let new_right = sign_spends_map(
        arena,
        right,
        pk_noun,
        pkh_key_noun,
        pkh_digest,
        cfg,
        pk_coords,
    )?;
    let new_spend = sign_spend_v1(
        arena,
        spend,
        pk_noun,
        pkh_key_noun,
        pkh_digest,
        cfg,
        pk_coords,
    )?;

    let new_node = build_tuple(arena, &[key, new_spend]);
    Ok(build_tuple(arena, &[new_node, new_left, new_right]))
}

fn spend_condition_pkh_lock(
    spend_condition: Noun,
    arena: &Arena,
) -> Result<Option<(u64, Noun)>, SignDraftError> {
    if spend_condition == arena.atom0() {
        return Ok(None);
    }
    let (head, tail) = uncons(spend_condition, arena).ok_or(SignDraftError::Malformed)?;
    let (header, body) = tuple2(head, arena).ok_or(SignDraftError::Malformed)?;
    if atom_eq_bytes(header, b"pkh", arena) {
        let (m, h_set) = tuple2(body, arena).ok_or(SignDraftError::Malformed)?;
        let m_u64 = noun_atom_u64(m, arena).ok_or(SignDraftError::Malformed)?;
        if m_u64 == 0 {
            return Err(SignDraftError::Malformed);
        }
        return Ok(Some((m_u64, h_set)));
    }
    spend_condition_pkh_lock(tail, arena)
}

fn zset_contains_hash(set: Noun, arena: &Arena, want: [u64; 5]) -> Result<bool, SignDraftError> {
    if set == arena.atom0() {
        return Ok(false);
    }
    let (value, left, right) = tuple3(set, arena).ok_or(SignDraftError::Malformed)?;
    let digest = parse_hash(value, arena).ok_or(SignDraftError::Malformed)?;
    if digest == want {
        return Ok(true);
    }
    Ok(zset_contains_hash(left, arena, want)? || zset_contains_hash(right, arena, want)?)
}

fn zmap_contains_hash(map: Noun, arena: &Arena, want: [u64; 5]) -> Result<bool, SignDraftError> {
    if map == arena.atom0() {
        return Ok(false);
    }
    let (node, left, right) = decompose_map(map, arena).ok_or(SignDraftError::Malformed)?;
    let (key, _value) = decompose_pair(node, arena).ok_or(SignDraftError::Malformed)?;
    let here = parse_hash(key, arena).is_some_and(|digest| digest == want);
    Ok(here || zmap_contains_hash(left, arena, want)? || zmap_contains_hash(right, arena, want)?)
}

fn zmap_count_up_to(map: Noun, arena: &Arena, limit: u64) -> Result<u64, SignDraftError> {
    if limit == 0 || map == arena.atom0() {
        return Ok(0);
    }
    let (_node, left, right) = decompose_map(map, arena).ok_or(SignDraftError::Malformed)?;

    let mut count = 1u64;
    if count >= limit {
        return Ok(count);
    }
    count = count.saturating_add(zmap_count_up_to(left, arena, limit - count)?);
    if count >= limit {
        return Ok(count);
    }
    count = count.saturating_add(zmap_count_up_to(right, arena, limit - count)?);
    Ok(count)
}

fn tuple_all_u64_eq(
    noun: Noun,
    arena: &Arena,
    count: usize,
    want: u64,
) -> Result<bool, SignDraftError> {
    if count == 0 {
        return Ok(noun == arena.atom0());
    }
    let mut cur = noun;
    for _ in 0..count.saturating_sub(1) {
        let (head, tail) = uncons(cur, arena).ok_or(SignDraftError::Malformed)?;
        let v = noun_atom_u64(head, arena).ok_or(SignDraftError::Malformed)?;
        if v != want {
            return Ok(false);
        }
        cur = tail;
    }
    let v = noun_atom_u64(cur, arena).ok_or(SignDraftError::Malformed)?;
    Ok(v == want)
}

fn is_placeholder_pkh_signature_value(value: Noun, arena: &Arena) -> Result<bool, SignDraftError> {
    let Some((pk, sig)) = tuple2(value, arena) else {
        return Ok(true);
    };
    let Some((x, y, _inf)) = tuple3(pk, arena) else {
        return Ok(true);
    };
    if !tuple_all_u64_eq(x, arena, 6, 0)? {
        return Ok(false);
    }
    if !tuple_all_u64_eq(y, arena, 6, 0)? {
        return Ok(false);
    }
    let Some((chal, sig_s)) = tuple2(sig, arena) else {
        return Ok(true);
    };
    if !tuple_all_u64_eq(chal, arena, 8, 0)? {
        return Ok(false);
    }
    if !tuple_all_u64_eq(sig_s, arena, 8, 0)? {
        return Ok(false);
    }
    Ok(true)
}

fn zmap_find_replaceable_key(map: Noun, arena: &Arena) -> Result<Option<Noun>, SignDraftError> {
    if map == arena.atom0() {
        return Ok(None);
    }

    let (node, left, right) = decompose_map(map, arena).ok_or(SignDraftError::Malformed)?;
    let (key, value) = decompose_pair(node, arena).ok_or(SignDraftError::Malformed)?;

    if parse_hash(key, arena).is_none() || is_placeholder_pkh_signature_value(value, arena)? {
        return Ok(Some(key));
    }

    if let Some(k) = zmap_find_replaceable_key(left, arena)? {
        return Ok(Some(k));
    }
    zmap_find_replaceable_key(right, arena)
}

fn zmap_remove_key(
    arena: &mut Arena,
    map: Noun,
    key_to_remove: Noun,
) -> Result<Noun, SignDraftError> {
    if map == arena.atom0() {
        return Ok(map);
    }

    let mut out = arena.atom0();
    zmap_rebuild_skipping_key(arena, map, key_to_remove, &mut out)?;
    Ok(out)
}

fn zmap_rebuild_skipping_key(
    arena: &mut Arena,
    map: Noun,
    key_to_remove: Noun,
    out: &mut Noun,
) -> Result<(), SignDraftError> {
    if map == arena.atom0() {
        return Ok(());
    }
    let (node, left, right) = decompose_map(map, arena).ok_or(SignDraftError::Malformed)?;
    let (key, value) = decompose_pair(node, arena).ok_or(SignDraftError::Malformed)?;
    if key != key_to_remove {
        *out = zmap::canonical_zmap_put(arena, *out, key, value)?;
    }
    zmap_rebuild_skipping_key(arena, left, key_to_remove, out)?;
    zmap_rebuild_skipping_key(arena, right, key_to_remove, out)?;
    Ok(())
}

struct TxIdCtx {
    null_digest: [u64; 5],
    fake_digest: [u64; 5],
    version_digest: [u64; 5],
}

fn tx_id_ctx(arena: &mut Arena) -> Result<TxIdCtx, SignDraftError> {
    let null_digest = tip5::hash_noun_varlen(arena.atom0(), arena)?;
    let fake_atom = arena.alloc_atom_bytes(b"fake");
    let fake_digest = tip5::hash_noun_varlen(fake_atom, arena)?;
    let version_atom = arena.alloc_atom_u64(1);
    let version_digest = tip5::hash_noun_varlen(version_atom, arena)?;
    Ok(TxIdCtx {
        null_digest,
        fake_digest,
        version_digest,
    })
}

fn mul_add_limbs_le(acc: &mut Vec<u64>, mul: u64, add: u64) {
    let mut carry = add as u128;
    for limb in acc.iter_mut() {
        let prod = (*limb as u128) * (mul as u128) + carry;
        *limb = prod as u64;
        carry = prod >> 64;
    }
    if carry != 0 {
        acc.push(carry as u64);
    }
}

fn digest_to_b58(digest: [u64; 5]) -> String {
    let digits = [digest[4], digest[3], digest[2], digest[1], digest[0]];
    let mut acc: Vec<u64> = Vec::new();
    acc.push(0);
    for digit in digits {
        mul_add_limbs_le(&mut acc, tip5::GOLDILOCKS_P, digit);
    }

    while acc.len() > 1 && acc.last() == Some(&0) {
        acc.pop();
    }

    let mut bytes_be: Vec<u8> = Vec::new();
    let mut it = acc.iter().rev();
    if let Some(&ms) = it.next() {
        let ms_be = ms.to_be_bytes();
        let first_non_zero = ms_be.iter().position(|&b| b != 0).unwrap_or(ms_be.len());
        bytes_be.extend_from_slice(&ms_be[first_non_zero..]);
        for &limb in it {
            bytes_be.extend_from_slice(&limb.to_be_bytes());
        }
    }

    bs58::encode(bytes_be).into_string()
}

fn cheetah_pubkey_noun(arena: &mut Arena, pk_coords: ([u64; 6], [u64; 6])) -> Noun {
    let x_elems = [
        arena.alloc_atom_u64(pk_coords.0[0]),
        arena.alloc_atom_u64(pk_coords.0[1]),
        arena.alloc_atom_u64(pk_coords.0[2]),
        arena.alloc_atom_u64(pk_coords.0[3]),
        arena.alloc_atom_u64(pk_coords.0[4]),
        arena.alloc_atom_u64(pk_coords.0[5]),
    ];
    let x_noun = build_tuple(arena, &x_elems);

    let y_elems = [
        arena.alloc_atom_u64(pk_coords.1[0]),
        arena.alloc_atom_u64(pk_coords.1[1]),
        arena.alloc_atom_u64(pk_coords.1[2]),
        arena.alloc_atom_u64(pk_coords.1[3]),
        arena.alloc_atom_u64(pk_coords.1[4]),
        arena.alloc_atom_u64(pk_coords.1[5]),
    ];
    let y_noun = build_tuple(arena, &y_elems);

    let inf_noun = arena.alloc_atom_u64(1);
    build_tuple(arena, &[x_noun, y_noun, inf_noun])
}

pub fn cheetah_pubkey_pkh_v1(pk_coords: ([u64; 6], [u64; 6])) -> Result<String, SignDraftError> {
    let mut arena = Arena::new();
    let pk_noun = cheetah_pubkey_noun(&mut arena, pk_coords);
    let pkh_digest = tip5::hash_noun_varlen(pk_noun, &arena)?;
    Ok(digest_to_b58(pkh_digest))
}

fn hash_nname_hashable(noun: Noun, arena: &Arena) -> Result<[u64; 5], SignDraftError> {
    let (first_noun, rest) = uncons(noun, arena).ok_or(SignDraftError::Malformed)?;
    let (second_noun, end) = uncons(rest, arena).ok_or(SignDraftError::Malformed)?;
    let first = parse_hash(first_noun, arena).ok_or(SignDraftError::Malformed)?;
    let second = parse_hash(second_noun, arena).ok_or(SignDraftError::Malformed)?;
    let end_digest = tip5::hash_noun_varlen(end, arena)?;
    let tail = tip5::hash_ten_cell(second, end_digest)?;
    Ok(tip5::hash_ten_cell(first, tail)?)
}

fn hash_zset_hashes(set: Noun, arena: &Arena, ctx: &TxIdCtx) -> Result<[u64; 5], SignDraftError> {
    if set == arena.atom0() {
        return Ok(ctx.null_digest);
    }
    let (value, left, right) = tuple3(set, arena).ok_or(SignDraftError::Malformed)?;
    let value_digest = parse_hash(value, arena).ok_or(SignDraftError::Malformed)?;
    let left_digest = hash_zset_hashes(left, arena, ctx)?;
    let right_digest = hash_zset_hashes(right, arena, ctx)?;
    let children_digest = tip5::hash_ten_cell(left_digest, right_digest)?;
    Ok(tip5::hash_ten_cell(value_digest, children_digest)?)
}

fn hash_optional_leaf(opt: Noun, arena: &Arena, ctx: &TxIdCtx) -> Result<[u64; 5], SignDraftError> {
    if opt == arena.atom0() {
        return Ok(ctx.null_digest);
    }
    let (tag, value) = tuple2(opt, arena).ok_or(SignDraftError::Malformed)?;
    if tag != arena.atom0() {
        return Err(SignDraftError::Malformed);
    }
    let value_digest = tip5::hash_noun_varlen(value, arena)?;
    Ok(tip5::hash_ten_cell(ctx.null_digest, value_digest)?)
}

fn hash_timelock_range(
    range: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], SignDraftError> {
    let (min, max) = tuple2(range, arena).ok_or(SignDraftError::Malformed)?;
    let min_digest = hash_optional_leaf(min, arena, ctx)?;
    let max_digest = hash_optional_leaf(max, arena, ctx)?;
    Ok(tip5::hash_ten_cell(min_digest, max_digest)?)
}

fn hash_lock_primitive_hashable(
    lp: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], SignDraftError> {
    let (header, body) = tuple2(lp, arena).ok_or(SignDraftError::Malformed)?;
    let Noun::Atom(header_id) = header else {
        return Err(SignDraftError::Malformed);
    };

    if arena.atom_eq_bytes(header_id, b"pkh") {
        let (m, h) = tuple2(body, arena).ok_or(SignDraftError::Malformed)?;
        let tag_digest = tip5::hash_noun_varlen(header, arena)?;
        let m_digest = tip5::hash_noun_varlen(m, arena)?;
        let h_digest = hash_zset_hashes(h, arena, ctx)?;
        let inner = tip5::hash_ten_cell(m_digest, h_digest)?;
        return Ok(tip5::hash_ten_cell(tag_digest, inner)?);
    }

    if arena.atom_eq_bytes(header_id, b"tim") {
        let (rel, abs) = tuple2(body, arena).ok_or(SignDraftError::Malformed)?;
        let tag_digest = tip5::hash_noun_varlen(header, arena)?;
        let rel_digest = hash_timelock_range(rel, arena, ctx)?;
        let abs_digest = hash_timelock_range(abs, arena, ctx)?;
        let inner = tip5::hash_ten_cell(rel_digest, abs_digest)?;
        return Ok(tip5::hash_ten_cell(tag_digest, inner)?);
    }

    if arena.atom_eq_bytes(header_id, b"hax") {
        let tag_digest = tip5::hash_noun_varlen(header, arena)?;
        return Ok(tip5::hash_ten_cell(tag_digest, ctx.fake_digest)?);
    }

    if arena.atom_eq_bytes(header_id, b"brn") {
        let tag_digest = tip5::hash_noun_varlen(header, arena)?;
        return Ok(tip5::hash_ten_cell(tag_digest, ctx.null_digest)?);
    }

    Err(SignDraftError::Unsupported)
}

fn hash_lock_primitives_list(
    prims: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], SignDraftError> {
    if prims == arena.atom0() {
        return Ok(ctx.null_digest);
    }
    let (head, tail) = uncons(prims, arena).ok_or(SignDraftError::Malformed)?;
    let head_digest = hash_lock_primitive_hashable(head, arena, ctx)?;
    let tail_digest = hash_lock_primitives_list(tail, arena, ctx)?;
    Ok(tip5::hash_ten_cell(head_digest, tail_digest)?)
}

fn hash_merkle_proof_hashable(
    merk: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], SignDraftError> {
    let (root_noun, path) = tuple2(merk, arena).ok_or(SignDraftError::Malformed)?;
    let root = parse_hash(root_noun, arena).ok_or(SignDraftError::Malformed)?;
    let path_digest = hash_hash_list_hashes(path, arena, ctx)?;
    Ok(tip5::hash_ten_cell(root, path_digest)?)
}

fn hash_hash_list_hashes(
    list: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], SignDraftError> {
    if list == arena.atom0() {
        return Ok(ctx.null_digest);
    }
    let (head, tail) = uncons(list, arena).ok_or(SignDraftError::Malformed)?;
    let head_digest = parse_hash(head, arena).ok_or(SignDraftError::Malformed)?;
    let tail_digest = hash_hash_list_hashes(tail, arena, ctx)?;
    Ok(tip5::hash_ten_cell(head_digest, tail_digest)?)
}

fn hash_lock_merkle_proof_hashable(
    lmp: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], SignDraftError> {
    let (version, spend_condition, axis, merk_proof) = decompose_lock_merkle_proof(lmp, arena)?;
    let spend_condition_hash = hash_lock_primitives_list(spend_condition, arena, ctx)?;
    let merk_digest = hash_merkle_proof_hashable(merk_proof, arena, ctx)?;

    match version {
        Some(version) => {
            let version_digest = tip5::hash_noun_varlen(version, arena)?;
            let axis_digest = tip5::hash_noun_varlen(axis, arena)?;
            let inner = tip5::hash_ten_cell(axis_digest, merk_digest)?;
            let inner = tip5::hash_ten_cell(spend_condition_hash, inner)?;
            Ok(tip5::hash_ten_cell(version_digest, inner)?)
        }
        None => {
            let axis_digest = LOCK_MERKLE_AXIS_HASH;
            let inner = tip5::hash_ten_cell(axis_digest, merk_digest)?;
            Ok(tip5::hash_ten_cell(spend_condition_hash, inner)?)
        }
    }
}

fn hash_hax_map_hashable(
    map: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], SignDraftError> {
    if map == arena.atom0() {
        return Ok(ctx.null_digest);
    }
    let (node, left, right) = decompose_map(map, arena).ok_or(SignDraftError::Malformed)?;
    let (key, value) = decompose_pair(node, arena).ok_or(SignDraftError::Malformed)?;

    let key_digest = parse_hash(key, arena).ok_or(SignDraftError::Malformed)?;
    let value_digest = hashable_noun_digest(value, arena)?;
    let node_digest = tip5::hash_ten_cell(key_digest, value_digest)?;

    let left_digest = hash_hax_map_hashable(left, arena, ctx)?;
    let right_digest = hash_hax_map_hashable(right, arena, ctx)?;
    let children_digest = tip5::hash_ten_cell(left_digest, right_digest)?;
    Ok(tip5::hash_ten_cell(node_digest, children_digest)?)
}

fn hash_pkh_signature_value_hashable(
    value: Noun,
    arena: &Arena,
) -> Result<[u64; 5], SignDraftError> {
    let (pk, sig) = tuple2(value, arena).ok_or(SignDraftError::Malformed)?;
    let pk_digest = tip5::hash_noun_varlen(pk, arena)?;
    let sig_digest = tip5::hash_noun_varlen(sig, arena)?;
    Ok(tip5::hash_ten_cell(pk_digest, sig_digest)?)
}

fn hash_pkh_signature_map_hashable(
    map: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], SignDraftError> {
    if map == arena.atom0() {
        return Ok(ctx.null_digest);
    }
    let (node, left, right) = decompose_map(map, arena).ok_or(SignDraftError::Malformed)?;
    let (key, value) = decompose_pair(node, arena).ok_or(SignDraftError::Malformed)?;

    let key_digest = parse_hash(key, arena).ok_or(SignDraftError::Malformed)?;
    let value_digest = hash_pkh_signature_value_hashable(value, arena)?;
    let node_digest = tip5::hash_ten_cell(key_digest, value_digest)?;

    let left_digest = hash_pkh_signature_map_hashable(left, arena, ctx)?;
    let right_digest = hash_pkh_signature_map_hashable(right, arena, ctx)?;
    let children_digest = tip5::hash_ten_cell(left_digest, right_digest)?;
    Ok(tip5::hash_ten_cell(node_digest, children_digest)?)
}

fn hash_seed_regular_hashable(seed: Noun, arena: &Arena) -> Result<[u64; 5], SignDraftError> {
    let (_output_source, lock_root_noun, note_data_noun, gift_noun, parent_hash_noun) =
        tuple5(seed, arena).ok_or(SignDraftError::Malformed)?;

    let lock_root = parse_hash(lock_root_noun, arena).ok_or(SignDraftError::Malformed)?;
    let note_data_hash = hash_note_data(note_data_noun, arena)?;
    let gift_digest = tip5::hash_noun_varlen(gift_noun, arena)?;
    let parent_hash = parse_hash(parent_hash_noun, arena).ok_or(SignDraftError::Malformed)?;

    let mut acc = parent_hash;
    acc = tip5::hash_ten_cell(gift_digest, acc)?;
    acc = tip5::hash_ten_cell(note_data_hash, acc)?;
    acc = tip5::hash_ten_cell(lock_root, acc)?;
    Ok(acc)
}

fn hash_seeds_regular(
    seeds_zset: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], SignDraftError> {
    if seeds_zset == arena.atom0() {
        return Ok(ctx.null_digest);
    }

    let (seed, lr) = uncons(seeds_zset, arena).ok_or(SignDraftError::Malformed)?;
    let (left, right) = uncons(lr, arena).ok_or(SignDraftError::Malformed)?;

    let node_digest = hash_seed_regular_hashable(seed, arena)?;
    let left_digest = hash_seeds_regular(left, arena, ctx)?;
    let right_digest = hash_seeds_regular(right, arena, ctx)?;
    let children_digest = tip5::hash_ten_cell(left_digest, right_digest)?;
    Ok(tip5::hash_ten_cell(node_digest, children_digest)?)
}

fn hash_witness_hashable(
    witness: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], SignDraftError> {
    let (lmp, pkh_map, hax, tim) = tuple4(witness, arena).ok_or(SignDraftError::Malformed)?;

    let lmp_hash = hash_lock_merkle_proof_hashable(lmp, arena, ctx)?;
    let pkh_hash = hash_pkh_signature_map_hashable(pkh_map, arena, ctx)?;
    let hax_hash = hash_hax_map_hashable(hax, arena, ctx)?;
    let tim_digest = tip5::hash_noun_varlen(tim, arena)?;

    let mut acc = tim_digest;
    acc = tip5::hash_ten_cell(hax_hash, acc)?;
    acc = tip5::hash_ten_cell(pkh_hash, acc)?;
    acc = tip5::hash_ten_cell(lmp_hash, acc)?;
    Ok(acc)
}

fn hash_spend_v1_hashable(
    spend: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], SignDraftError> {
    let (ver_noun, body_noun) = tuple2(spend, arena).ok_or(SignDraftError::Malformed)?;
    if noun_atom_u64(ver_noun, arena) != Some(1) {
        return Err(SignDraftError::Unsupported);
    }

    let (witness, seeds, fee) = tuple3(body_noun, arena).ok_or(SignDraftError::Malformed)?;
    let witness_digest = hash_witness_hashable(witness, arena, ctx)?;
    let seeds_digest = hash_seeds_regular(seeds, arena, ctx)?;
    let fee_digest = tip5::hash_noun_varlen(fee, arena)?;

    let inner = tip5::hash_ten_cell(seeds_digest, fee_digest)?;
    let body_digest = tip5::hash_ten_cell(witness_digest, inner)?;
    let ver_digest = tip5::hash_noun_varlen(ver_noun, arena)?;
    Ok(tip5::hash_ten_cell(ver_digest, body_digest)?)
}

fn hash_spends_hashable(
    spends: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], SignDraftError> {
    if spends == arena.atom0() {
        return Ok(ctx.null_digest);
    }

    let (node, left, right) = decompose_map(spends, arena).ok_or(SignDraftError::Malformed)?;
    let (name_noun, spend_noun) = decompose_pair(node, arena).ok_or(SignDraftError::Malformed)?;

    let name_digest = hash_nname_hashable(name_noun, arena)?;
    let spend_digest = hash_spend_v1_hashable(spend_noun, arena, ctx)?;
    let node_digest = tip5::hash_ten_cell(name_digest, spend_digest)?;

    let left_digest = hash_spends_hashable(left, arena, ctx)?;
    let right_digest = hash_spends_hashable(right, arena, ctx)?;
    let children_digest = tip5::hash_ten_cell(left_digest, right_digest)?;
    Ok(tip5::hash_ten_cell(node_digest, children_digest)?)
}

fn compute_tx_id_v1(
    spends: Noun,
    arena: &Arena,
    ctx: &TxIdCtx,
) -> Result<[u64; 5], SignDraftError> {
    let spends_digest = hash_spends_hashable(spends, arena, ctx)?;
    Ok(tip5::hash_ten_cell(ctx.version_digest, spends_digest)?)
}

fn looks_like_spends_v1_map(spends: Noun, arena: &Arena) -> bool {
    if spends == arena.atom0() {
        return true;
    }
    let Some((node, _left, _right)) = decompose_map(spends, arena) else {
        return false;
    };
    let Some((_key, spend)) = decompose_pair(node, arena) else {
        return false;
    };
    let Some((spend_ver, _body)) = tuple2(spend, arena) else {
        return false;
    };
    noun_atom_u64(spend_ver, arena) == Some(1)
}

fn atom_eq_bytes(noun: Noun, bytes: &[u8], arena: &Arena) -> bool {
    match noun {
        Noun::Atom(id) => arena.atom_eq_bytes(id, bytes),
        _ => false,
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

fn zset_any_value(set: Noun, arena: &Arena) -> Option<Noun> {
    if set == arena.atom0() {
        return None;
    }
    let (value, _lr) = uncons(set, arena)?;
    Some(value)
}

fn zset_count_up_to(set: Noun, arena: &Arena, limit: u8) -> Result<u8, SignDraftError> {
    if limit == 0 || set == arena.atom0() {
        return Ok(0);
    }
    let (value, lr) = uncons(set, arena).ok_or(SignDraftError::Malformed)?;
    let _ = value;

    let mut count = 1u8;
    if count >= limit {
        return Ok(count);
    }

    let (left, right) = uncons(lr, arena).ok_or(SignDraftError::Malformed)?;
    count = count.saturating_add(zset_count_up_to(left, arena, limit - count)?);
    if count >= limit {
        return Ok(count);
    }
    count = count.saturating_add(zset_count_up_to(right, arena, limit - count)?);
    Ok(count)
}

fn seed_recipient_pkh(
    seed_note_data: Noun,
    arena: &Arena,
) -> Result<Option<[u64; 5]>, SignDraftError> {
    // note-data is a z-map of @tas -> *
    // We expect key "lock" to be lock-data: [%0 spend-condition]
    let Some(lock_data) = note_data_find(seed_note_data, arena, b"lock") else {
        return Ok(None);
    };

    let (ver, lock) = tuple2(lock_data, arena).ok_or(SignDraftError::Malformed)?;
    if noun_atom_u64(ver, arena) != Some(0) {
        return Ok(None);
    }

    // spend-condition is a list of lock-primitives; we only support a single %pkh primitive.
    let (prim, _rest) = uncons(lock, arena).ok_or(SignDraftError::Malformed)?;
    let (header, body) = tuple2(prim, arena).ok_or(SignDraftError::Malformed)?;
    if !atom_eq_bytes(header, b"pkh", arena) {
        return Ok(None);
    }

    // body for pkh: [m h=(z-set hash)]
    let (m, h_set) = tuple2(body, arena).ok_or(SignDraftError::Malformed)?;
    let m_u64 = noun_atom_u64(m, arena).ok_or(SignDraftError::Malformed)?;
    if m_u64 == 0 {
        return Err(SignDraftError::Malformed);
    }

    // If multiple recipient hashes exist, fall back to lock-root instead of picking an arbitrary one.
    if zset_count_up_to(h_set, arena, 2)? != 1 {
        return Ok(None);
    }

    let any = zset_any_value(h_set, arena).ok_or(SignDraftError::Malformed)?;
    let digest = parse_hash(any, arena).ok_or(SignDraftError::Malformed)?;
    Ok(Some(digest))
}

fn collect_outputs_from_seeds(
    seeds_zset: Noun,
    arena: &Arena,
    signer_pkh: [u64; 5],
    acc: &mut Vec<([u64; 5], u64)>,
    refund: &mut Option<u64>,
) -> Result<(), SignDraftError> {
    if seeds_zset == arena.atom0() {
        return Ok(());
    }

    let (seed, lr) = uncons(seeds_zset, arena).ok_or(SignDraftError::Malformed)?;
    let (left, right) = uncons(lr, arena).ok_or(SignDraftError::Malformed)?;

    let (_output_source, _lock_root, note_data, gift_noun, _parent_hash) =
        tuple5(seed, arena).ok_or(SignDraftError::Malformed)?;
    let gift = match gift_noun {
        Noun::Atom(id) => arena.atom_u64(id).ok_or(SignDraftError::Malformed)?,
        _ => return Err(SignDraftError::Malformed),
    };
    if gift != 0 {
        let lock_root_digest = parse_hash(_lock_root, arena).ok_or(SignDraftError::Malformed)?;
        let recipient = seed_recipient_pkh(note_data, arena)?.unwrap_or(lock_root_digest);
        if recipient == signer_pkh {
            let next = refund
                .unwrap_or(0)
                .checked_add(gift)
                .ok_or(SignDraftError::Malformed)?;
            *refund = Some(next);
        } else if let Some(existing) = acc.iter_mut().find(|(d, _)| *d == recipient) {
            existing.1 = existing
                .1
                .checked_add(gift)
                .ok_or(SignDraftError::Malformed)?;
        } else {
            acc.push((recipient, gift));
        }
    }

    collect_outputs_from_seeds(left, arena, signer_pkh, acc, refund)?;
    collect_outputs_from_seeds(right, arena, signer_pkh, acc, refund)?;
    Ok(())
}

fn collect_outputs_from_spends(
    spends: Noun,
    arena: &Arena,
    signer_pkh: [u64; 5],
    acc: &mut Vec<([u64; 5], u64)>,
    refund: &mut Option<u64>,
    input_count: &mut u32,
    fee_total: &mut u64,
) -> Result<(), SignDraftError> {
    if spends == arena.atom0() {
        return Ok(());
    }

    let (node, left, right) = decompose_map(spends, arena).ok_or(SignDraftError::Malformed)?;
    let (_name, spend) = decompose_pair(node, arena).ok_or(SignDraftError::Malformed)?;
    let (ver, body) = tuple2(spend, arena).ok_or(SignDraftError::Malformed)?;
    if noun_atom_u64(ver, arena) != Some(1) {
        return Err(SignDraftError::Unsupported);
    }
    let (_witness, seeds, fee) = tuple3(body, arena).ok_or(SignDraftError::Malformed)?;
    let fee = noun_atom_u64(fee, arena).ok_or(SignDraftError::Malformed)?;
    *input_count = input_count
        .checked_add(1)
        .ok_or(SignDraftError::Malformed)?;
    *fee_total = fee_total
        .checked_add(fee)
        .ok_or(SignDraftError::Malformed)?;
    collect_outputs_from_seeds(seeds, arena, signer_pkh, acc, refund)?;

    collect_outputs_from_spends(left, arena, signer_pkh, acc, refund, input_count, fee_total)?;
    collect_outputs_from_spends(
        right,
        arena,
        signer_pkh,
        acc,
        refund,
        input_count,
        fee_total,
    )?;
    Ok(())
}

pub fn draft_outputs_v1(
    draft_jam: &[u8],
    cfg: &SignerConfig,
) -> Result<Vec<DraftOutputV1>, SignDraftError> {
    Ok(draft_review_v1(draft_jam, cfg)?.outputs)
}

pub fn draft_review_v1(
    draft_jam: &[u8],
    cfg: &SignerConfig,
) -> Result<DraftReviewV1, SignDraftError> {
    let pk_coords = cheetah_pub_from_sk_tuple(cfg.sk_be);

    let mut arena = Arena::new();
    let root = cue(draft_jam, &mut arena)?;

    // Build pubkey noun: [x y inf]
    let x_elems = [
        arena.alloc_atom_u64(pk_coords.0[0]),
        arena.alloc_atom_u64(pk_coords.0[1]),
        arena.alloc_atom_u64(pk_coords.0[2]),
        arena.alloc_atom_u64(pk_coords.0[3]),
        arena.alloc_atom_u64(pk_coords.0[4]),
        arena.alloc_atom_u64(pk_coords.0[5]),
    ];
    let x_noun = build_tuple(&mut arena, &x_elems);

    let y_elems = [
        arena.alloc_atom_u64(pk_coords.1[0]),
        arena.alloc_atom_u64(pk_coords.1[1]),
        arena.alloc_atom_u64(pk_coords.1[2]),
        arena.alloc_atom_u64(pk_coords.1[3]),
        arena.alloc_atom_u64(pk_coords.1[4]),
        arena.alloc_atom_u64(pk_coords.1[5]),
    ];
    let y_noun = build_tuple(&mut arena, &y_elems);

    let inf_noun = arena.alloc_atom_u64(1);
    let pk_noun = build_tuple(&mut arena, &[x_noun, y_noun, inf_noun]);
    let signer_pkh = tip5::hash_noun_varlen(pk_noun, &arena)?;

    // Detect outer wrapper (same shapes as `sign_draft_v1`).
    enum Outer {
        Spends { spends: Noun },
    }

    let outer = if let Some((ver, id, spends)) = tuple3(root, &arena) {
        if noun_atom_u64(ver, &arena) == Some(1) && parse_hash(id, &arena).is_some() {
            Some(Outer::Spends { spends })
        } else {
            None
        }
    } else {
        None
    };

    let spends = if let Some(Outer::Spends { spends }) = outer {
        spends
    } else if let Some((head, tail)) = uncons(root, &arena) {
        if let Some((ver, id, spends)) = tuple3(head, &arena) {
            if noun_atom_u64(ver, &arena) == Some(1) && parse_hash(id, &arena).is_some() {
                spends
            } else {
                return Err(SignDraftError::Unsupported);
            }
        } else if matches!(head, Noun::Atom(_)) {
            if noun_atom_u64(head, &arena) == Some(1) {
                let (_name, spends, _display, _witness_data) =
                    tuple4(tail, &arena).ok_or(SignDraftError::Malformed)?;
                spends
            } else {
                tail
            }
        } else {
            return Err(SignDraftError::Malformed);
        }
    } else {
        return Err(SignDraftError::Malformed);
    };

    let minimum_fee = calculate_minimum_fee_v1(&mut arena, spends)?;

    let mut acc: Vec<([u64; 5], u64)> = Vec::new();
    let mut refund: Option<u64> = None;
    let mut input_count = 0u32;
    let mut fee_total = 0u64;
    collect_outputs_from_spends(
        spends,
        &arena,
        signer_pkh,
        &mut acc,
        &mut refund,
        &mut input_count,
        &mut fee_total,
    )?;

    let mut out: Vec<DraftOutputV1> = Vec::with_capacity(acc.len() + 1);
    let mut external_total = 0u64;
    let external_output_count = acc.len() as u32;
    for (digest, gift) in acc {
        external_total = external_total
            .checked_add(gift)
            .ok_or(SignDraftError::Malformed)?;
        out.push(DraftOutputV1 {
            recipient_b58: digest_to_b58(digest),
            gift,
            is_refund: false,
        });
    }
    let refund_total = refund.unwrap_or(0);
    if refund_total != 0 {
        out.push(DraftOutputV1 {
            recipient_b58: digest_to_b58(signer_pkh),
            gift: refund_total,
            is_refund: true,
        });
    }
    Ok(DraftReviewV1 {
        outputs: out,
        input_count,
        external_output_count,
        external_total,
        refund_total,
        fee_total,
        minimum_fee,
    })
}

pub fn sign_draft_v1(draft_jam: &[u8], cfg: &SignerConfig) -> Result<Vec<u8>, SignDraftError> {
    let pk_coords = cheetah_pub_from_sk_tuple(cfg.sk_be);

    let mut arena = Arena::new();
    let root = cue(draft_jam, &mut arena)?;

    // Build pubkey noun: [x y inf]
    let x_elems = [
        arena.alloc_atom_u64(pk_coords.0[0]),
        arena.alloc_atom_u64(pk_coords.0[1]),
        arena.alloc_atom_u64(pk_coords.0[2]),
        arena.alloc_atom_u64(pk_coords.0[3]),
        arena.alloc_atom_u64(pk_coords.0[4]),
        arena.alloc_atom_u64(pk_coords.0[5]),
    ];
    let x_noun = build_tuple(&mut arena, &x_elems);

    let y_elems = [
        arena.alloc_atom_u64(pk_coords.1[0]),
        arena.alloc_atom_u64(pk_coords.1[1]),
        arena.alloc_atom_u64(pk_coords.1[2]),
        arena.alloc_atom_u64(pk_coords.1[3]),
        arena.alloc_atom_u64(pk_coords.1[4]),
        arena.alloc_atom_u64(pk_coords.1[5]),
    ];
    let y_noun = build_tuple(&mut arena, &y_elems);

    let inf_noun = arena.alloc_atom_u64(1);
    let pk_tuple = [x_noun, y_noun, inf_noun];
    let pk_noun = build_tuple(&mut arena, &pk_tuple);

    let txid_ctx = tx_id_ctx(&mut arena)?;

    // pkh = hash-noun-varlen(pubkey noun)
    let pkh_digest = tip5::hash_noun_varlen(pk_noun, &arena)?;
    let pkh_key_noun = build_hash_noun(&mut arena, pkh_digest);

    // Detect outer wrapper.
    enum Outer {
        RawTx {
            raw: Noun,
        },
        TxTransact {
            raw: Noun,
            tail: Noun,
        },
        WalletV1 {
            spends: Noun,
        },
        WalletTxV1 {
            tag: Noun,
            spends: Noun,
            display: Noun,
            witness_data: Noun,
        },
    }

    let outer = if let Some((ver, id, _spends)) = tuple3(root, &arena) {
        if noun_atom_u64(ver, &arena) == Some(1) && parse_hash(id, &arena).is_some() {
            Some(Outer::RawTx { raw: root })
        } else {
            None
        }
    } else {
        None
    };

    let outer = if let Some(outer) = outer {
        outer
    } else if let Some((head, tail)) = uncons(root, &arena) {
        if let Some((ver, id, _spends)) = tuple3(head, &arena) {
            if noun_atom_u64(ver, &arena) == Some(1) && parse_hash(id, &arena).is_some() {
                Outer::TxTransact { raw: head, tail }
            } else {
                return Err(SignDraftError::Unsupported);
            }
        } else if matches!(head, Noun::Atom(_)) {
            let is_tag_v1 = noun_atom_u64(head, &arena) == Some(1);
            if is_tag_v1 {
                let (_name, spends, display, witness_data) =
                    tuple4(tail, &arena).ok_or(SignDraftError::Malformed)?;
                if looks_like_spends_v1_map(spends, &arena) {
                    Outer::WalletTxV1 {
                        tag: head,
                        spends,
                        display,
                        witness_data,
                    }
                } else {
                    return Err(SignDraftError::Malformed);
                }
            } else {
                if looks_like_spends_v1_map(tail, &arena) {
                    Outer::WalletV1 { spends: tail }
                } else {
                    return Err(SignDraftError::Malformed);
                }
            }
        } else {
            return Err(SignDraftError::Malformed);
        }
    } else {
        return Err(SignDraftError::Malformed);
    };

    let new_root = match outer {
        Outer::WalletV1 { spends } => {
            let new_spends = sign_spends_map(
                &mut arena,
                spends,
                pk_noun,
                pkh_key_noun,
                pkh_digest,
                cfg,
                pk_coords,
            )?;
            let tx_id = compute_tx_id_v1(new_spends, &arena, &txid_ctx)?;
            let name_b58 = digest_to_b58(tx_id);
            let name_noun = arena.alloc_atom_bytes(name_b58.as_bytes());
            build_tuple(&mut arena, &[name_noun, new_spends])
        }
        Outer::WalletTxV1 {
            tag,
            spends,
            display,
            witness_data,
        } => {
            let new_spends = sign_spends_map(
                &mut arena,
                spends,
                pk_noun,
                pkh_key_noun,
                pkh_digest,
                cfg,
                pk_coords,
            )?;
            let tx_id = compute_tx_id_v1(new_spends, &arena, &txid_ctx)?;
            let name_b58 = digest_to_b58(tx_id);
            let name_noun = arena.alloc_atom_bytes(name_b58.as_bytes());
            build_tuple(
                &mut arena,
                &[tag, name_noun, new_spends, display, witness_data],
            )
        }
        Outer::RawTx { raw } => {
            let (ver, _id, spends) = tuple3(raw, &arena).ok_or(SignDraftError::Malformed)?;
            let new_spends = sign_spends_map(
                &mut arena,
                spends,
                pk_noun,
                pkh_key_noun,
                pkh_digest,
                cfg,
                pk_coords,
            )?;
            let tx_id = compute_tx_id_v1(new_spends, &arena, &txid_ctx)?;
            let id_noun = build_hash_noun(&mut arena, tx_id);
            build_tuple(&mut arena, &[ver, id_noun, new_spends])
        }
        Outer::TxTransact { raw, tail } => {
            let (ver, _id, spends) = tuple3(raw, &arena).ok_or(SignDraftError::Malformed)?;
            let new_spends = sign_spends_map(
                &mut arena,
                spends,
                pk_noun,
                pkh_key_noun,
                pkh_digest,
                cfg,
                pk_coords,
            )?;
            let tx_id = compute_tx_id_v1(new_spends, &arena, &txid_ctx)?;
            let id_noun = build_hash_noun(&mut arena, tx_id);
            let new_raw = build_tuple(&mut arena, &[ver, id_noun, new_spends]);
            arena.alloc_cell(new_raw, tail)
        }
    };

    Ok(jam(new_root, &arena))
}

pub fn rewrite_txid_v1(tx_jam: &[u8]) -> Result<RewriteTxIdV1, SignDraftError> {
    let mut arena = Arena::new();
    let root = cue(tx_jam, &mut arena)?;
    let txid_ctx = tx_id_ctx(&mut arena)?;

    let rewrite = |arena: &mut Arena, name: String, new_root: Noun| -> RewriteTxIdV1 {
        RewriteTxIdV1 {
            name,
            rewritten: Some(jam(new_root, arena)),
        }
    };

    if let Some((ver, id, spends)) = tuple3(root, &arena) {
        if noun_atom_u64(ver, &arena) == Some(1) && parse_hash(id, &arena).is_some() {
            let tx_id = compute_tx_id_v1(spends, &arena, &txid_ctx)?;
            let name = digest_to_b58(tx_id);
            if parse_hash(id, &arena) == Some(tx_id) {
                return Ok(RewriteTxIdV1 {
                    name,
                    rewritten: None,
                });
            }
            let id_noun = build_hash_noun(&mut arena, tx_id);
            let new_root = build_tuple(&mut arena, &[ver, id_noun, spends]);
            return Ok(rewrite(&mut arena, name, new_root));
        }
    }

    let Some((head, tail)) = uncons(root, &arena) else {
        return Err(SignDraftError::Malformed);
    };

    if let Some((ver, id, spends)) = tuple3(head, &arena) {
        if noun_atom_u64(ver, &arena) != Some(1) || parse_hash(id, &arena).is_none() {
            return Err(SignDraftError::Unsupported);
        }
        let tx_id = compute_tx_id_v1(spends, &arena, &txid_ctx)?;
        let name = digest_to_b58(tx_id);
        if parse_hash(id, &arena) == Some(tx_id) {
            return Ok(RewriteTxIdV1 {
                name,
                rewritten: None,
            });
        }
        let id_noun = build_hash_noun(&mut arena, tx_id);
        let new_raw = build_tuple(&mut arena, &[ver, id_noun, spends]);
        let new_root = arena.alloc_cell(new_raw, tail);
        return Ok(rewrite(&mut arena, name, new_root));
    }

    if !matches!(head, Noun::Atom(_)) {
        return Err(SignDraftError::Malformed);
    }

    if noun_atom_u64(head, &arena) == Some(1) {
        let (name_noun, spends, display, witness_data) =
            tuple4(tail, &arena).ok_or(SignDraftError::Malformed)?;
        if !looks_like_spends_v1_map(spends, &arena) {
            return Err(SignDraftError::Malformed);
        }
        let tx_id = compute_tx_id_v1(spends, &arena, &txid_ctx)?;
        let name = digest_to_b58(tx_id);
        if atom_eq_bytes(name_noun, name.as_bytes(), &arena) {
            return Ok(RewriteTxIdV1 {
                name,
                rewritten: None,
            });
        }
        let name_noun = arena.alloc_atom_bytes(name.as_bytes());
        let new_root = build_tuple(
            &mut arena,
            &[head, name_noun, spends, display, witness_data],
        );
        return Ok(rewrite(&mut arena, name, new_root));
    }

    if !looks_like_spends_v1_map(tail, &arena) {
        return Err(SignDraftError::Malformed);
    }
    let tx_id = compute_tx_id_v1(tail, &arena, &txid_ctx)?;
    let name = digest_to_b58(tx_id);
    if atom_eq_bytes(head, name.as_bytes(), &arena) {
        return Ok(RewriteTxIdV1 {
            name,
            rewritten: None,
        });
    }
    let name_noun = arena.alloc_atom_bytes(name.as_bytes());
    let new_root = build_tuple(&mut arena, &[name_noun, tail]);
    Ok(rewrite(&mut arena, name, new_root))
}
