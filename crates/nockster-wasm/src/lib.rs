use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use nockapp::noun::slab::NounSlab;
use noun_serde::NounEncode;
use tx_types::hashing::hashable::Hashable;
use tx_types::transaction_types::{Hash, SchnorrPubkey, F6LT};

mod compose_v1;
mod tip5;

#[wasm_bindgen(start)]
pub fn init() {
    // pre-grow memory before setting up panic hook or doing any allocations
    #[cfg(target_arch = "wasm32")]
    {
        let initial_pages = core::arch::wasm32::memory_size(0);

        // 256MB to ensure we have space for 64MB nockstack
        let target_pages = 4096;
        if initial_pages < target_pages {
            let grow_result = core::arch::wasm32::memory_grow(0, target_pages - initial_pages);
            if grow_result != usize::MAX {
                let final_pages = core::arch::wasm32::memory_size(0);
            }
        }
    }

    console_error_panic_hook::set_once();

    #[cfg(target_arch = "wasm32")]
    {
        let pages = core::arch::wasm32::memory_size(0);
        let bytes = pages * 64 * 1024;
        web_sys::console::log_1(
            &format!(
                "WASM memory: {} pages = {} MB",
                pages,
                bytes / (1024 * 1024)
            )
            .into(),
        );
    }
}

#[cfg(target_arch = "wasm32")]
fn fmt_u64x5(arr: &[u64; 5]) -> String {
    format!(
        "{:016x}.{:016x}.{:016x}.{:016x}.{:016x}",
        arr[0], arr[1], arr[2], arr[3], arr[4]
    )
}

#[cfg(target_arch = "wasm32")]
fn format_hash(hash: &Hash) -> String {
    fmt_u64x5(&hash.values)
}

/// Hash a SchnorrPubkey using NounSlab (not NockStack!)
/// Converts to noun then hashes with reference hasher
pub fn build_schnorr_pubkey_hashable(pk: &SchnorrPubkey) -> Result<Hashable, JsValue> {
    use nockapp::noun::slab::NounSlab;
    use noun_serde::NounEncode;
    use tx_types::hashing::hashable::Hashable;

    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&"hash_schnorr_pubkey: creating NounSlab".into());

    let mut slab: NounSlab = NounSlab::new();

    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&"hash_schnorr_pubkey: converting pubkey to noun".into());

    let pk_noun = pk.to_noun(&mut slab);

    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(
        &"hash_schnorr_pubkey: hashing noun (may crash if NockStack needed)".into(),
    );

    let pk_hash = tx_types::hashing::tip5::Tip5Hasher::hash_noun_varlen(pk_noun)
        .map_err(|e| JsValue::from_str(&format!("Pubkey hash failed: {:?}", e)))?;

    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(
        &format!(
            "hash_schnorr_pubkey: success! hash = {}",
            format_hash(&pk_hash)
        )
        .into(),
    );

    Ok(Hashable::Hash(pk_hash))
}

pub fn hash_hashable_wasm(h: &tx_types::hashing::hashable::Hashable) -> Result<Hash, JsValue> {
    use tx_types::hashing::hashable::Hashable;

    match h {
        Hashable::Leaf(data) => {
            // convert bytes to zero-padded u64s
            let mut u64s = Vec::new();
            for chunk in data.chunks(8) {
                let mut bytes = [0u8; 8];
                bytes[..chunk.len()].copy_from_slice(chunk);
                u64s.push(u64::from_le_bytes(bytes));
            }
            Ok(tip5::hash_varlen_u64s(&u64s))
        }
        Hashable::Hash(digest) => {
            // already hashed
            Ok(digest.clone())
        }
        Hashable::Cell(left, right) => {
            // recursively hash both sides and combine
            let left_hash = hash_hashable_wasm(left)?;
            let right_hash = hash_hashable_wasm(right)?;
            // combine hash([left.values, right.values])
            Ok(tip5::hash_two_hashes(&left_hash, &right_hash))
        }
        Hashable::List(items) => {
            // hash each item recursively
            let hashes: Result<Vec<Hash>, JsValue> = items.iter().map(hash_hashable_wasm).collect();
            let hashes = hashes?;

            // hash the list of hashes
            Ok(tip5::hash_hash_list(&hashes))
        }
    }
}

