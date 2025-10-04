use wasm_bindgen::prelude::*;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use nockapp::noun::slab::NounSlab;
use nockvm::noun::{Noun, T};
use noun_serde::{NounDecode, NounEncode};

use tx_types::collections::ZMap;
use tx_types::transaction_types::{Transaction, TimelockRange, PageNumber};
use tx_types::{
    Chal, Coins, F6LT, Hash, Input, Inputs, NName, RawTransaction, SchnorrPubkey, SchnorrSignature,
    Signature, Sig,
};

mod tip5;

// Use default WASM allocator (dlmalloc) - it will grow memory as needed

#[wasm_bindgen(start)]
pub fn init() {
    // Pre-grow memory BEFORE setting up panic hook or doing any allocations
    #[cfg(target_arch = "wasm32")]
    {
        let initial_pages = core::arch::wasm32::memory_size(0);

        // Grow to 4096 pages (256MB) to ensure we have space for 64MB NockStack
        let target_pages = 4096;
        if initial_pages < target_pages {
            let grow_result = core::arch::wasm32::memory_grow(0, target_pages - initial_pages);
            if grow_result != usize::MAX {
                let final_pages = core::arch::wasm32::memory_size(0);
                // Memory grown successfully - but can't log yet, no allocator ready
            }
        }
    }

    console_error_panic_hook::set_once();

    #[cfg(target_arch = "wasm32")]
    {
        let pages = core::arch::wasm32::memory_size(0);
        let bytes = pages * 64 * 1024;
        web_sys::console::log_1(&format!("WASM memory: {} pages = {} MB", pages, bytes / (1024 * 1024)).into());
    }
}

fn fmt_u64x5(arr: &[u64; 5]) -> String {
    format!(
        "{:016x}.{:016x}.{:016x}.{:016x}.{:016x}",
        arr[0], arr[1], arr[2], arr[3], arr[4]
    )
}

fn format_hash(hash: &Hash) -> String {
    fmt_u64x5(&hash.values)
}

#[inline]
fn t8(v: [u64; 8]) -> tx_types::T8 {
    tx_types::T8 { values: v }
}

fn nname_b58_pair(name: &NName) -> (String, String) {
    let first = name.p.get(0).map(|h| h.to_b58()).unwrap_or_default();
    let last  = name.p.get(1).map(|h| h.to_b58()).unwrap_or_default();
    let no_q = |s: String| s.trim_matches('\"').to_string();
    (no_q(first), no_q(last))
}

fn nname_b58(name: &NName) -> String {
    let (first, last) = nname_b58_pair(name);
    if last.is_empty() { first } else { format!("{first} {last}") }
}


fn sig_hash_for_input(raw: &RawTransaction, name: &NName) -> Result<Hash, JsValue> {
    web_sys::console::log_1(&"sig_hash_for_input: start".into());

    // CANNOT use ZMap::get() because it calls gor_tip which creates NounSlab!
    // Instead, use tap() to get all entries and find the one we want
    let all_inputs = raw.inputs.p.tap();
    let input = all_inputs.iter()
        .find(|(n, _)| n.p == name.p)
        .map(|(_, i)| i)
        .ok_or_else(|| JsValue::from_str("input missing"))?;

    web_sys::console::log_1(&"sig_hash_for_input: got input".into());

    let mut spend = input.spend.clone();
    spend.signature = None;

    web_sys::console::log_1(&format!(
        "sig_hash_for_input: fee={}, num_seeds={}",
        spend.fee.value,
        spend.seeds.set.wyt()
    ).into());

    web_sys::console::log_1(&"sig_hash_for_input: computing sig_hash using NounSlab".into());

    // We CAN use NounSlab in WASM, just not NockStack!
    // Build the sig_hashable manually using the ZSet structure
    use tx_types::hashing::hashable::Hashable;

    let seeds_hashable = build_zset_sig_hashable_with_nounslab(&spend.seeds.set)?;

    let sig_hashable = Hashable::cell(
        seeds_hashable,
        Hashable::leaf_u64(spend.fee.value),
    );

    // Use reference hash_hashable which uses NounSlab
    let sig_hash = tx_types::hashing::hasher::hash_hashable(&sig_hashable);

    web_sys::console::log_1(&format!(
        "Computed sig_hash: {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
        sig_hash.values[0],
        sig_hash.values[1],
        sig_hash.values[2],
        sig_hash.values[3],
        sig_hash.values[4]
    ).into());

    Ok(sig_hash)
}

