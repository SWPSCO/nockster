use zeroize::{Zeroize, Zeroizing};
mod planner;
mod tx_engine;
mod tx_jam;
mod tx_v1;
mod wallet;

mod js_error;
mod vault;
mod vmk_slot;
mod secret;

use crate::tx_engine::{
    build_transaction_from_api, build_transaction_from_api_with_options, BuildTxOptions, RpcNote,
};
use crate::tx_jam::{
    inspect_tx_jam, sign_tx_jam, sign_tx_jam_selected, to_raw_tx_jam, to_wallet_tx_jam,
    JamInspectResult, JamSignResult,
};
use crate::vault::{KdfType, Vault, VaultExtern, VaultLogEntryExtern, WalletSummary};
use crate::wallet::Wallet;
use log::*;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};
use tx_types::transaction_types::{SchnorrPubkey, F6LT};
use wasm_bindgen::prelude::*;

/// Result of creating and signing a transaction
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TransactionResult {
    base64_tx: String,
    fee_paid: u64,
    tx_id: String,
    input_notes: Vec<String>,
}

#[wasm_bindgen(start)]

pub fn start() {
    console_error_panic_hook::set_once();
    if let Err(err) = console_log::init_with_level(log::Level::Debug) {
        // It's fine if the logger is already installed (e.g., multiple init attempts).
        log::warn!("logger already initialized: {:?}", err);
    }
    info!("wasm loaded");
}

#[wasm_bindgen(js_name = cheetahPkhB58)]
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
    Ok(pk.to_hash().to_b58())
}

