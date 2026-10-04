mod wallet;
mod js_error;
mod tx;
mod tx_v1;

#[cfg(test)]
mod tests;

use wasm_bindgen::{
    prelude::*,
};
use js_sys;
use log::*;
use crate::js_error::*;
use crate::tx_v1::{create_transaction_v1, sign_transaction_v1};
use nockblocks_api::models::{NoteObject as ApiNote, Timelocks as ApiTimelocks, TimelockRange as ApiTlRange, Lock as ApiLock};
use tx_types::transaction_types::{NNote, Lock as TxLock, Transaction};
use nockapp::noun::slab::NounSlab;
use noun_serde::{NounEncode, NounDecode};

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Debug).unwrap(); // set once
    info!("wasm loaded");
}

// wallet Management
// Ideally, we want to never expose the private key etc to javascript besides
// that one time when we create the wallet
// so we should probably also always take a password + encrypted vault from the user 
// which will be used to enc/dec the individual wallets

/// New wallet from random seed
#[wasm_bindgen(js_name = createWallet)]
pub fn create_wallet() -> Result<JsValue, JsValue> {
    wallet::Wallet::random_wallet()?.to_js()
}

/// Import wallet from seed phrase
#[wasm_bindgen(js_name = createWalletFromSeedPhrase)]
pub fn create_wallet_from_seed_phrase(seed_phrase: String) -> Result<JsValue, JsValue> {
    wallet::Wallet::from_seed_phrase(seed_phrase)?.to_js()
}

/// Core function to import wallet from jam data (Rust-only, no WASM bindings)
pub fn import_wallet_core(jam_data: &[u8]) -> Result<wallet::Wallet, String> {
    wallet::Wallet::import_from_jam(jam_data)
}

/// Import wallet from zorp's exported wallet jam (WASM wrapper)
#[wasm_bindgen(js_name = importWalletFile)]
pub fn import_wallet_file(jam_data: &[u8]) -> Result<JsValue, JsValue> {
    let wallet = import_wallet_core(jam_data)
        .map_err(|e| js_error(&e))?;
    wallet.to_js()
}

/// Export wallet to zorp's exported wallet jam (WASM wrapper)
#[wasm_bindgen(js_name = exportWalletFile)]
pub fn export_wallet_file(wallet_js: JsValue) -> Result<JsValue, JsValue> {
    info!("Exporting wallet to jam file");

    // Deserialize the wallet from JavaScript
    let wallet: wallet::Wallet = serde_wasm_bindgen::from_value(wallet_js)
        .map_err(|e| js_error(&format!("Failed to deserialize wallet: {}", e)))?;

    // Export wallet to jam data
    let jammed_bytes = wallet.export_to_jam()
        .map_err(|e| js_error(&e))?;

    // Convert to JavaScript Uint8Array
    //TODO I have no idea if this is the right thing. even if it is, because the current wallet
    // noun-serde implementation is "simplified" with u64 keys, chain code, etc, it will probably
    // need to be updated to work with actual keys etc later.
    let js_array = js_sys::Uint8Array::new_with_length(jammed_bytes.len() as u32);
    js_array.copy_from(&jammed_bytes);

    Ok(js_array.into())
}

// Transaction Management