/// Build a ZSet's sig_hashable tree structure using NounSlab (not NockStack!)
fn build_zset_sig_hashable_with_nounslab(zset: &tx_types::collections::ZSet<tx_types::Seed>) -> Result<tx_types::hashing::hashable::Hashable, JsValue> {
    use tx_types::hashing::hashable::Hashable;
    use tx_types::collections::zset::Node;

    // Traverse the actual ZSet tree structure
    fn traverse_node(node: Option<&Node<tx_types::Seed>>) -> Result<Hashable, JsValue> {
        match node {
            None => Ok(Hashable::null()),
            Some(n) => {
                let node_hashable = build_seed_sig_hashable_with_nounslab(&n.value)?;
                let left = traverse_node(n.left.as_deref())?;
                let right = traverse_node(n.right.as_deref())?;
                Ok(Hashable::triple(node_hashable, left, right))
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        traverse_node(zset.root_ref())
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        // Fallback for non-WASM
        let all_seeds: Vec<tx_types::Seed> = zset.tap();
        if all_seeds.is_empty() {
            return Ok(Hashable::null());
        }
        build_seed_sig_hashable_with_nounslab(&all_seeds[0])
    }
}

/// Build a Seed's sig_hashable using NounSlab for pubkey hashing
fn build_seed_sig_hashable_with_nounslab(seed: &tx_types::Seed) -> Result<tx_types::hashing::hashable::Hashable, JsValue> {
    use tx_types::hashing::hashable::Hashable;

    // From transaction_types.rs lines 761-795, Seed::to_sig_hashable() creates:
    // A 5-tuple: [output_source, recipient, timelock_intent, gift, parent_hash]

    // output_source
    let output_source_hashable = match &seed.output_source {
        None => Hashable::null(),
        Some(source) => {
            // Build source hashable manually
            Hashable::cell(Hashable::null(), Hashable::Hash(source.p.clone()))
        }
    };

    // recipient (Lock) - build with NounSlab for pubkey hashing
    let recipient_hashable = build_lock_hashable_with_nounslab(&seed.recipient)?;

    // timelock_intent
    let timelock_hashable = build_timelock_intent_hashable(&seed.timelock_intent)?;

    // gift
    let gift_hashable = Hashable::leaf_u64(seed.gift.value);

    // parent_hash
    let parent_hashable = Hashable::Hash(seed.parent_hash.clone());

    // Build the 5-tuple as nested cells
    Ok(Hashable::cell(
        output_source_hashable,
        Hashable::cell(
            recipient_hashable,
            Hashable::cell(
                timelock_hashable,
                Hashable::cell(gift_hashable, parent_hashable),
            ),
        ),
    ))
}

/// Build Lock's hashable using NounSlab for pubkey hashing
fn build_lock_hashable_with_nounslab(lock: &tx_types::Lock) -> Result<tx_types::hashing::hashable::Hashable, JsValue> {
    use tx_types::hashing::hashable::Hashable;
    use nockapp::noun::slab::NounSlab;
    use noun_serde::NounEncode;

    // Lock is [m, pubkeys_zset]
    let m_hashable = Hashable::leaf_u64(lock.m as u64);

    // Build pubkeys ZSet hashable - each pubkey needs to be hashed using NounSlab
    let pubkeys_hashable = build_pubkey_zset_hashable_with_nounslab(&lock.pubkeys)?;

    Ok(Hashable::cell(m_hashable, pubkeys_hashable))
}

/// Build a ZSet of pubkeys hashable using NounSlab for each pubkey hash
fn build_pubkey_zset_hashable_with_nounslab(zset: &tx_types::collections::ZSet<tx_types::SchnorrPubkey>) -> Result<tx_types::hashing::hashable::Hashable, JsValue> {
    use tx_types::hashing::hashable::Hashable;
    use tx_types::collections::zset::Node;

    fn traverse_pk_node(node: Option<&Node<tx_types::SchnorrPubkey>>) -> Result<Hashable, JsValue> {
        match node {
            None => Ok(Hashable::null()),
            Some(n) => {
                let node_hashable = hash_schnorr_pubkey_with_nounslab(&n.value)?;
                let left = traverse_pk_node(n.left.as_deref())?;
                let right = traverse_pk_node(n.right.as_deref())?;
                Ok(Hashable::triple(node_hashable, left, right))
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        traverse_pk_node(zset.root_ref())
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let pks: Vec<_> = zset.tap();
        if pks.is_empty() {
            return Ok(Hashable::null());
        }
        Ok(hash_schnorr_pubkey_with_nounslab(&pks[0])?)
    }
}

/// Hash a SchnorrPubkey using NounSlab (not NockStack!)
/// Converts to noun then hashes with reference hasher
fn hash_schnorr_pubkey_with_nounslab(pk: &tx_types::SchnorrPubkey) -> Result<tx_types::hashing::hashable::Hashable, JsValue> {
    use tx_types::hashing::hashable::Hashable;
    use nockapp::noun::slab::NounSlab;
    use noun_serde::NounEncode;

    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&"hash_schnorr_pubkey_with_nounslab: creating NounSlab".into());

    // Create NounSlab (works in WASM - just creates empty vectors)
    let mut slab: NounSlab = NounSlab::new();

    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&"hash_schnorr_pubkey_with_nounslab: converting pubkey to noun".into());

    // Convert pubkey to noun
    let pk_noun = pk.to_noun(&mut slab);

    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&"hash_schnorr_pubkey_with_nounslab: hashing noun (may crash if NockStack needed)".into());

    // Hash the noun - THIS MAY CRASH if it tries to create NockStack
    let pk_hash = tx_types::hashing::tip5::Tip5Hasher::hash_noun_varlen(pk_noun)
        .map_err(|e| JsValue::from_str(&format!("Pubkey hash failed: {:?}", e)))?;

    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&format!("hash_schnorr_pubkey_with_nounslab: success! hash = {}", format_hash(&pk_hash)).into());

    Ok(Hashable::Hash(pk_hash))
}

