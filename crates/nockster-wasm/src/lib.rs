use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use nockapp::noun::slab::NounSlab;
use noun_serde::NounEncode;
use tx_types::hashing::hashable::Hashable;
use tx_types::transaction_types::{Hash, SchnorrPubkey, F6LT};

mod compose_v1;
mod review_v1;
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

// --- Wallet keyfile interop (keys.export / master-pubkey.export) and
// --- preimage-vault noun helpers. Uses the pokenoun codec so the byte-level
// --- noun encoding matches the firmware and the nockchain wallet.
mod wallet_keyfile {
    use super::*;
    use tx_types::pokenoun::{cue, jam, Arena, Noun};

    fn uncons(noun: Noun, arena: &Arena) -> Option<(Noun, Noun)> {
        match noun {
            Noun::Cell(id) => {
                let cell = arena.cell(id);
                Some((cell.head, cell.tail))
            }
            _ => None,
        }
    }

    fn atom_text(noun: Noun, arena: &Arena) -> Option<String> {
        match noun {
            Noun::Atom(id) => core::str::from_utf8(arena.atom_bytes(id))
                .ok()
                .map(|s| s.to_string()),
            _ => None,
        }
    }

    fn atom_is(noun: Noun, arena: &Arena, tag: &[u8]) -> bool {
        match noun {
            Noun::Atom(id) => arena.atom_eq_bytes(id, tag),
            _ => false,
        }
    }

    #[derive(Serialize, Default)]
    pub struct KeyfileSummary {
        /// Seed phrases found in the file (the wallet stores the mnemonic as
        /// a `%seed` entry); usually zero or one.
        pub seedphrases: Vec<String>,
        pub coil_pub_count: u32,
        pub coil_prv_count: u32,
        pub label_count: u32,
        pub watch_count: u32,
        /// Coil versions seen (0 = pre-Oct-2025 addressing, 1 = current).
        pub versions: Vec<u8>,
        pub entry_count: u32,
    }