/// ---- JS ------------------------------------------------------------

#[wasm_bindgen]
#[derive(Serialize, Deserialize, Clone)]
pub struct TransactionInfo {
    tx_id: String,
    shape: String,
    version: u8,
    input_count: usize,
}

#[wasm_bindgen]
impl TransactionInfo {
    #[wasm_bindgen(getter)]
    pub fn tx_id(&self) -> String {
        self.tx_id.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn shape(&self) -> String {
        self.shape.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn version(&self) -> u8 {
        self.version
    }
    #[wasm_bindgen(getter)]
    pub fn input_count(&self) -> usize {
        self.input_count
    }
}

/// ---- ParsedTransaction ---------------------------------------------------

#[wasm_bindgen]
pub struct ParsedTransaction {
    inner: ParsedTxInner,
}

enum ParsedTxInner {
    V1(ParsedTxV1),
}

struct ParsedTxV1 {
    outer: OuterTypeV1,
    tx_id_b58: String,
    spend_count: usize,
    arena: tx_types::pokenoun::Arena,
    spends: tx_types::pokenoun::Noun,
}

enum OuterTypeV1 {
    RawTx,
    TxTransact,
    WalletTx,
}

mod v1_draft {
    use super::*;
    use tx_types::pokenoun::{cue, Arena, CodecError, Noun};

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

    fn hash_to_b58(digest: [u64; 5]) -> String {
        tx_types::transaction_types::Hash { values: digest }.to_b58()
    }

    fn parse_nname(noun: Noun, arena: &Arena) -> Option<(String, String)> {
        let (first, rest) = uncons(noun, arena)?;
        let (second, _end) = uncons(rest, arena)?;
        let first = parse_hash(first, arena)?;
        let second = parse_hash(second, arena)?;
        Some((hash_to_b58(first), hash_to_b58(second)))
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

    fn zmap_count(map: Noun, arena: &Arena) -> usize {
        if map == arena.atom0() {
            return 0;
        }
        let Some((_node, left, right)) = decompose_map(map, arena) else {
            return 0;
        };
        1 + zmap_count(left, arena) + zmap_count(right, arena)
    }

    fn zset_for_each(set: Noun, arena: &Arena, f: &mut impl FnMut(Noun)) {
        if set == arena.atom0() {
            return;
        }
        let Some((value, left, right)) = tuple3(set, arena) else {
            return;
        };
        f(value);
        zset_for_each(left, arena, f);
        zset_for_each(right, arena, f);
    }

    fn zset_count_up_to(set: Noun, arena: &Arena, limit: u8) -> u8 {
        if limit == 0 || set == arena.atom0() {
            return 0;
        }
        let Some((_value, left, right)) = tuple3(set, arena) else {
            return 0;
        };
        let mut count = 1u8;
        if count >= limit {
            return count;
        }
        count = count.saturating_add(zset_count_up_to(left, arena, limit - count));
        if count >= limit {
            return count;
        }
        count = count.saturating_add(zset_count_up_to(right, arena, limit - count));
        count
    }

    fn map_get_atom_key(map: Noun, arena: &Arena, key_bytes: &[u8]) -> Option<Noun> {
        if map == arena.atom0() {
            return None;
        }
        let (node, left, right) = decompose_map(map, arena)?;
        let (key, value) = decompose_pair(node, arena)?;
        if let Noun::Atom(id) = key {
            if arena.atom_bytes(id) == key_bytes {
                return Some(value);
            }
        }
        map_get_atom_key(left, arena, key_bytes)
            .or_else(|| map_get_atom_key(right, arena, key_bytes))
    }

    fn seed_recipient_pkh(note_data: Noun, arena: &Arena) -> Option<[u64; 5]> {
        let lock_data = map_get_atom_key(note_data, arena, b"lock")?;
        let (ver, lock) = tuple2(lock_data, arena)?;
        if noun_atom_u64(ver, arena) != Some(0) {
            return None;
        }
        let (prim, _rest) = uncons(lock, arena)?;
        let (header, body) = tuple2(prim, arena)?;
        let header_id = match header {
            Noun::Atom(id) => id,
            _ => return None,
        };
        if arena.atom_bytes(header_id) != b"pkh" {
            return None;
        }
        let (m, h_set) = tuple2(body, arena)?;
        if noun_atom_u64(m, arena) != Some(1) {
            return None;
        }
        // Only treat it as a simple recipient if there's exactly one allowed pkh hash.
        // For multisig (or 1-of-n), fall back to lock-root rather than picking an arbitrary hash.
        if zset_count_up_to(h_set, arena, 2) != 1 {
            return None;
        }
        let (any, _l, _r) = tuple3(h_set, arena)?;
        parse_hash(any, arena)
    }

    fn collect_spend_details(spends: Noun, arena: &Arena) -> serde_json::Value {
        use serde_json::json;
        if spends == arena.atom0() {
            return json!([]);
        }

        let mut out: Vec<serde_json::Value> = Vec::new();
        let mut walk = |pair: Noun, _left: Noun, _right: Noun| {
            let Some((name_noun, spend_noun)) = decompose_pair(pair, arena) else {
                return;
            };
            let (name_first, name_last) = match parse_nname(name_noun, arena) {
                Some(v) => v,
                None => ("".to_string(), "".to_string()),
            };
            let Some((ver, body)) = tuple2(spend_noun, arena) else {
                return;
            };
            if noun_atom_u64(ver, arena) != Some(1) {
                return;
            }
            let Some((_witness, seeds, fee_noun)) = tuple3(body, arena) else {
                return;
            };
            let fee = noun_atom_u64(fee_noun, arena).unwrap_or(0);

            let mut seeds_out: Vec<serde_json::Value> = Vec::new();
            zset_for_each(seeds, arena, &mut |seed| {
                let Some((
                    _output_source,
                    lock_root_noun,
                    note_data_noun,
                    gift_noun,
                    parent_hash_noun,
                )) = tuple5(seed, arena)
                else {
                    return;
                };
                let lock_root = parse_hash(lock_root_noun, arena);
                let recipient = seed_recipient_pkh(note_data_noun, arena).or(lock_root);
                let gift = noun_atom_u64(gift_noun, arena).unwrap_or(0);
                let parent_hash = parse_hash(parent_hash_noun, arena);

                seeds_out.push(json!({
                    "gift": gift,
                    "recipient_pkh": recipient.map(hash_to_b58),
                    "lock_root": lock_root.map(hash_to_b58),
                    "parent_hash": parent_hash.map(hash_to_b58),
                }));
            });

            out.push(json!({
                "name_first": name_first,
                "name_last": name_last,
                "fee": fee,
                "seeds": seeds_out,
            }));
        };

        fn walk_map(map: Noun, arena: &Arena, f: &mut impl FnMut(Noun, Noun, Noun)) {
            if map == arena.atom0() {
                return;
            }
            let Some((node, left, right)) = decompose_map(map, arena) else {
                return;
            };
            f(node, left, right);
            walk_map(left, arena, f);
            walk_map(right, arena, f);
        }

        walk_map(spends, arena, &mut walk);
        serde_json::Value::Array(out)
    }

    pub(super) struct ParsedV1 {
        pub outer: OuterTypeV1,
        pub tx_id_b58: String,
        pub spend_count: usize,
        pub arena: Arena,
        pub spends: Noun,
    }

    pub(super) fn parse(bytes: &[u8]) -> Result<ParsedV1, JsValue> {
        let mut arena = Arena::new();
        let root = cue(bytes, &mut arena).map_err(|e| match e {
            CodecError::UnexpectedEof => JsValue::from_str("jam decode: unexpected eof"),
            CodecError::AtomTooLarge => JsValue::from_str("jam decode: atom too large"),
            CodecError::InvalidBackref => JsValue::from_str("jam decode: invalid backref"),
            CodecError::InvalidEncoding => JsValue::from_str("jam decode: invalid encoding"),
        })?;

        if let Some((tag, name_noun, spends, _display, _witness_data)) = tuple5(root, &arena) {
            if noun_atom_u64(tag, &arena) == Some(1) {
                if let Noun::Atom(name_atom) = name_noun {
                    if let Ok(name_str) = core::str::from_utf8(arena.atom_bytes(name_atom)) {
                        if tx_types::transaction_types::Hash::from_b58(name_str).is_ok() {
                            let tx_id_b58 = name_str.to_string();
                            let spend_count = zmap_count(spends, &arena);
                            return Ok(ParsedV1 {
                                outer: OuterTypeV1::WalletTx,
                                tx_id_b58,
                                spend_count,
                                arena,
                                spends,
                            });
                        }
                    }
                }
            }
        }

        // wallet tx v1: [name=@t spends]
        if let Some((name_noun, spends)) = tuple2(root, &arena) {
            if let Noun::Atom(name_atom) = name_noun {
                if let Ok(name_str) = core::str::from_utf8(arena.atom_bytes(name_atom)) {
                    if tx_types::transaction_types::Hash::from_b58(name_str).is_ok() {
                        let tx_id_b58 = name_str.to_string();
                        let spend_count = zmap_count(spends, &arena);
                        return Ok(ParsedV1 {
                            outer: OuterTypeV1::WalletTx,
                            tx_id_b58,
                            spend_count,
                            arena,
                            spends,
                        });
                    }
                }
            }
        }

        // raw tx v1: [ver=1 id spends]
        if let Some((ver, id, spends)) = tuple3(root, &arena) {
            if noun_atom_u64(ver, &arena) == Some(1) {
                if let Some(id_digest) = parse_hash(id, &arena) {
                    let tx_id_b58 = hash_to_b58(id_digest);
                    let spend_count = zmap_count(spends, &arena);
                    return Ok(ParsedV1 {
                        outer: OuterTypeV1::RawTx,
                        tx_id_b58,
                        spend_count,
                        arena,
                        spends,
                    });
                }
            }
        }

        // [raw-tx tail]
        if let Some((head, _tail)) = uncons(root, &arena) {
            if let Some((ver, id, spends)) = tuple3(head, &arena) {
                if noun_atom_u64(ver, &arena) == Some(1) {
                    if let Some(id_digest) = parse_hash(id, &arena) {
                        let tx_id_b58 = hash_to_b58(id_digest);
                        let spend_count = zmap_count(spends, &arena);
                        return Ok(ParsedV1 {
                            outer: OuterTypeV1::TxTransact,
                            tx_id_b58,
                            spend_count,
                            arena,
                            spends,
                        });
                    }
                }
            }
        }

        // tx-engine wrapper (TxV1): [ver=1 raw-tx total-size outputs]
        if let Some((ver, raw_tx, _total, _outputs)) = tuple4(root, &arena) {
            if noun_atom_u64(ver, &arena) == Some(1) {
                if let Some((ver2, id, spends)) = tuple3(raw_tx, &arena) {
                    if noun_atom_u64(ver2, &arena) == Some(1) {
                        if let Some(id_digest) = parse_hash(id, &arena) {
                            let tx_id_b58 = hash_to_b58(id_digest);
                            let spend_count = zmap_count(spends, &arena);
                            return Ok(ParsedV1 {
                                outer: OuterTypeV1::RawTx,
                                tx_id_b58,
                                spend_count,
                                arena,
                                spends,
                            });
                        }
                    }
                }
            }
        }

        Err(JsValue::from_str("unrecognized v1 transaction shape"))
    }

    pub(super) fn details_json(tx_id_b58: &str, spends: Noun, arena: &Arena) -> serde_json::Value {
        use serde_json::json;
        let spends_json = collect_spend_details(spends, arena);
        json!({
            "version": 1,
            "transaction_id": tx_id_b58,
            "spend_count": zmap_count(spends, arena),
            "spends": spends_json,
        })
    }
}

#[wasm_bindgen]
impl ParsedTransaction {
    /// Parse a jam file (transaction draft) from bytes
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Result<ParsedTransaction, JsValue> {
        match v1_draft::parse(bytes) {
            Ok(v1) => Ok(ParsedTransaction {
                inner: ParsedTxInner::V1(ParsedTxV1 {
                    outer: v1.outer,
                    tx_id_b58: v1.tx_id_b58,
                    spend_count: v1.spend_count,
                    arena: v1.arena,
                    spends: v1.spends,
                }),
            }),
            Err(e) => {
                web_sys::console::error_1(&format!("WASM: V1 parse failed: {:?}", e).into());
                Err(JsValue::from_str(
                    "Unrecognized Bythos/V1 transaction format",
                ))
            }
        }
    }

    /// get transaction info
    pub fn info(&self) -> TransactionInfo {
        match &self.inner {
            ParsedTxInner::V1(v1) => TransactionInfo {
                tx_id: v1.tx_id_b58.clone(),
                shape: match &v1.outer {
                    OuterTypeV1::RawTx => "raw-tx".to_string(),
                    OuterTypeV1::TxTransact => "[raw-tx tail]".to_string(),
                    OuterTypeV1::WalletTx => "wallet-tx".to_string(),
                },
                version: 1,
                input_count: v1.spend_count,
            },
        }
    }

    /// get transaction details as json
    pub fn get_details(&self) -> JsValue {
        match &self.inner {
            ParsedTxInner::V1(v1) => {
                let details = v1_draft::details_json(&v1.tx_id_b58, v1.spends, &v1.arena);
                match serde_wasm_bindgen::to_value(&details) {
                    Ok(result) => result,
                    Err(e) => {
                        web_sys::console::error_1(
                            &format!("WASM: serialization failed: {}", e).into(),
                        );
                        JsValue::from_str("{\"error\": \"serialization failed\"}")
                    }
                }
            }
        }
    }

    pub fn get_signing_inputs(&self, _device_pubkeys: JsValue) -> Result<Vec<JsValue>, JsValue> {
        Ok(Vec::new())
    }

    pub fn apply_signatures(&mut self, _signatures: JsValue) -> Result<(), JsValue> {
        Err(JsValue::from_str(
            "apply_signatures: pre-Bythos manual signing is unsupported; use SignDraft",
        ))
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, JsValue> {
        Err(JsValue::from_str(
            "to_bytes: pre-Bythos manual signing is unsupported; use SignDraft",
        ))
    }
}

#[wasm_bindgen]
pub fn cheetah_pkh_b58(pubkey_x: Vec<String>, pubkey_y: Vec<String>) -> Result<String, JsValue> {
    if pubkey_x.len() != 6 || pubkey_y.len() != 6 {
        return Err(JsValue::from_str("expected 6 limbs for x and y"));
    }
    let mut x = [0u64; 6];
    let mut y = [0u64; 6];
    for (i, s) in pubkey_x.iter().enumerate() {
        x[i] = s
            .parse::<u64>()
            .map_err(|_| JsValue::from_str("invalid u64 limb in x"))?;
    }
    for (i, s) in pubkey_y.iter().enumerate() {
        y[i] = s
            .parse::<u64>()
            .map_err(|_| JsValue::from_str("invalid u64 limb in y"))?;
    }

    let pk = SchnorrPubkey {
        x: F6LT { values: x },
        y: F6LT { values: y },
        inf: false,
    };
    let mut slab: NounSlab = NounSlab::new();
    let pk_noun = pk.to_noun(&mut slab);
    let digest = tx_types::hashing::tip5::Tip5Hasher::hash_noun_varlen(pk_noun)
        .map_err(|_| JsValue::from_str("failed to hash pubkey"))?;
    Ok(digest.to_b58())
}