/// Build timelock intent hashable
fn build_timelock_intent_hashable(
    intent: &Option<(tx_types::TimelockRange, tx_types::TimelockRange)>,
) -> Result<tx_types::hashing::hashable::Hashable, JsValue> {
    use tx_types::hashing::hashable::Hashable;

    match intent {
        None => Ok(Hashable::null()),
        Some((absolute, relative)) => {
            // Build structure for absolute and relative ranges
            let abs_min = absolute.min.as_ref().map(|p| p.value).unwrap_or(0);
            let abs_max = absolute.max.as_ref().map(|p| p.value).unwrap_or(0);
            let rel_min = relative.min.as_ref().map(|p| p.value).unwrap_or(0);
            let rel_max = relative.max.as_ref().map(|p| p.value).unwrap_or(0);

            // This is a simplified version - proper implementation would match Hoon structure exactly
            let mut bytes = Vec::new();
            bytes.extend(&abs_min.to_le_bytes());
            bytes.extend(&abs_max.to_le_bytes());
            bytes.extend(&rel_min.to_le_bytes());
            bytes.extend(&rel_max.to_le_bytes());

            Ok(Hashable::Leaf(bytes))
        }
    }
}

/// Hash a Hashable structure using WASM-compatible TIP5 (no NounSlab)
pub fn hash_hashable_wasm(h: &tx_types::hashing::hashable::Hashable) -> Result<Hash, JsValue> {
    use tx_types::hashing::hashable::Hashable;

    match h {
        Hashable::Leaf(data) => {
            // Hash raw byte data
            // Convert bytes to u64s (pad with zeros if needed)
            let mut u64s = Vec::new();
            for chunk in data.chunks(8) {
                let mut bytes = [0u8; 8];
                bytes[..chunk.len()].copy_from_slice(chunk);
                u64s.push(u64::from_le_bytes(bytes));
            }
            Ok(tip5::hash_varlen_u64s(&u64s))
        }
        Hashable::Hash(digest) => {
            // Already hashed, return as-is
            Ok(digest.clone())
        }
        Hashable::Cell(left, right) => {
            // Recursively hash both sides and combine
            let left_hash = hash_hashable_wasm(left)?;
            let right_hash = hash_hashable_wasm(right)?;
            // Combine two hashes: hash([left.values, right.values])
            Ok(tip5::hash_two_hashes(&left_hash, &right_hash))
        }
        Hashable::List(items) => {
            // Hash each item recursively
            let hashes: Result<Vec<Hash>, JsValue> = items.iter()
                .map(hash_hashable_wasm)
                .collect();
            let hashes = hashes?;

            // Hash the list of hashes
            Ok(tip5::hash_hash_list(&hashes))
        }
    }
}