    /// Walk one `meta` noun: `[%coil coil-v3]`, `[%label @t]`, `[%seed @t]`,
    /// or `[%watch-key @t]`.
    fn scan_meta(meta: Noun, arena: &Arena, out: &mut KeyfileSummary) {
        let Some((tag, payload)) = uncons(meta, arena) else {
            return;
        };
        if atom_is(tag, arena, b"seed") {
            if let Some(text) = atom_text(payload, arena) {
                if !text.is_empty() {
                    out.seedphrases.push(text);
                }
            }
        } else if atom_is(tag, arena, b"label") {
            out.label_count += 1;
        } else if atom_is(tag, arena, b"watch-key") {
            out.watch_count += 1;
        } else if atom_is(tag, arena, b"coil") {
            // coil-v3: [%0|%1 [key cc]]; legacy coil-v0 is bare [key cc].
            let (version, coil_data) = match uncons(payload, arena) {
                Some((head, tail)) if matches!(head, Noun::Atom(_)) => {
                    let v = match head {
                        Noun::Atom(id) => arena.atom_u64(id).unwrap_or(0) as u8,
                        _ => 0,
                    };
                    (v, tail)
                }
                _ => (0, payload),
            };
            if !out.versions.contains(&version) {
                out.versions.push(version);
            }
            if let Some((key, _cc)) = uncons(coil_data, arena) {
                if let Some((key_tag, _key_atom)) = uncons(key, arena) {
                    if atom_is(key_tag, arena, b"pub") {
                        out.coil_pub_count += 1;
                    } else if atom_is(key_tag, arena, b"prv") {
                        out.coil_prv_count += 1;
                    }
                }
            }
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<KeyfileSummary, String> {
        let mut arena = Arena::new();
        let root = cue(bytes, &mut arena).map_err(|_| "not a valid jammed noun".to_string())?;
        let mut out = KeyfileSummary::default();
        // keys.export is `(list [trek meta])`.
        let mut cursor = root;
        while let Some((entry, rest)) = uncons(cursor, &arena) {
            out.entry_count += 1;
            if let Some((_trek, meta)) = uncons(entry, &arena) {
                scan_meta(meta, &arena, &mut out);
            }
            cursor = rest;
            if out.entry_count > 4096 {
                return Err("keyfile too large".to_string());
            }
        }
        if out.entry_count == 0 {
            return Err("no entries found (is this a keys.export file?)".to_string());
        }
        Ok(out)
    }


    /// Affine-point serialization matching `ser-p` on the Hoon side (and
    /// `tx_types::crypto::cheetah_nostd::ser_a_pt`, which is cfg'd out of std
    /// builds): 0x01 sentinel, then Y and X limbs big-endian, high limb first.
    fn ser_a_pt(pk: &([u64; 6], [u64; 6])) -> [u8; 97] {
        let (x, y) = pk;
        let mut out = [0u8; 97];
        out[0] = 0x01;
        let mut off = 1;
        for &w in y.iter().rev().chain(x.iter().rev()) {
            out[off..off + 8].copy_from_slice(&w.to_be_bytes());
            off += 8;
        }
        out
    }

    /// Build the jammed `coil` noun the nockchain wallet's
    /// `import-master-pubkey` expects: `[%coil [%1 [[%pub p=@] cc=@]]]`.
    /// Atom byte order follows the CKD HMAC convention (atom LE bytes ==
    /// `ser_a_pt` / chain-code array order), which is what the Hoon side
    /// produces and consumes.
    pub fn build_master_pubkey(x: [u64; 6], y: [u64; 6], chain_code: &[u8]) -> Vec<u8> {
        // `ser_a_pt` (like Hoon's `hmac-sha512l` input rendering) is the
        // atom's MSB-first octet stream; pokenoun atoms take LE bytes, so
        // both atoms are byte-reversed here. The 0x01 ser-p sentinel becomes
        // the atom's top byte, preserving the fixed 97-byte width.
        let mut ser = ser_a_pt(&(x, y));
        ser.reverse();
        let mut cc_le = [0u8; 32];
        for (dst, src) in cc_le.iter_mut().zip(chain_code.iter().rev()) {
            *dst = *src;
        }
        let mut arena = Arena::new();
        let tag_coil = arena.alloc_atom_bytes(b"coil");
        let tag_pub = arena.alloc_atom_bytes(b"pub");
        let pub_atom = arena.alloc_atom_bytes(&ser);
        let cc_atom = arena.alloc_atom_bytes(&cc_le);
        let key = arena.alloc_cell(tag_pub, pub_atom);
        let coil_data = arena.alloc_cell(key, cc_atom);
        let version = arena.alloc_atom_u64(1);
        let coil_v3 = arena.alloc_cell(version, coil_data);
        let coil = arena.alloc_cell(tag_coil, coil_v3);
        jam(coil, &arena)
    }

    pub fn jam_atom(bytes: &[u8]) -> Vec<u8> {
        let mut arena = Arena::new();
        let atom = arena.alloc_atom_bytes(bytes);
        jam(atom, &arena)
    }

    pub fn cue_atom(bytes: &[u8]) -> Result<Vec<u8>, String> {
        let mut arena = Arena::new();
        let root = cue(bytes, &mut arena).map_err(|_| "not a valid jammed noun".to_string())?;
        match root {
            Noun::Atom(id) => Ok(arena.atom_bytes(id).to_vec()),
            Noun::Cell(_) => Err("noun is a cell, not an atom".to_string()),
        }
    }

    pub fn commitment_b58(bytes: &[u8]) -> Result<String, String> {
        let mut arena = Arena::new();
        let root = cue(bytes, &mut arena).map_err(|_| "not a valid jammed noun".to_string())?;
        let digest = tx_types::pokenoun::hash_noun_varlen(root, &arena)
            .map_err(|_| "tip5 hash failed".to_string())?;
        Ok(Hash { values: digest }.to_b58())
    }
}

/// Parse a nockchain-wallet `keys.export` file and summarize its contents
/// (seed phrases, key coils, labels, watch entries).
#[wasm_bindgen]
pub fn parse_wallet_keyfile(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let summary = wallet_keyfile::parse(bytes).map_err(|e| JsValue::from_str(&e))?;
    serde_wasm_bindgen::to_value(&summary).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Build a `master-pubkey.export` file (jammed coil) the nockchain wallet can
/// import for watch-only use, from device-exported master pubkey + chain code.
#[wasm_bindgen]
pub fn build_master_pubkey_export(
    pubkey_x: Vec<String>,
    pubkey_y: Vec<String>,
    chain_code: &[u8],
) -> Result<Vec<u8>, JsValue> {
    if pubkey_x.len() != 6 || pubkey_y.len() != 6 {
        return Err(JsValue::from_str("expected 6 limbs for x and y"));
    }
    if chain_code.len() != 32 {
        return Err(JsValue::from_str("expected 32-byte chain code"));
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
    Ok(wallet_keyfile::build_master_pubkey(x, y, chain_code))
}

/// Wrap raw bytes as a jammed atom noun — the usual shape for a `%hax`
/// preimage going into the device vault.
#[wasm_bindgen]
pub fn jam_byte_atom(bytes: &[u8]) -> Vec<u8> {
    wallet_keyfile::jam_atom(bytes)
}

/// Extract the raw bytes of a jammed atom noun (e.g. a revealed vault
/// preimage). Errors if the noun is a cell.
#[wasm_bindgen]
pub fn cue_byte_atom(bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
    wallet_keyfile::cue_atom(bytes).map_err(|e| JsValue::from_str(&e))
}

/// Tip5 hash-noun commitment (base58) of a jammed preimage — the value a
/// `%hax` lock commits to. Host-side preview; the device computes its own.
#[wasm_bindgen]
pub fn noun_commitment_b58(bytes: &[u8]) -> Result<String, JsValue> {
    wallet_keyfile::commitment_b58(bytes).map_err(|e| JsValue::from_str(&e))
}

/// Render a Tip5 digest given as five decimal u64 limb strings (e.g. a vault
/// commitment from the device) in the chain's base58 hash encoding.
#[wasm_bindgen]
pub fn tip5_limbs_b58(limbs: Vec<String>) -> Result<String, JsValue> {
    if limbs.len() != 5 {
        return Err(JsValue::from_str("expected 5 limbs"));
    }
    let mut values = [0u64; 5];
    for (i, s) in limbs.iter().enumerate() {
        values[i] = s
            .parse::<u64>()
            .map_err(|_| JsValue::from_str("invalid u64 limb"))?;
    }
    Ok(Hash { values }.to_b58())
}

// --- Generic noun inspector: cue any jam and render it as a typed tree, the
// --- pseudo-JSON treatment used for transactions, for arbitrary nouns
// --- (.tx/.psnt/keys.export/.sig/.jam). Host-side, read-only.
mod noun_inspect {
    use serde::Serialize;
    use tx_types::pokenoun::{cue, Arena, Noun};

    const MAX_NODES: usize = 20_000;
    const MAX_DEPTH: usize = 96;
    const MAX_LIST: usize = 4_096;

    #[derive(Serialize)]
    #[serde(tag = "kind")]
    pub enum NounView {
        /// A leaf atom with type heuristics.
        #[serde(rename = "atom")]
        Atom {
            /// Decimal value when it fits in u64.
            num: Option<String>,
            /// Big-endian hex (most significant byte first).
            hex: String,
            /// Printable-ASCII rendering of the cord bytes, when applicable.
            text: Option<String>,
            /// Short `%tas`-style tag, when the bytes look like one.
            tag: Option<String>,
            /// Significant byte length.
            bytes: usize,
        },
        /// A null-terminated proper list, rendered as its elements.
        #[serde(rename = "list")]
        List { items: Vec<NounView>, truncated: bool },
        /// An improper cell (head/tail pair).
        #[serde(rename = "cell")]
        Cell {
            head: Box<NounView>,
            tail: Box<NounView>,
        },
        /// Depth/size budget exceeded; rendering stopped here.
        #[serde(rename = "elided")]
        Elided,
    }

    fn render_atom(bytes: &[u8]) -> NounView {
        // pokenoun atom bytes are little-endian, significant bytes only.
        let num = if bytes.len() <= 8 {
            let mut v = 0u64;
            for (i, b) in bytes.iter().enumerate() {
                v |= (*b as u64) << (8 * i);
            }
            Some(v.to_string())
        } else {
            None
        };
        let mut hex = String::from("0x");
        if bytes.is_empty() {
            hex.push('0');
        } else {
            for b in bytes.iter().rev() {
                hex.push_str(&format!("{b:02x}"));
            }
        }
        // A cord stores its first character in the low byte, so the LE bytes
        // already read left-to-right as text.
        let printable = !bytes.is_empty() && bytes.iter().all(|b| (0x20..0x7f).contains(b));
        let text = if printable {
            core::str::from_utf8(bytes).ok().map(|s| s.to_string())
        } else {
            None
        };
        let tag = text
            .as_ref()
            .filter(|t| t.len() <= 16 && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            .cloned();
        NounView::Atom {
            num,
            hex,
            text,
            tag,
            bytes: bytes.len(),
        }
    }

    fn render(noun: Noun, arena: &Arena, depth: usize, budget: &mut usize) -> NounView {
        if *budget == 0 || depth >= MAX_DEPTH {
            return NounView::Elided;
        }
        *budget -= 1;
        match noun {
            Noun::Atom(id) => render_atom(arena.atom_bytes(id)),
            Noun::Cell(_) => {
                // Walk the right spine: if it terminates in atom 0 it is a
                // proper list; render as items. Otherwise it is a pair.
                let mut items = Vec::new();
                let mut cursor = noun;
                let mut truncated = false;
                loop {
                    match cursor {
                        Noun::Cell(cid) => {
                            let cell = arena.cell(cid);
                            if items.len() >= MAX_LIST {
                                truncated = true;
                                break;
                            }
                            if *budget == 0 {
                                truncated = true;
                                break;
                            }
                            items.push(render(cell.head, arena, depth + 1, budget));
                            cursor = cell.tail;
                        }
                        Noun::Atom(aid) if arena.atom_bytes(aid).is_empty() => {
                            // Proper list terminator (~).
                            return NounView::List { items, truncated };
                        }
                        other => {
                            // Improper tail: fall back to a head/tail cell on
                            // the first element, preserving structure.
                            if items.len() == 1 {
                                return NounView::Cell {
                                    head: Box::new(items.pop().unwrap()),
                                    tail: Box::new(render(other, arena, depth + 1, budget)),
                                };
                            }
                            let tail = render(other, arena, depth + 1, budget);
                            return NounView::List {
                                items: {
                                    items.push(tail);
                                    items
                                },
                                truncated,
                            };
                        }
                    }
                }
                NounView::List { items, truncated }
            }
        }
    }

    pub fn inspect(bytes: &[u8]) -> Result<NounView, String> {
        let mut arena = Arena::new();
        let root = cue(bytes, &mut arena).map_err(|_| "not a valid jammed noun".to_string())?;
        let mut budget = MAX_NODES;
        Ok(render(root, &arena, 0, &mut budget))
    }
}

/// Cue a jammed noun and return a typed tree (atoms with number/hex/text/tag
/// heuristics, lists, cells) for display. Works on any jam.
#[wasm_bindgen]
pub fn inspect_noun(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let view = noun_inspect::inspect(bytes).map_err(|e| JsValue::from_str(&e))?;
    serde_wasm_bindgen::to_value(&view).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Review a jammed v1 draft / `.psnt` against a watch-only source pkh (base58),
/// returning the same facts the device shows: outputs (recipient, gift,
/// refund, bridge EVM address, lock primitives), per-input multisig
/// coordination, and totals. Device verifies lock-roots; this host view does
/// not.
#[wasm_bindgen]
pub fn review_draft(jam: &[u8], source_pkh_b58: &str) -> Result<JsValue, JsValue> {
    let view = review_v1::review(jam, source_pkh_b58).map_err(|e| JsValue::from_str(&e))?;
    serde_wasm_bindgen::to_value(&view).map_err(|e| JsValue::from_str(&e.to_string()))
}
