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

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
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


fn sig_hash_for_input(raw: &RawTransaction, name: &NName) -> Hash {
    let mut spend = raw
        .inputs
        .p
        .get(name)
        .expect("input missing")
        .spend
        .clone();
    spend.signature = None;
    spend.sig_hash()
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

#[derive(Serialize, Deserialize)]
struct SignatureData {
    // Must equal nname_b58(name) (i.e., "first last")
    input_name: String,
    pubkey_x: [u64; 6],
    pubkey_y: [u64; 6],
    chal: [u64; 8],
    sig: [u64; 8],
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
            let seeds_json: Vec<_> = input.spend.seeds.set.iter().enumerate().map(|(k, seed)| {
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

            let mut matching_keys = Vec::new();
            for dev_pk in &dev_keys {
                let pk_dev = SchnorrPubkey {
                    x: F6LT { values: dev_pk.x },
                    y: F6LT { values: dev_pk.y },
                    inf: false,
                };
                let dev_hash = pk_dev.to_hash();

                if lock.pubkeys.iter().any(|pk| pk.to_hash() == dev_hash) {
                    matching_keys.push(dev_hash.to_b58());
                }
            }

            if !matching_keys.is_empty() {
                web_sys::console::log_1(&format!("WASM: Found matching input").into());

                let (first, last) = nname_b58_pair(&name);
                let combined = if last.is_empty() { first.clone() } else { format!("{first} {last}") };

                // Return input info without computing sig hash
                // The hash will be computed on-device during signing
                use serde_json::json;
                let input_info = json!({
                    "name_first": first,
                    "name_last": last,
                    "input_name": combined,
                    "pubkey_hashes": matching_keys,
                    "has_spend": true,
                });

                signing_inputs.push(serde_wasm_bindgen::to_value(&input_info)?);
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
        let sigs: Vec<SignatureData> = serde_wasm_bindgen::from_value(signatures)
            .map_err(|e| JsValue::from_str(&format!("Invalid signatures: {}", e)))?;

        let raw = &mut self.inner.raw;
        let mut new_inputs: ZMap<NName, Input> = ZMap::new();

        for (name, mut input) in raw.inputs.p.tap() {
            // existing signatures or empty
            let mut sig_map: ZMap<SchnorrPubkey, SchnorrSignature> = input
                .spend
                .signature
                .as_ref()
                .map(|s| s.map.clone())
                .unwrap_or_else(ZMap::new);

            let this_name = nname_b58(&name);

            for sig_data in &sigs {
                if sig_data.input_name == this_name {
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

        raw.id = tx_types::hashing::tx_id::compute_tx_id(
            &raw.inputs,
            &raw.timelock_range,
            raw.total_fees,
        );

        Ok(())
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, JsValue> {
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

        Ok(out_bytes.to_vec())
    }
}