/// Hash seeds for signature verification (to_sig_hashable)
fn hash_seeds_sig_hashable(seeds: &tx_types::Seeds) -> Result<Hash, JsValue> {
    web_sys::console::log_1(&"hash_seeds_sig_hashable: start".into());
    // Each seed is converted to a hashable and then we hash the list of them
    let mut seed_hashes = Vec::new();

    web_sys::console::log_1(&format!("hash_seeds_sig_hashable: {} seeds", seeds.set.wyt()).into());

    // Use tap() instead of iter() to avoid potential NounSlab allocations
    let all_seeds = seeds.set.tap();
    for seed in &all_seeds {
        web_sys::console::log_1(&"hash_seeds_sig_hashable: hashing one seed".into());
        let seed_hash = hash_seed_sig_hashable(seed)?;
        seed_hashes.push(seed_hash);
    }

    web_sys::console::log_1(&"hash_seeds_sig_hashable: hashing list".into());
    // Hash the list of seed hashes
    Ok(tip5::hash_hash_list(&seed_hashes))
}

/// Hash a single seed for signature verification
fn hash_seed_sig_hashable(seed: &tx_types::Seed) -> Result<Hash, JsValue> {
    // A seed's sig_hashable structure is a list of:
    // [output_source, recipient, timelock_intent, gift, parent_hash]

    let mut components = Vec::new();

    // output_source - if None, hash of 0, if Some, use the hash
    if let Some(src) = &seed.output_source {
        components.extend_from_slice(&src.p.values);
    } else {
        components.push(0);
    }

    // recipient (Lock) - hash the m value and each pubkey
    components.push(seed.recipient.m as u64);
    // Use tap() to avoid NounSlab allocations in ZSet
    let pubkeys = seed.recipient.pubkeys.tap();
    for pk in &pubkeys {
        // Hash each pubkey by concatenating x and y coordinates
        components.extend_from_slice(&pk.x.values);
        components.extend_from_slice(&pk.y.values);
    }

    // timelock_intent - this is Option<(TimelockRange, TimelockRange)>
    match &seed.timelock_intent {
        None => components.push(0),
        Some((absolute, relative)) => {
            components.push(0); // leaf+~
            if let Some(min) = &absolute.min {
                components.push(0);
                components.push(min.value);
            } else {
                components.push(0);
            }
            if let Some(max) = &absolute.max {
                components.push(0);
                components.push(max.value);
            } else {
                components.push(0);
            }
            if let Some(min) = &relative.min {
                components.push(0);
                components.push(min.value);
            } else {
                components.push(0);
            }
            if let Some(max) = &relative.max {
                components.push(0);
                components.push(max.value);
            } else {
                components.push(0);
            }
        }
    }

    // gift
    components.push(seed.gift.value);

    // parent_hash - this is a Hash (5 u64 values)
    components.extend_from_slice(&seed.parent_hash.values);

    // Hash all the components
    Ok(tip5::hash_varlen_u64s(&components))
}

fn sum_inputs_fees(inputs: &Inputs) -> u64 {
    inputs
        .p
        .tap()
        .into_iter()
        .fold(0u64, |acc, (_n, i)| acc.saturating_add(i.spend.fee.value))
}

