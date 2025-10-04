use wasm_bindgen::prelude::*;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use nockapp::noun::slab::NounSlab;
use nockvm::noun::{Noun, T};
use noun_serde::{NounDecode, NounEncode};

use tx_types::collections::ZMap;
use tx_types::{
    Chal, F6LT, Hash, Input, Inputs, NName, RawTransaction, SchnorrPubkey, SchnorrSignature,
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
}

#[wasm_bindgen]
impl ParsedTransaction {
    /// Parse a jam file (transaction draft) from bytes
    #[wasm_bindgen(constructor)]
    pub fn new(bytes: &[u8]) -> Result<ParsedTransaction, JsValue> {
        let mut slab: NounSlab = NounSlab::new();
        let noun: Noun = slab
            .cue_into(Bytes::from(bytes.to_vec()))
            .map_err(|e| JsValue::from_str(&format!("Failed to cue jam: {:?}", e)))?;

        // [raw-tx tail]
        if let Ok(cell) = noun.as_cell() {
            if let Ok(r) = RawTransaction::from_noun(&cell.head()) {
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
        if let Ok(r) = RawTransaction::from_noun(&noun) {
            return Ok(ParsedTransaction {
                inner: ParsedTxInner {
                    outer: OuterType::RawTx,
                    raw: r,
                    tail_jam: None,
                },
            });
        }

        Err(JsValue::from_str(
            "Unrecognized transaction format (expected [raw-tx tail] or raw-tx)",
        ))
    }

    /// Get transaction info
    pub fn info(&self) -> TransactionInfo {
        let raw = &self.inner.raw;
        TransactionInfo {
            tx_id: raw.id.to_b58(),
            shape: match self.inner.outer {
                OuterType::RawTx => "raw-tx".to_string(),
                OuterType::TxTransact => "[raw-tx tail]".to_string(),
            },
            input_count: raw.inputs.p.wyt(),
        }
    }

    /// device_pubkeys: array of {x: bigint[], y: bigint[]}
    pub fn get_signing_inputs(&self, device_pubkeys: JsValue) -> Result<Vec<JsValue>, JsValue> {
        let dev_keys: Vec<DevicePubkey> = serde_wasm_bindgen::from_value(device_pubkeys)
            .map_err(|e| JsValue::from_str(&format!("Invalid device_pubkeys: {}", e)))?;

        let raw = &self.inner.raw;
        let mut signing_inputs = Vec::new();

        for (name, input) in raw.inputs.p.tap() {
            let lock = &input.note.lock;
            let msg_hash = sig_hash_for_input(raw, &name);
            let msg5: [u64; 5] = msg_hash.values;

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
              let (first, last) = nname_b58_pair(&name);
              let combined = if last.is_empty() { first.clone() } else { format!("{first} {last}") };

              let si = SigningInput {
                  name_first: first,
                  name_last: last,
                  input_name: combined,
                  sig_hash: fmt_u64x5(&msg5),
                  msg5: msg5.to_vec(),
                  pubkey_hashes: matching_keys,
              };
                signing_inputs.push(serde_wasm_bindgen::to_value(&si)?);
            }
        }

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
        };

        Ok(out_bytes.to_vec())
    }
}