// Minimal payload returned to JS when building a vault via WASM.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VaultPayload {
    vault: VaultExtern,
    vault_key: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnlockVaultPayload {
    vault: VaultExtern,
    vault_key: u32,
    wallets: Vec<WalletSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckPasswordPayload {
    vault: VaultExtern,
    matches: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LockVaultPayload {
    log: Vec<VaultLogEntryExtern>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportWalletPayload {
    vault: VaultExtern,
    vault_key: u32,
    wallet: WalletSummary,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RenameWalletPayload {
    vault: VaultExtern,
    vault_key: u32,
    wallet: WalletSummary,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteWalletPayload {
    vault: VaultExtern,
    vault_key: u32,
    deleted: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WalletPayload {
    vault: VaultExtern,
    vault_key: u32,
    wallet: WalletSummary,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WalletsPayload {
    vault: VaultExtern,
    vault_key: u32,
    wallets: Vec<WalletSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportWalletPayload {
    vault: VaultExtern,
    vault_key: u32,
    wallet: Wallet,
}

/// Bridge lock root constant - the fixed lock root for all bridge deposits
/// This corresponds to the bridge's multi-sig spend condition
pub const BRIDGE_LOCK_ROOT: &str = "AcsPkuhXQoGeEsF91yynpm1kcW17PQ2Z1MEozgx7YnDPkZwrtzLuuqd";

/// Minimum amount in nicks for bridge transactions (100k NOCK = 100,000 * 65536 nicks)
pub const BRIDGE_MINIMUM_NICKS: u64 = 100_000 * 65536;

/// Base field prime p = 2^64 - 2^32 + 1
const BASE_FIELD_PRIME: u128 = 18446744069414584321;

#[derive(Debug, Deserialize)]
struct TransactionRecipient {
    address: String,
    gift: u64,
    /// Optional EVM address for bridge transactions (hex string with or without 0x prefix)
    #[serde(rename = "bridgeEvmAddress")]
    bridge_evm_address: Option<String>,
}

#[wasm_bindgen(js_name = buildVault)]
pub fn build_vault(password: &str) -> Result<JsValue, JsValue> {
    let artifacts = Vault::new(password, KdfType::Argon2id)?;
    let wrapped_vault =
        VaultExtern::from_vault(artifacts.vault, "BUILD_VAULT", artifacts.vmk_slot)?;

    let payload = VaultPayload {
        vault: wrapped_vault,
        vault_key: artifacts.vmk_slot,
    };

    serde_wasm_bindgen::to_value(&payload).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen(js_name = unlockVault)]
pub fn unlock_vault(password: &str, vault_js: JsValue) -> Result<JsValue, JsValue> {
    let vault: VaultExtern = serde_wasm_bindgen::from_value(vault_js)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize vault: {err}")))?;
    let (vault, vmk_slot, wallets) = Vault::unlock_serialized(password, vault)?;
    let payload = UnlockVaultPayload {
        vault,
        vault_key: vmk_slot,
        wallets,
    };
    serde_wasm_bindgen::to_value(&payload).map_err(|err| JsValue::from_str(&err.to_string()))
}

/// Unlock the vault with a base64-encoded, 32-byte master key unwrapped by the device.
#[wasm_bindgen(js_name = unlockVaultWithVmk)]
pub fn unlock_vault_with_vmk(vmk_b64: &str, vault_js: JsValue) -> Result<JsValue, JsValue> {
    use base64::{engine::general_purpose, Engine as _};

    let vmk_bytes = Zeroizing::new(
        general_purpose::STANDARD
            .decode(vmk_b64)
            .map_err(|err| JsValue::from_str(&format!("failed to decode VMK: {err}")))?,
    );

    let vmk: &[u8; 32] = vmk_bytes
        .as_slice()
        .try_into()
        .map_err(|_| JsValue::from_str("VMK must be exactly 32 bytes"))?;

    let vault: VaultExtern = serde_wasm_bindgen::from_value(vault_js)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize vault: {err}")))?;

    let (vault, vmk_slot, wallets) = Vault::unlock_with_vmk(vmk, vault)?;
    let payload = UnlockVaultPayload {
        vault,
        vault_key: vmk_slot,
        wallets,
    };
    serde_wasm_bindgen::to_value(&payload).map_err(|err| JsValue::from_str(&err.to_string()))
}

/// Export the master key for authenticated hardware wrapping.
/// Callers keep this value transient and persist only its encrypted envelope.
#[wasm_bindgen(js_name = exportVmk)]
pub fn export_vmk(vault_key: u32) -> Result<String, JsValue> {
    use base64::{engine::general_purpose, Engine as _};

    let mut vmk = crate::vmk_slot::VmkSlots::export(vault_key)?;
    let encoded = general_purpose::STANDARD.encode(&vmk);
    vmk.zeroize();
    Ok(encoded)
}

#[wasm_bindgen(js_name = checkPassword)]
pub fn check_password(
    password: &str,
    vault_js: JsValue,
    vault_key: u32,
) -> Result<JsValue, JsValue> {
    let mut vault: VaultExtern = serde_wasm_bindgen::from_value(vault_js)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize vault: {err}")))?;
    let matches = Vault::verify_password(password, &vault, vault_key)?;
    vault.append_log("CHECK_PASSWORD");
    let payload = CheckPasswordPayload { vault, matches };
    serde_wasm_bindgen::to_value(&payload).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen(js_name = lockVault)]
pub fn lock_vault(vault_js: JsValue, vault_key: u32) -> Result<JsValue, JsValue> {
    let mut vault: VaultExtern = serde_wasm_bindgen::from_value(vault_js)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize vault: {err}")))?;
    vault.append_log("LOCK");
    crate::vmk_slot::VmkSlots::free(vault_key)?;
    let payload = LockVaultPayload { log: vault.log };
    serde_wasm_bindgen::to_value(&payload).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen(js_name = freeVaultKey)]
pub fn free_vault_key(vault_key: u32) -> Result<(), JsValue> {
    crate::vmk_slot::VmkSlots::free(vault_key)
}

#[wasm_bindgen(js_name = generateKey)]
pub fn generate_key() -> Result<JsValue, JsValue> {
    let words = Wallet::generate_key()?;
    serde_wasm_bindgen::to_value(&words).map_err(|err| JsValue::from_str(&err.to_string()))
}

/// Validate recovery material without creating or changing a vault.
#[wasm_bindgen(js_name = validateWalletKey)]
pub fn validate_wallet_key(key: &str) -> Result<String, JsValue> {
    Ok(Wallet::from_key(key)?.public_key.clone())
}

#[wasm_bindgen(js_name = importWallet)]
pub fn import_wallet(
    vault_js: JsValue,
    vault_key: u32,
    nickname: &str,
    key: &str,
) -> Result<JsValue, JsValue> {
    let mut vault: VaultExtern = serde_wasm_bindgen::from_value(vault_js)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize vault: {err}")))?;
    let wallet = vault.import_wallet(vault_key, nickname, key)?;
    vault.append_log("IMPORT_WALLET");
    let payload = ImportWalletPayload {
        vault,
        vault_key,
        wallet,
    };
    serde_wasm_bindgen::to_value(&payload).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen(js_name = renameWallet)]
pub fn rename_wallet(
    vault_js: JsValue,
    vault_key: u32,
    old_nickname: &str,
    new_nickname: &str,
) -> Result<JsValue, JsValue> {
    let mut vault: VaultExtern = serde_wasm_bindgen::from_value(vault_js)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize vault: {err}")))?;
    let wallet = vault.rename_wallet(vault_key, old_nickname, new_nickname)?;
    vault.append_log("RENAME_WALLET");
    let payload = RenameWalletPayload {
        vault,
        vault_key,
        wallet,
    };
    serde_wasm_bindgen::to_value(&payload).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen(js_name = deleteWallet)]
pub fn delete_wallet(
    vault_js: JsValue,
    vault_key: u32,
    nickname: &str,
) -> Result<JsValue, JsValue> {
    let mut vault: VaultExtern = serde_wasm_bindgen::from_value(vault_js)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize vault: {err}")))?;
    let deleted = vault.delete_wallet(vault_key, nickname)?;
    vault.append_log("DELETE_WALLET");
    let payload = DeleteWalletPayload {
        vault,
        vault_key,
        deleted,
    };
    serde_wasm_bindgen::to_value(&payload).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen(js_name = getPubkey)]
pub fn get_pubkey(vault_js: JsValue, vault_key: u32, nickname: &str) -> Result<JsValue, JsValue> {
    let mut vault: VaultExtern = serde_wasm_bindgen::from_value(vault_js)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize vault: {err}")))?;
    let wallet = vault.wallet_summary(vault_key, nickname)?;
    vault.append_log("GET_PUBKEY");
    let payload = WalletPayload {
        vault,
        vault_key,
        wallet,
    };
    serde_wasm_bindgen::to_value(&payload).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen(js_name = getWallets)]
pub fn get_wallets(vault_js: JsValue, vault_key: u32) -> Result<JsValue, JsValue> {
    let mut vault: VaultExtern = serde_wasm_bindgen::from_value(vault_js)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize vault: {err}")))?;
    let wallets = vault.wallet_summaries(vault_key)?;
    vault.append_log("GET_WALLETS");
    let payload = WalletsPayload {
        vault,
        vault_key,
        wallets,
    };
    serde_wasm_bindgen::to_value(&payload).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen(js_name = exportWallet)]
pub fn export_wallet(
    vault_js: JsValue,
    vault_key: u32,
    nickname: &str,
) -> Result<JsValue, JsValue> {
    let mut vault: VaultExtern = serde_wasm_bindgen::from_value(vault_js)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize vault: {err}")))?;
    let wallet = vault.export_wallet(vault_key, nickname)?;
    vault.append_log("EXPORT_WALLET");
    let payload = ExportWalletPayload {
        vault,
        vault_key,
        wallet,
    };
    serde_wasm_bindgen::to_value(&payload).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen(js_name = createAndSignTx)]
pub fn create_and_sign_tx(
    vault_js: JsValue,
    vault_key: u32,
    wallet_nickname: &str,
    notes_js: JsValue,
    recipients_js: JsValue,
) -> Result<JsValue, JsValue> {
    let vault: VaultExtern = serde_wasm_bindgen::from_value(vault_js)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize vault: {err}")))?;

    let wallet = vault.export_wallet(vault_key, wallet_nickname)?;

    let notes_json: JsonValue = serde_wasm_bindgen::from_value(notes_js)
        .map_err(|err| JsValue::from_str(&format!("failed to parse notes payload: {err}")))?;
    let normalized_notes_json =
        normalize_note_payload(notes_json).map_err(|err| JsValue::from_str(&err))?;
    let notes: Vec<RpcNote> = serde_json::from_value(normalized_notes_json)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize notes: {err}")))?;

    let recipients: Vec<TransactionRecipient> = serde_wasm_bindgen::from_value(recipients_js)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize recipients: {err}")))?;

    let built = build_transaction_from_api(&wallet, notes, recipients)
        .map_err(|err| JsValue::from_str(&err))?;

    let result = TransactionResult {
        base64_tx: built.base64_tx,
        fee_paid: built.fee_paid,
        tx_id: built.tx_id,
        input_notes: built.input_notes,
    };

    serde_wasm_bindgen::to_value(&result).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen(js_name = createAndSignTxWithOptions)]
pub fn create_and_sign_tx_with_options(
    vault_js: JsValue,
    vault_key: u32,
    wallet_nickname: &str,
    notes_js: JsValue,
    recipients_js: JsValue,
    private_outputs: bool,
    chain_height: Option<u32>,
) -> Result<JsValue, JsValue> {
    let vault: VaultExtern = serde_wasm_bindgen::from_value(vault_js)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize vault: {err}")))?;

    let wallet = vault.export_wallet(vault_key, wallet_nickname)?;

    let notes_json: JsonValue = serde_wasm_bindgen::from_value(notes_js)
        .map_err(|err| JsValue::from_str(&format!("failed to parse notes payload: {err}")))?;
    let normalized_notes_json =
        normalize_note_payload(notes_json).map_err(|err| JsValue::from_str(&err))?;
    let notes: Vec<RpcNote> = serde_json::from_value(normalized_notes_json)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize notes: {err}")))?;

    let recipients: Vec<TransactionRecipient> = serde_wasm_bindgen::from_value(recipients_js)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize recipients: {err}")))?;

    let built = build_transaction_from_api_with_options(
        &wallet,
        notes,
        recipients,
        BuildTxOptions {
            private_outputs,
            chain_height: chain_height.map(u64::from),
        },
    )
    .map_err(|err| JsValue::from_str(&err))?;

    let result = TransactionResult {
        base64_tx: built.base64_tx,
        fee_paid: built.fee_paid,
        tx_id: built.tx_id,
        input_notes: built.input_notes,
    };

    serde_wasm_bindgen::to_value(&result).map_err(|err| JsValue::from_str(&err.to_string()))
}

/// Verify serialized bridge outputs against the reviewed payment.
#[wasm_bindgen(js_name = verifyBridgeIntent)]
pub fn verify_bridge_intent(
    transaction: &str, source: &str, recipients: JsValue, network_fee: u64,
) -> Result<(), JsValue> {
    let source = tx_engine::parse_hash(source).map_err(|e| JsValue::from_str(&e))?;
    let recipients = serde_wasm_bindgen::from_value(recipients)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    tx_engine::verify_bridge_intent(transaction, &source, recipients, network_fee)
        .map_err(|e| JsValue::from_str(&e))
}

/// Compose a draft without exposing signing keys to the hardware transport.
#[wasm_bindgen(js_name = composeUnsignedTx)]
pub fn compose_unsigned_tx(
    source: &str,
    notes_js: JsValue,
    recipients_js: JsValue,
    chain_height: Option<u32>,
) -> Result<JsValue, JsValue> {
    let source = tx_engine::parse_hash(source).map_err(|e| JsValue::from_str(&e))?;
    let json =
        serde_wasm_bindgen::from_value(notes_js).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let json = normalize_note_payload(json).map_err(|e| JsValue::from_str(&e))?;
    let notes = serde_json::from_value(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let recipients = serde_wasm_bindgen::from_value(recipients_js)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let built =
        tx_engine::build_unsigned_from_api(&source, notes, recipients, chain_height.map(u64::from))
            .map_err(|e| JsValue::from_str(&e))?;
    serde_wasm_bindgen::to_value(&TransactionResult {
        base64_tx: built.base64_tx,
        fee_paid: built.fee_paid,
        tx_id: built.tx_id,
        input_notes: built.input_notes,
    })
    .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen(js_name = maxSendAmount)]
pub fn max_send_amount(
    source: &str,
    notes_js: JsValue,
    destination: &str,
    private_outputs: bool,
    chain_height: Option<u32>,
) -> Result<u64, JsValue> {
    let source = tx_engine::parse_hash(source).map_err(|e| JsValue::from_str(&e))?;
    let destination = tx_engine::parse_hash(destination).map_err(|e| JsValue::from_str(&e))?;
    let json =
        serde_wasm_bindgen::from_value(notes_js).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let json = normalize_note_payload(json).map_err(|e| JsValue::from_str(&e))?;
    let notes = serde_json::from_value(json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    tx_engine::max_payment(
        &source,
        notes,
        &destination,
        BuildTxOptions {
            private_outputs,
            chain_height: chain_height.map(u64::from),
        },
    )
    .map_err(|e| JsValue::from_str(&e))
}

/// Returns the minimum network fee in nicks
#[wasm_bindgen(js_name = getMinNetworkFee)]
pub fn get_min_network_fee() -> u64 {
    tx_types::fee_calculator::MIN_FEE
}

/// Returns the base fee per word in nicks (fee = words * BASE_FEE)
#[wasm_bindgen(js_name = getBaseFeePerWord)]
pub fn get_base_fee_per_word() -> u64 {
    tx_types::fee_calculator::BASE_FEE
}

#[wasm_bindgen(js_name = inspectTxJam)]
pub fn inspect_tx_jam_wasm(base64_jam: &str) -> Result<JsValue, JsValue> {
    let result: JamInspectResult =
        inspect_tx_jam(base64_jam).map_err(|err| JsValue::from_str(&err))?;
    serde_wasm_bindgen::to_value(&result).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen(js_name = signTxJam)]
pub fn sign_tx_jam_wasm(
    vault_js: JsValue,
    vault_key: u32,
    wallet_nickname: &str,
    base64_jam: &str,
) -> Result<JsValue, JsValue> {
    let vault: VaultExtern = serde_wasm_bindgen::from_value(vault_js)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize vault: {err}")))?;
    let wallet = vault.export_wallet(vault_key, wallet_nickname)?;
    let result: JamSignResult =
        sign_tx_jam(&wallet, base64_jam).map_err(|err| JsValue::from_str(&err))?;
    serde_wasm_bindgen::to_value(&result).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen(js_name = signTxJamSelected)]
pub fn sign_tx_jam_selected_wasm(
    vault_js: JsValue,
    vault_key: u32,
    wallet_nickname: &str,
    base64_jam: &str,
    selected_spends_js: JsValue,
) -> Result<JsValue, JsValue> {
    let vault: VaultExtern = serde_wasm_bindgen::from_value(vault_js)
        .map_err(|err| JsValue::from_str(&format!("failed to deserialize vault: {err}")))?;
    let wallet = vault.export_wallet(vault_key, wallet_nickname)?;
    let selected_spends: Vec<String> =
        serde_wasm_bindgen::from_value(selected_spends_js).map_err(|err| {
            JsValue::from_str(&format!("failed to deserialize selected spends: {err}"))
        })?;
    let result: JamSignResult = sign_tx_jam_selected(&wallet, base64_jam, selected_spends)
        .map_err(|err| JsValue::from_str(&err))?;
    serde_wasm_bindgen::to_value(&result).map_err(|err| JsValue::from_str(&err.to_string()))
}

#[wasm_bindgen(js_name = verifySignedDraft)]
pub fn verify_signed_draft(draft: &str, signed: &str) -> Result<(), JsValue> {
    tx_jam::verify_signed_draft(draft, signed).map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen(js_name = verifyPartialSignedDraft)]
pub fn verify_partial_signed_draft(draft: &str, signed: &str) -> Result<(), JsValue> {
    tx_jam::verify_partial_signed_draft(draft, signed).map_err(|e| JsValue::from_str(&e))
}

#[wasm_bindgen(js_name = toRawTxJam)]
pub fn to_raw_tx_jam_wasm(base64_jam: &str) -> Result<String, JsValue> {
    to_raw_tx_jam(base64_jam).map_err(|err| JsValue::from_str(&err))
}

#[wasm_bindgen(js_name = toWalletTxJam)]
pub fn to_wallet_tx_jam_wasm(base64_jam: &str) -> Result<String, JsValue> {
    to_wallet_tx_jam(base64_jam).map_err(|err| JsValue::from_str(&err))
}

fn normalize_note_payload(value: JsonValue) -> Result<JsonValue, String> {
    match value {
        JsonValue::Array(mut notes) => {
            for (idx, note) in notes.iter_mut().enumerate() {
                if !note.is_object() {
                    return Err(format!(
                        "expected each note to be an object, found {note:?}"
                    ));
                }
                let map = note
                    .as_object_mut()
                    .expect("checked note is object; as_object_mut must succeed");
                let version_value = map
                    .get_mut("version")
                    .ok_or_else(|| format!("note #{idx} is missing required field 'version'"))?;
                normalize_version_value(version_value)
                    .map_err(|err| format!("failed to normalize version for note #{idx}: {err}"))?;
                ensure_note_data(map).map_err(|err| {
                    format!("failed to normalize noteData for note #{idx}: {err}")
                })?;
            }
            Ok(JsonValue::Array(notes))
        }
        other => Err(format!("expected notes array, found {other:?}")),
    }
}

fn ensure_note_data(map: &mut JsonMap<String, JsonValue>) -> Result<(), String> {
    match map.get_mut("noteData") {
        Some(JsonValue::Object(_)) => Ok(()),
        Some(JsonValue::Null) => {
            map.insert("noteData".to_string(), JsonValue::Object(JsonMap::new()));
            Ok(())
        }
        Some(other) => Err(format!("noteData must be an object, found {other:?}")),
        None => {
            map.insert("noteData".to_string(), JsonValue::Object(JsonMap::new()));
            Ok(())
        }
    }
}

fn normalize_version_value(value: &mut JsonValue) -> Result<(), String> {
    match value {
        JsonValue::String(_) => Ok(()),
        JsonValue::Number(num) => {
            let stringified = stringify_integer_number(num)
                .ok_or_else(|| format!("version value must be an integer, found {num:?}"))?;
            *value = JsonValue::String(stringified);
            Ok(())
        }
        other => Err(format!(
            "version must be a number or string, found {other:?}"
        )),
    }
}

fn stringify_integer_number(num: &serde_json::Number) -> Option<String> {
    if let Some(val) = num.as_u64() {
        Some(val.to_string())
    } else if let Some(val) = num.as_i64() {
        Some(val.to_string())
    } else {
        num.as_f64().and_then(|float_val| {
            let rounded = float_val.round();
            if (float_val - rounded).abs() < f64::EPSILON {
                let integer = rounded as i64;
                Some(integer.to_string())
            } else {
                None
            }
        })
    }
}