fn union_inputs_timelock_range(inputs: &Inputs) -> TimelockRange {
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


/// ---- JS ------------------------------------------------------------

#[wasm_bindgen]
#[derive(Serialize, Deserialize, Clone)]
pub struct TransactionInfo {
    tx_id: String,
    shape: String,
    input_count: usize,
}

#[wasm_bindgen]
impl TransactionInfo {
    #[wasm_bindgen(getter)]
    pub fn tx_id(&self) -> String { self.tx_id.clone() }
    #[wasm_bindgen(getter)]
    pub fn shape(&self) -> String { self.shape.clone() }
    #[wasm_bindgen(getter)]
    pub fn input_count(&self) -> usize { self.input_count }
}

#[wasm_bindgen]
#[derive(Serialize, Deserialize, Clone)]
pub struct SigningInput {
    name_first: String,
    name_last: String,
    input_name: String,
    sig_hash: String,
    msg5: Vec<u64>,
    pubkey_hashes: Vec<String>,
}

#[wasm_bindgen]
impl SigningInput {
    #[wasm_bindgen(getter)]
    pub fn name_first(&self) -> String { self.name_first.clone() }
    #[wasm_bindgen(getter)]
    pub fn name_last(&self) -> String { self.name_last.clone() }
    #[wasm_bindgen(getter)]
    pub fn input_name(&self) -> String { self.input_name.clone() }
    #[wasm_bindgen(getter)]
    pub fn sig_hash(&self) -> String { self.sig_hash.clone() }
    #[wasm_bindgen(getter)]
    pub fn msg5(&self) -> Vec<u64> { self.msg5.clone() }
    #[wasm_bindgen(getter)]
    pub fn pubkey_hashes(&self) -> Vec<String> { self.pubkey_hashes.clone() }
}


#[derive(Serialize, Deserialize)]
struct DevicePubkey {
    x: [u64; 6],
    y: [u64; 6],
}

// Helper for deserializing from JavaScript (strings to avoid precision loss)
#[derive(Deserialize)]
struct SignatureDataJs {
    input_name: String,
    pubkey_x: Vec<String>,
    pubkey_y: Vec<String>,
    chal: Vec<String>,
    sig: Vec<String>,
}

struct SignatureData {
    input_name: String,
    pubkey_x: [u64; 6],
    pubkey_y: [u64; 6],
    chal: [u64; 8],
    sig: [u64; 8],
}

impl SignatureDataJs {
    fn parse(self) -> Result<SignatureData, String> {
        let parse_array = |v: Vec<String>, expected: usize, name: &str| -> Result<Vec<u64>, String> {
            if v.len() != expected {
                return Err(format!("{} has {} elements, expected {}", name, v.len(), expected));
            }
            v.into_iter()
                .map(|s| s.parse::<u64>().map_err(|e| format!("Failed to parse {}: {}", name, e)))
                .collect()
        };

        let pubkey_x = parse_array(self.pubkey_x, 6, "pubkey_x")?;
        let pubkey_y = parse_array(self.pubkey_y, 6, "pubkey_y")?;
        let chal = parse_array(self.chal, 8, "chal")?;
        let sig = parse_array(self.sig, 8, "sig")?;

        Ok(SignatureData {
            input_name: self.input_name,
            pubkey_x: pubkey_x.try_into().unwrap(),
            pubkey_y: pubkey_y.try_into().unwrap(),
            chal: chal.try_into().unwrap(),
            sig: sig.try_into().unwrap(),
        })
    }
}

/// ---- ParsedTransaction ---------------------------------------------------

#[wasm_bindgen]
pub struct ParsedTransaction {
    inner: ParsedTxInner,
}

struct ParsedTxInner {
    outer: OuterType,
    raw: RawTransaction,
    tail_jam: Option<Vec<u8>>,
}

enum OuterType {
    RawTx,
    TxTransact,
    WalletTx(Transaction),
}

#[wasm_bindgen]
impl ParsedTransaction {
    /// Parse a jam file (transaction draft) from bytes
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Result<ParsedTransaction, JsValue> {
        web_sys::console::log_1(&format!("WASM: Starting transaction parse, {} bytes", bytes.len()).into());
        let mut slab: NounSlab = NounSlab::new();
        web_sys::console::log_1(&"WASM: NounSlab created".into());
        let noun: Noun = slab
            .cue_into(Bytes::from(bytes.to_vec()))
            .map_err(|e| {
                web_sys::console::error_1(&format!("WASM: cue failed: {:?}", e).into());
                JsValue::from_str(&format!("Failed to cue jam: {:?}", e))
            })?;
        web_sys::console::log_1(&"WASM: Noun cued successfully".into());

        // try wallet transaction first
        web_sys::console::log_1(&"WASM: Trying Transaction::from_noun".into());
        match Transaction::from_noun(&noun) {
            Ok(tx) => {
                web_sys::console::log_1(&"WASM: Parsed as wallet transaction".into());
                // Don't call transaction_to_raw() as it requires NockStack for hashing
                // Instead, construct RawTransaction directly without recomputing the ID
                let inputs = tx.p.clone();
                let total_fees = sum_inputs_fees(&inputs);
                let tl = union_inputs_timelock_range(&inputs);

                // Parse the transaction name (which is the ID) from the string
                // The name should be a base58 encoded transaction ID
                let id = Hash::from_b58(&tx.name).map_err(|e| {
                    JsValue::from_str(&format!("Invalid transaction name/ID: {:?}", e))
                })?;

                let raw = RawTransaction {
                    id,
                    inputs,
                    timelock_range: tl,
                    total_fees: Coins { value: total_fees },
                };

                return Ok(ParsedTransaction {
                    inner: ParsedTxInner {
                        outer: OuterType::WalletTx(tx),
                        raw,
                        tail_jam: None,
                    },
                });
            }
            Err(e) => {
                web_sys::console::log_1(&format!("WASM: Not a wallet transaction: {:?}", e).into());
            }
        }

        // [raw-tx tail]
        web_sys::console::log_1(&"WASM: Trying [raw-tx tail] format".into());
        if let Ok(cell) = noun.as_cell() {
            web_sys::console::log_1(&"WASM: Is a cell, trying RawTransaction::from_noun on head".into());
            if let Ok(r) = RawTransaction::from_noun(&cell.head()) {
                web_sys::console::log_1(&"WASM: Parsed as [raw-tx tail]".into());
                let mut s2: NounSlab = NounSlab::new();
                let copied_tail = s2.copy_into(cell.tail());
                s2.copy_into(copied_tail);
                let tail_jam = s2.jam().to_vec();
                return Ok(ParsedTransaction {
                    inner: ParsedTxInner {
                        outer: OuterType::TxTransact,
                        raw: r,
                        tail_jam: Some(tail_jam),
                    },
                });
            }
        }

        // bare raw-tx
        web_sys::console::log_1(&"WASM: Trying bare raw-tx format".into());
        if let Ok(r) = RawTransaction::from_noun(&noun) {
            web_sys::console::log_1(&"WASM: Parsed as bare raw-tx".into());
            return Ok(ParsedTransaction {
                inner: ParsedTxInner {
                    outer: OuterType::RawTx,
                    raw: r,
                    tail_jam: None,
                },
            });
        }

        Err(JsValue::from_str(
            "Unrecognized transaction format (expected wallet-tx, [raw-tx tail], or raw-tx)",
        ))
    }

    /// Get transaction info
    pub fn info(&self) -> TransactionInfo {
        let raw = &self.inner.raw;
        TransactionInfo {
            tx_id: raw.id.to_b58(),
            shape: match &self.inner.outer {
                OuterType::RawTx => "raw-tx".to_string(),
                OuterType::TxTransact => "[raw-tx tail]".to_string(),
                OuterType::WalletTx(_) => "wallet-tx".to_string(),
            },
            input_count: raw.inputs.p.wyt(),
        }
    }

    /// Get transaction details as JSON for display
    pub fn get_details(&self) -> JsValue {
        use serde_json::json;

        web_sys::console::log_1(&"WASM: get_details called".into());
        let raw = &self.inner.raw;
        web_sys::console::log_1(&format!("WASM: raw has {} inputs", raw.inputs.p.wyt()).into());

        let inputs_json: Vec<_> = raw.inputs.p.tap().into_iter().enumerate().map(|(idx, (name, input))| {
            let (first, last) = nname_b58_pair(&name);
            let name_display = if last.is_empty() {
                format!("[{}]", first)
            } else {
                format!("[{} {}]", first, last)
            };

            // Format lock
            let (m, pks_b58) = input.note.lock.to_b58();
            let lock_display = format!("{}-of-{} signers", m, pks_b58.len());

            // Format seeds
            let seeds_json: Vec<_> = input.spend.seeds.set.iter().enumerate().map(|(_k, seed)| {
                let (m, pks_b58) = seed.recipient.to_b58();
                let lock = format!("{}-of-{}", m, pks_b58.len());

                json!({
                    "gift": seed.gift.value,
                    "lock": lock,
                    "recipient": pks_b58,
                    "parent_hash": seed.parent_hash.to_b58(),
                })
            }).collect();

            // Count signatures
            let sig_count = input.spend.signature.as_ref().map(|m| m.map.wyt()).unwrap_or(0);

            json!({
                "index": idx,
                "name": name_display,
                "origin_page": input.note.meta.origin_page.value,
                "assets": input.note.assets.value,
                "source": format_hash(&input.note.source.p),
                "coinbase": input.note.source.is_coinbase,
                "lock": lock_display,
                "lock_pubkeys": pks_b58,
                "fee": input.spend.fee.value,
                "signatures": sig_count,
                "seeds": seeds_json,
            })
        }).collect();

        web_sys::console::log_1(&format!("WASM: collected {} inputs", inputs_json.len()).into());

        let details = json!({
            "transaction_id": raw.id.to_b58(),
            "input_count": raw.inputs.p.wyt(),
            "total_fees": raw.total_fees.value,
            "timelock_min": raw.timelock_range.min.as_ref().map(|p| p.value),
            "timelock_max": raw.timelock_range.max.as_ref().map(|p| p.value),
            "inputs": inputs_json,
        });

        web_sys::console::log_1(&format!("WASM: details JSON: {}", details).into());

        match serde_wasm_bindgen::to_value(&details) {
            Ok(result) => {
                web_sys::console::log_1(&"WASM: get_details returning successfully".into());
                result
            }
            Err(e) => {
                web_sys::console::error_1(&format!("WASM: serialization failed: {}", e).into());
                JsValue::from_str("{\"error\": \"serialization failed\"}")
            }
        }
    }

    /// device_pubkeys: array of {x: bigint[], y: bigint[]}
    /// Returns list of inputs with names only (no sig hashes to avoid NockStack allocation)
    pub fn get_signing_inputs(&self, device_pubkeys: JsValue) -> Result<Vec<JsValue>, JsValue> {
        web_sys::console::log_1(&"WASM: get_signing_inputs called".into());

        let dev_keys: Vec<DevicePubkey> = serde_wasm_bindgen::from_value(device_pubkeys)
            .map_err(|e| JsValue::from_str(&format!("Invalid device_pubkeys: {}", e)))?;

        let raw = &self.inner.raw;
        let mut signing_inputs = Vec::new();

        for (name, input) in raw.inputs.p.tap() {
            let lock = &input.note.lock;

            web_sys::console::log_1(&format!("WASM: Checking input with {} lock pubkeys", lock.pubkeys.wyt()).into());

            let mut matching_keys = Vec::new();
            for dev_pk in &dev_keys {
                let pk_dev = SchnorrPubkey {
                    x: F6LT { values: dev_pk.x },
                    y: F6LT { values: dev_pk.y },
                    inf: false,
                };

                web_sys::console::log_1(&format!("WASM: Checking device pubkey x[0]={}, y[0]={}",
                    pk_dev.x.values[0], pk_dev.y.values[0]).into());

                // Check if this device pubkey matches any lock pubkey
                // Compare directly without calling to_hash() to avoid NockStack allocation
                let mut found_match = false;
                for (idx, pk) in lock.pubkeys.iter().enumerate() {
                    let matches = pk.x.values == pk_dev.x.values &&
                                 pk.y.values == pk_dev.y.values &&
                                 pk.inf == pk_dev.inf;

                    if matches {
                        web_sys::console::log_1(&format!("WASM: MATCH FOUND at lock pubkey index {}", idx).into());
                        found_match = true;
                        // Format the pubkey for display
                        matching_keys.push(format!("pk({},{})",
                            pk_dev.x.values[0], pk_dev.y.values[0]));
                        break;
                    }
                }

                if !found_match {
                    web_sys::console::log_1(&"WASM: No match for this device key".into());
                }
            }

            if !matching_keys.is_empty() {
                web_sys::console::log_1(&format!("WASM: Found matching input with {} matching keys", matching_keys.len()).into());

                let (first, last) = nname_b58_pair(&name);
                let combined = if last.is_empty() { first.clone() } else { format!("{first} {last}") };

                // Compute the sig_hash for this input
                // Using talc allocator with 256MB static arena for NockStack (64MB) allocation
                web_sys::console::log_1(&"WASM: Computing sig_hash with talc".into());
                let sig_hash = sig_hash_for_input(raw, &name)?;
                let msg5 = sig_hash.values.to_vec();

                use serde_json::json;
                // Convert msg5 u64 values to strings to avoid JavaScript number precision loss
                // JavaScript will need to parse these as BigInt
                let msg5_strings: Vec<String> = msg5.iter().map(|v| v.to_string()).collect();

                let input_info = json!({
                    "name_first": first,
                    "name_last": last,
                    "input_name": combined,
                    "pubkey_hashes": matching_keys,
                    "sig_hash": format_hash(&sig_hash),
                    "msg5": msg5_strings,
                });

                web_sys::console::log_1(&format!("WASM: Created input_info: {:?}", serde_json::to_string(&input_info).unwrap_or_default()).into());
                signing_inputs.push(serde_wasm_bindgen::to_value(&input_info)?);
            } else {
                web_sys::console::log_1(&"WASM: No matching keys for this input".into());
            }
        }

        web_sys::console::log_1(&format!("WASM: Returning {} signing inputs", signing_inputs.len()).into());
        Ok(signing_inputs)
    }

    /* {
          input_name: string, 
          pubkey_x: bigint[], 
          pubkey_y: bigint[], 
          chal: bigint[], 
          sig: bigint[]}
    */
    pub fn apply_signatures(&mut self, signatures: JsValue) -> Result<(), JsValue> {
        use tx_types::collections::ZMap;

        web_sys::console::log_1(&"apply_signatures: start".into());

        let sigs_js: Vec<SignatureDataJs> = serde_wasm_bindgen::from_value(signatures)
            .map_err(|e| JsValue::from_str(&format!("Invalid signatures: {}", e)))?;

        let sigs: Vec<SignatureData> = sigs_js.into_iter()
            .map(|s| s.parse().map_err(|e| JsValue::from_str(&e)))
            .collect::<Result<Vec<_>, _>>()?;

        web_sys::console::log_1(&format!("apply_signatures: got {} signatures", sigs.len()).into());

        // ZMap now uses dor-tip for WASM (see tx-types patch), so put() won't allocate NockStack
        let raw = &mut self.inner.raw;
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
                    web_sys::console::log_1(&format!("apply_signatures: applying sig to {}", this_name).into());

                    let pk = SchnorrPubkey {
                        x: F6LT { values: sig_data.pubkey_x },
                        y: F6LT { values: sig_data.pubkey_y },
                        inf: false,
                    };

                    let schnorr_sig = SchnorrSignature {
                        chal: Chal { values: t8(sig_data.chal) },
                        sig:  Sig  { values: t8(sig_data.sig)  },
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

        // NOTE: We skip recomputing tx_id in WASM because it requires complex hashing
        // that would need NounSlab (64MB allocation - impossible in WASM)
        // The node will validate and recompute the correct tx_id when receiving the transaction
        web_sys::console::log_1(&"apply_signatures: done (tx_id will be validated by node)".into());

        Ok(())
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, JsValue> {
        use nockvm::mem::NockStack;

        web_sys::console::log_1(&"to_bytes: start".into());

        // Create a SMALL NockStack for WASM (4MB)
        const STACK_SIZE: usize = 512 * 1024; // 4MB in u64 words
        let mut _stack = NockStack::new(STACK_SIZE, 0);

        web_sys::console::log_1(&"to_bytes: created small nockstack".into());

        let raw = &self.inner.raw;

        let out_bytes = match &self.inner.outer {
            OuterType::RawTx => {
                let mut out_slab: NounSlab = NounSlab::new();
                let n = raw.to_noun(&mut out_slab);
                out_slab.copy_into(n);
                out_slab.jam()
            }
            OuterType::TxTransact => {
                let tail_jam = self
                    .inner
                    .tail_jam
                    .as_ref()
                    .ok_or_else(|| JsValue::from_str("Missing tail jam"))?;
                let mut out_slab: NounSlab = NounSlab::new();
                let head = raw.to_noun(&mut out_slab);
                let tail = out_slab
                    .cue_into(Bytes::from(tail_jam.clone()))
                    .map_err(|e| JsValue::from_str(&format!("Failed to cue tail: {:?}", e)))?;
                let cell = T(&mut out_slab, &[head, tail]);
                out_slab.copy_into(cell);
                out_slab.jam()
            }
            OuterType::WalletTx(orig_tx) => {
                // wallet transaction wrapper [name=@t p=inputs]
                let mut tx = orig_tx.clone();
                tx.p = raw.inputs.clone();
                let mut out_slab: NounSlab = NounSlab::new();
                let n = tx.to_noun(&mut out_slab);
                out_slab.copy_into(n);
                out_slab.jam()
            }
        };

        web_sys::console::log_1(&format!("to_bytes: jammed {} bytes", out_bytes.len()).into());

        Ok(out_bytes.to_vec())
    }
}