/// Create a new transaction (WASM wrapper) - V1 version
///
/// Note: Since NNote doesn't implement WASM traits, we expect JavaScript to pass
/// notes and fees as separate jam-encoded byte arrays
#[wasm_bindgen(js_name = createTransaction)]
pub fn create_transaction(
    refund_address: String,
    notes_js: JsValue,
    _fees_js: JsValue,
    recipients_js: JsValue,
) -> Result<JsValue, JsValue> {
    info!("Creating V1 transaction from JavaScript");

    // 1) notes from JS
    let api_notes: Vec<ApiNote> = serde_wasm_bindgen::from_value(notes_js)
        .map_err(|e| js_error(&format!("Failed to deserialize notes: {}", e)))?;
    info!("Received {} notes from JS", api_notes.len());

    // 2) recipients
    let recipients_array: js_sys::Array = recipients_js.dyn_into()
        .map_err(|_| js_error("Expected array of recipient arrays"))?;
    let mut recipients: Vec<(String, u64)> = Vec::with_capacity(recipients_array.length() as usize);
    let mut total_send: u128 = 0;
    for item in recipients_array.iter() {
        let arr: js_sys::Array = item.dyn_into()
            .map_err(|_| js_error("Each recipient must be [address, amount]"))?;
        if arr.length() != 2 {
            return Err(js_error("Recipient array must be exactly 2 elements: [address, amount]"));
        }
        let address = arr.get(0).as_string()
            .ok_or_else(|| js_error("Recipient address must be a string"))?;
        let amount_u64 = arr.get(1).as_f64()
            .ok_or_else(|| js_error("Recipient amount must be a number"))? as u64;
        total_send = total_send.saturating_add(amount_u64 as u128);
        recipients.push((address, amount_u64));
    }

    // 3) Convert API notes to NNote and collect locks
    let mut notes_with_fees_and_locks: Vec<(NNote, u64, Option<TxLock>)> = Vec::new();

    for api_note in &api_notes {
        let (note, lock) = api_note_to_nnote_v1(api_note)
            .map_err(|e| js_error(&format!("Failed to convert note: {}", e)))?;

        // For now, use fee of 0 for all notes
        // V1 notes need the lock provided separately since it's not stored in the note
        let lock_option = match &note {
            NNote::V0(_) => None, // V0 notes have lock embedded
            NNote::V1(_) => Some(lock), // V1 notes need lock provided
        };

        notes_with_fees_and_locks.push((note, 0, lock_option));
    }

    // 4) Call V1 core function
    let transaction = create_transaction_v1(refund_address, notes_with_fees_and_locks, recipients)
        .map_err(|e| js_error(&e))?;

    // 5) Serialize transaction to JAM format
    let transaction_bytes = jam_transaction(&transaction)
        .map_err(|e| js_error(&format!("Failed to serialize transaction: {}", e)))?;

    // 6) Return Uint8Array
    let js_array = js_sys::Uint8Array::new_with_length(transaction_bytes.len() as u32);
    js_array.copy_from(&transaction_bytes);
    Ok(js_array.into())
}

/// Sign a transaction (WASM wrapper) - V1 version
#[wasm_bindgen(js_name = signTransaction)]
pub fn sign_transaction(transaction_jam: &[u8], wallet_js: JsValue) -> Result<JsValue, JsValue> {
    info!("Signing V1 transaction from JavaScript");

    // Deserialize the wallet from JavaScript
    let wallet: wallet::Wallet = serde_wasm_bindgen::from_value(wallet_js)
        .map_err(|e| js_error(&format!("Failed to deserialize wallet: {}", e)))?;

    // Deserialize transaction from JAM
    let transaction = unjam_transaction(transaction_jam)
        .map_err(|e| js_error(&format!("Failed to deserialize transaction: {}", e)))?;

    // Get wallet keys
    let (privkey, pubkey) = wallet.get_keys()
        .map_err(|e| js_error(&format!("Failed to get wallet keys: {}", e)))?;

    // Sign using V1 core function
    let signed_transaction = sign_transaction_v1(transaction, &privkey, &pubkey)
        .map_err(|e| js_error(&e))?;

    // Serialize signed transaction back to JAM
    let signed_bytes = jam_transaction(&signed_transaction)
        .map_err(|e| js_error(&format!("Failed to serialize signed transaction: {}", e)))?;

    // Convert to JavaScript Uint8Array
    let js_array = js_sys::Uint8Array::new_with_length(signed_bytes.len() as u32);
    js_array.copy_from(&signed_bytes);

    Ok(js_array.into())
}

////// converting API notes to transactions

// wrapper for api notes -> nnotes
fn api_tlrange_to_internal(r: &ApiTlRange) -> Result<tx_types::transaction_types::TimelockRange, String> {
    use tx_types::transaction_types::{PageNumber as TxPageNumber, TimelockRange as TxTlRange};
    let min = r.min.map(|v| TxPageNumber { value: v as i64 as u64 });
    let max = r.max.map(|v| TxPageNumber { value: v as i64 as u64 });
    Ok(TxTlRange { min, max })
}

fn api_timelock_to_internal(src: &Option<Box<ApiTimelocks>>) -> Result<tx_types::transaction_types::Timelock, String> {
    use tx_types::transaction_types::Timelock as TxTimelock;
    match src {
        None => Ok(TxTimelock::new_unchecked(None)),
        Some(t) => {
            // generator currently gives Box<ApiTlRange>
            let abs = api_tlrange_to_internal(&t.absolute)?;
            let rel = api_tlrange_to_internal(&t.relative)?;
            Ok(TxTimelock::new_unchecked(Some((abs, rel))))
        }
    }
}

fn api_lock_to_internal(src: &ApiLock) -> Result<tx_types::transaction_types::Lock, String> {
    use tx_types::transaction_types::Lock as TxLock;
    let m = src.m as u64;
    let pks = src.pubkeys.clone();
    TxLock::from_b58(m, pks)
}

/// Convert API note to V1 NNote (returns both note and lock)
///
/// For V1 notes, the lock is returned separately since it's not stored in the note itself
fn api_note_to_nnote_v1(n: &ApiNote) -> Result<(NNote, TxLock), String> {
    use tx_types::transaction_types as t;
    use tx_types::transaction_types_v0::NNoteV0;
    use tx_types::transaction_types_v1::{NNoteV1, NoteData};
    use tx_types::collections::ZMap;
    use crate::tx_v1::compute_lock_root;

    // Parse lock first (needed for both V0 and V1)
    let lock = api_lock_to_internal(&n.lock)?;

    // Parse source
    let p = t::Hash::from_b58(&n.source_hash)
        .map_err(|e| format!("invalid sourceHash base58: {e}"))?;
    let source = t::Source { p, is_coinbase: n.is_coinbase };

    // Parse assets
    if n.assets < 0 { return Err("assets must be non-negative".into()); }
    let assets = t::Coins { value: n.assets as u64 };

    // Parse version and origin_page
    let version = n.version as i64 as u64;
    let origin_page = t::PageNumber { value: n.origin_page as i64 as u64 };

    // Determine if this is V0 or V1 based on version or presence of timelock
    // For now, if there's a timelock, treat as V0; otherwise V1
    let has_timelock = n.timelock.is_some();

    let note = if has_timelock {
        // V0 note
        let timelock = api_timelock_to_internal(&n.timelock)?;
        let name = t::NName::new_default_v0(lock.clone(), source.clone(), timelock.clone());

        NNote::V0(NNoteV0 {
            meta: t::NNoteHead { version, origin_page, timelock },
            name,
            lock: lock.clone(),
            source,
            assets,
        })
    } else {
        // V1 note
        // Compute lock root from the lock
        let pubkeys_vec: Vec<t::SchnorrPubkey> = lock.pubkeys.iter().cloned().collect();
        let lock_root = compute_lock_root(lock.m, pubkeys_vec);

        // Create note name for V1
        let name = t::NName::new_v1(lock_root, source.clone());

        // Create empty note data (no additional data from API)
        let note_data = NoteData {
            map: ZMap::new(),
        };

        NNote::V1(NNoteV1 {
            version,
            origin_page,
            name,
            note_data,
            assets,
        })
    };

    Ok((note, lock))
}

/// Serialize transaction to JAM format
fn jam_transaction(tx: &Transaction) -> Result<Vec<u8>, String> {
    let mut slab: NounSlab = NounSlab::new();
    let tx_noun = tx.to_noun(&mut slab);
    slab.set_root(tx_noun);
    let jammed_bytes = slab.jam();
    Ok(jammed_bytes.to_vec())
}

/// Deserialize transaction from JAM format
fn unjam_transaction(jam_data: &[u8]) -> Result<Transaction, String> {
    use bytes::Bytes;

    let mut slab: NounSlab = NounSlab::new();
    let noun = slab.cue_into(Bytes::copy_from_slice(jam_data))
        .map_err(|e| format!("Failed to cue JAM data: {:?}", e))?;

    Transaction::from_noun(&noun)
        .map_err(|e| format!("Failed to decode transaction from noun: {:?}", e))
}
