use crate::tx_v1::sign_transaction_v1;
use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use nockapp::noun::slab::{NockJammer, NounSlab};
use nockblocks_api_v1::models::{note_v1_object::Version as ApiNoteVersion, NoteV1Object};
use noun_serde::{NounDecode, NounEncode};
use serde_json::Value as JsonValue;
use tx_types::{
    compute_tx_id_v1, lock_data_to_untyped_noun, Coins, Hash, InputV1, Inputs, InputsV1, LockData,
    LockPrimitive, LockPrimitiveBody, NName, NNote, NNoteV1, NoteData, PageNumber, PkhSignature,
    Spend, SpendBody, SpendCondition, SpendsV1, Tim, TimelockRange, Transaction, UntypedNoun, ZMap,
    ZSet,
};

use crate::wallet::Wallet;
use crate::{TransactionRecipient, BASE_FIELD_PRIME, BRIDGE_LOCK_ROOT, BRIDGE_MINIMUM_NICKS};

const MAX_FEE_ADJUSTMENTS: usize = 32;

/// Convert an EVM address (hex string) to based field elements for noteData.bridge
/// The EVM address is stored as [%0 [%base addr=evm-address-based]]
/// where evm-address-based is a triple of base field elements
///
/// EVM addresses are 160 bits (20 bytes), so we need to handle them as big integers
/// by parsing the hex in chunks.
fn evm_address_to_based(evm_hex: &str) -> Result<[u64; 3], String> {
    // Remove 0x prefix if present
    let hex = evm_hex.strip_prefix("0x").unwrap_or(evm_hex);

    if hex.len() != 40 {
        return Err(format!(
            "invalid EVM address '{}': expected 40 hex characters, got {}",
            evm_hex,
            hex.len()
        ));
    }

    // Parse the 160-bit address as bytes
    if !hex.is_ascii() {
        return Err("EVM address must be hexadecimal ASCII".into());
    }
    let bytes =
        hex_to_bytes(hex).map_err(|e| format!("invalid EVM address '{}': {}", evm_hex, e))?;
    if bytes.iter().all(|byte| *byte == 0) {
        return Err("bridge destination cannot be the zero address".into());
    }

    // Convert bytes to little-endian limbs
    let mut addr_limbs = [0u64; 3];
    // bytes is big-endian, we need little-endian
    let bytes_le: Vec<u8> = bytes.iter().rev().cloned().collect();

    // Pack into u64 limbs (little-endian)
    for (i, chunk) in bytes_le.chunks(8).enumerate() {
        if i >= 3 {
            break;
        }
        let mut val = 0u64;
        for (j, &b) in chunk.iter().enumerate() {
            val |= (b as u64) << (j * 8);
        }
        addr_limbs[i] = val;
    }

    // Now perform division by p = 2^64 - 2^32 + 1 = 18446744069414584321
    // to get three based field elements
    let p = BASE_FIELD_PRIME;

    // Helper: divide [u64; 3] by u128, return (quotient, remainder)
    fn div_big_by_p(limbs: [u64; 3], p: u128) -> ([u64; 3], u64) {
        // Convert limbs to 192-bit value and divide
        // limbs[0] + limbs[1] * 2^64 + limbs[2] * 2^128
        let mut quot = [0u64; 3];
        let mut rem = 0u128;

        // Process from most significant limb
        for i in (0..3).rev() {
            let cur = (rem << 64) | limbs[i] as u128;
            quot[i] = (cur / p) as u64;
            rem = cur % p;
        }

        (quot, rem as u64)
    }

    let (q1, a) = div_big_by_p(addr_limbs, p);
    let (q2, b) = div_big_by_p(q1, p);
    let (q3, c) = div_big_by_p(q2, p);

    // q3 should be all zeros
    if q3 != [0, 0, 0] {
        return Err(format!(
            "EVM address {} is too large for based encoding",
            evm_hex
        ));
    }

    Ok([a, b, c])
}

/// Parse a hex string into bytes
fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err("hex string must have even length".to_string());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16)
                .map_err(|_| format!("invalid hex at position {}", i))
        })
        .collect()
}

/// Struct representing the bridge deposit data: [%0 [%base [a b c]]]
/// Uses noun_serde for automatic encoding
#[derive(Clone, Debug, NounEncode, NounDecode)]
struct BridgeDepositData {
    /// Version tag: always 0
    version: u64,
    /// Bridge type and address
    body: BridgeDepositBody,
}

#[derive(Clone, Debug, NounEncode, NounDecode)]
struct BridgeDepositBody {
    /// Type tag: "base" as a cord
    tag: String,
    /// EVM address as three based field elements
    addr: (u64, u64, u64),
}

/// Build noteData for a bridge output containing the EVM destination address
fn build_bridge_note_data(evm_address: &str) -> Result<NoteData, String> {
    let based_addr = evm_address_to_based(evm_address)?;

    // Build the bridge data structure matching Hoon: [%0 [%base [a b c]]]
    let bridge_data = BridgeDepositData {
        version: 0,
        body: BridgeDepositBody {
            tag: "base".to_string(),
            addr: (based_addr[0], based_addr[1], based_addr[2]),
        },
    };

    // Encode to noun and jam it
    let mut slab = NounSlab::<NockJammer>::new();
    let noun = bridge_data.to_noun(&mut slab);
    slab.set_root(noun);
    let jammed = slab.jam();

    let bridge_noun = UntypedNoun {
        p: Bytes::from(jammed.to_vec()),
    };

    let mut note_data_map = ZMap::new();
    note_data_map.put("bridge".to_string(), bridge_noun);

    Ok(NoteData { map: note_data_map })
}

#[derive(Clone)]
struct PreparedNote {
    note: NNote,
}

pub(crate) type RpcNote = NoteV1Object;

#[derive(Debug, Clone, NounEncode, NounDecode)]
pub(crate) struct WalletTransaction {
    pub name: String,
    pub spends: SpendsV1,
}

#[allow(dead_code)]
pub struct BuiltTransaction {
    pub base64_tx: String,
    pub fee_paid: u64,
    pub tx_id: String,
    pub input_notes: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct BuildTxOptions {
    pub private_outputs: bool,
    pub chain_height: Option<u64>,
}

impl Default for BuildTxOptions {
    fn default() -> Self {
        Self {
            private_outputs: false,
            chain_height: None,
        }
    }
}

pub fn build_transaction_from_api(
    wallet: &Wallet,
    notes: Vec<NoteV1Object>,
    recipients: Vec<TransactionRecipient>,
) -> Result<BuiltTransaction, String> {
    build_transaction_from_api_with_options(wallet, notes, recipients, BuildTxOptions::default())
}

pub fn build_transaction_from_api_with_options(
    wallet: &Wallet,
    notes: Vec<NoteV1Object>,
    recipients: Vec<TransactionRecipient>,
    options: BuildTxOptions,
) -> Result<BuiltTransaction, String> {
    if notes.is_empty() {
        return Err("no notes provided".to_string());
    }
    if recipients.is_empty() {
        return Err("no recipients provided".to_string());
    }

    let signing_key = wallet.private_key_t8()?;
    let pubkey = wallet.schnorr_pubkey()?;
    let source = wallet.public_key_hash()?;
    if pubkey.to_hash() != source {
        return Err("wallet address does not match signing key".into());
    }
    let (spends, prepared, fee) = plan_transaction(&source, notes, recipients, options)?;
    let unsigned = build_transaction_from_spends(&spends, &prepared)?;
    let signed = sign_transaction_v1(unsigned, &signing_key, &pubkey)?;
    let spends = extract_spends_from_transaction(&signed)?;
    if tx_types::fee_calculator::calculate_min_fee(&spends.map).value > fee {
        return Err("signed transaction exceeds planned fee".into());
    }
    finish_transaction(spends, fee)
}

pub(crate) fn build_unsigned_from_api(
    source: &Hash,
    notes: Vec<NoteV1Object>,
    recipients: Vec<TransactionRecipient>,
    chain_height: Option<u64>,
) -> Result<BuiltTransaction, String> {
    let (spends, _, fee) = plan_transaction(
        source,
        notes,
        recipients,
        BuildTxOptions {
            private_outputs: false,
            chain_height,
        },
    )?;
    finish_transaction(spends, fee)
}

pub(crate) fn max_payment(
    source: &Hash,
    notes: Vec<NoteV1Object>,
    destination: &Hash,
    options: BuildTxOptions,
) -> Result<u64, String> {
    let prepared = convert_notes(notes)?;
    let notes: Vec<_> = prepared
        .into_iter()
        .map(|p| match p.note {
            NNote::V1(n) => n,
            _ => unreachable!(),
        })
        .collect();
    let available = notes
        .iter()
        .filter(|n| crate::planner::spendable_lock(n, source, options.chain_height).is_some())
        .try_fold(0u64, |sum, n| {
            sum.checked_add(n.assets.value).ok_or("balance overflow")
        })?;
    let mut fee = tx_types::fee_calculator::MIN_FEE;
    for _ in 0..MAX_FEE_ADJUSTMENTS {
        let amount = available
            .checked_sub(fee)
            .filter(|amount| *amount > 0)
            .ok_or("balance does not cover the network fee")?;
        let output = crate::planner::payment(destination, amount, options.private_outputs);
        let spends = crate::planner::plan_spends(
            &notes,
            &[output],
            fee,
            source,
            options.private_outputs,
            options.chain_height,
        )?;
        let required = crate::planner::signed_fee(&spends, source);
        if required <= fee {
            return Ok(amount);
        }
        fee = required;
    }
    Err("unable to converge on a network fee".into())
}

fn plan_transaction(
    source: &Hash,
    notes: Vec<NoteV1Object>,
    recipients: Vec<TransactionRecipient>,
    options: BuildTxOptions,
) -> Result<(SpendsV1, Vec<PreparedNote>, u64), String> {
    if notes.is_empty() || recipients.is_empty() {
        return Err("notes and recipients are required".into());
    }
    let prepared = convert_notes(notes)?;
    let notes = prepared
        .iter()
        .map(|p| match &p.note {
            NNote::V1(n) => n.clone(),
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();
    let mut outputs = convert_recipients(recipients)?;
    if options.private_outputs
        && outputs.iter().any(|output| output.note_data.map.has(&"bridge".into()))
    {
        return Err("bridge transactions require public outputs".into());
    }
    if options.private_outputs {
        for output in &mut outputs {
            output.note_data.map.del(&"lock".into());
        }
    }
    let mut fee = tx_types::fee_calculator::MIN_FEE;
    for _ in 0..MAX_FEE_ADJUSTMENTS {
        let spends = crate::planner::plan_spends(
            &notes,
            &outputs,
            fee,
            source,
            options.private_outputs,
            options.chain_height,
        )?;
        let required = crate::planner::signed_fee(&spends, source);
        if required <= fee {
            return Ok((spends, prepared, fee));
        }
        let selected_total = notes
            .iter()
            .filter(|n| spends.map.has(&n.name))
            .try_fold(0u64, |sum, n| {
                sum.checked_add(n.assets.value).ok_or("input overflow")
            })?;
        let output_total = outputs.iter().try_fold(0u64, |sum, o| {
            sum.checked_add(o.amount).ok_or("output overflow")
        })?;
        let remainder = selected_total
            .checked_sub(output_total)
            .ok_or("insufficient funds")?;
        if remainder >= tx_types::fee_calculator::MIN_FEE && remainder <= required {
            let exact = crate::planner::plan_spends(
                &notes,
                &outputs,
                remainder,
                source,
                options.private_outputs,
                options.chain_height,
            )?;
            if crate::planner::signed_fee(&exact, source) <= remainder {
                return Ok((exact, prepared, remainder));
            }
        }
        fee = required;
    }
    Err("unable to converge on a network fee".into())
}

fn finish_transaction(spends: SpendsV1, fee: u64) -> Result<BuiltTransaction, String> {
    let tx_id = compute_tx_id_v1(&spends).to_b58();
    let input_notes = spends
        .map
        .tap()
        .iter()
        .map(|(name, _)| format_note_name(name))
        .collect();
    let base64_tx = encode_wallet_transaction(&WalletTransaction {
        name: tx_id.clone(),
        spends,
    });
    Ok(BuiltTransaction {
        base64_tx,
        fee_paid: fee,
        tx_id,
        input_notes,
    })
}

fn convert_notes(notes: Vec<NoteV1Object>) -> Result<Vec<PreparedNote>, String> {
    let mut out = Vec::with_capacity(notes.len());
    for note in notes {
        if note.spent_on.is_some() {
            return Err("cannot spend a spent note".into());
        }
        out.push(PreparedNote {
            note: NNote::V1(convert_note_v1(note)?),
        });
    }
    Ok(out)
}

fn build_transaction_from_spends(
    spends: &SpendsV1,
    notes: &[PreparedNote],
) -> Result<Transaction, String> {
    let mut inputs_map = ZMap::new();
    for (name, spend) in spends.map.tap() {
        let prepared = find_note(notes, &name)
            .ok_or_else(|| format!("missing note for {}", format_note_name(&name)))?;
        let note_clone = match &prepared.note {
            NNote::V1(v1) => v1.clone(),
            _ => {
                return Err("only v1 notes are supported".to_string());
            }
        };
        match spend.clone().body {
            SpendBody::V1(mut body) => {
                body.witness.pkh = PkhSignature { map: ZMap::new() };
                let input = InputV1 {
                    note: note_clone,
                    spend: body,
                };
                inputs_map.put(name.clone(), input);
            }
            _ => {
                return Err("unexpected spend body version".to_string());
            }
        }
    }
    let tx_id = compute_tx_id_v1(spends);
    Ok(Transaction {
        name: tx_id.to_b58(),
        p: Inputs::V1(InputsV1 { map: inputs_map }),
    })
}

fn extract_spends_from_transaction(tx: &Transaction) -> Result<SpendsV1, String> {
    match &tx.p {
        Inputs::V1(inputs) => {
            let mut map = ZMap::new();
            for (name, input) in inputs.map.tap() {
                map.put(
                    name.clone(),
                    Spend {
                        version: 1,
                        body: SpendBody::V1(input.spend.clone()),
                    },
                );
            }
            Ok(SpendsV1 { map })
        }
        _ => Err("transaction is not v1".to_string()),
    }
}

fn find_note<'a>(notes: &'a [PreparedNote], name: &NName) -> Option<&'a PreparedNote> {
    notes
        .iter()
        .find(|prepared| note_identifier(&prepared.note) == *name)
}

fn note_identifier(note: &NNote) -> NName {
    match note {
        NNote::V0(n) => n.name.clone(),
        NNote::V1(n) => n.name.clone(),
    }
}

fn format_note_name(name: &NName) -> String {
    name.p
        .iter()
        .map(|hash| hash.to_b58())
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn parse_hash(value: &str) -> Result<Hash, String> {
    let hash = Hash::from_b58(value)?;
    if hash
        .values
        .iter()
        .any(|v| u128::from(*v) >= BASE_FIELD_PRIME)
    {
        return Err("hash exceeds field size".into());
    }
    Ok(hash)
}

fn convert_note_v1(note: NoteV1Object) -> Result<NNoteV1, String> {
    let version = match note.version {
        ApiNoteVersion::Variant1 => 1,
    };
    let origin_page = if note.origin_page < 0 {
        return Err("note originPage must be non-negative".into());
    } else {
        note.origin_page as u64
    };
    if note.assets < 0 || note.assets > 9_007_199_254_740_991 {
        return Err("note assets must be a non-negative safe integer".into());
    }
    let assets = note.assets as u64;

    let first = parse_hash(&note.first_name)
        .map_err(|e| format!("invalid firstName hash '{}': {e}", note.first_name))?;
    let last = parse_hash(&note.last_name)
        .map_err(|e| format!("invalid lastName hash '{}': {e}", note.last_name))?;
    let mut name_parts = Vec::with_capacity(2);
    name_parts.push(first);
    name_parts.push(last);

    let mut note_data_map = ZMap::new();
    for (key, value) in note.note_data {
        if key == "lock" {
            // The API labels inferred spending conditions; they are not on-chain note data.
            // Parent hashes commit only to the actual input note.
            if value.get("source").and_then(JsonValue::as_str) == Some("reconstructed") {
                continue;
            }
            if let Some(lock_noun) = lock_value_to_untyped_noun(value)? {
                note_data_map.put(key, lock_noun);
            }
            continue;
        }

        if let Some(untyped) = base64_json_value_to_untyped_noun(value, &key)? {
            note_data_map.put(key, untyped);
        }
    }

    Ok(NNoteV1 {
        version,
        origin_page: PageNumber { value: origin_page },
        name: NName { p: name_parts },
        note_data: NoteData { map: note_data_map },
        assets: Coins { value: assets },
    })
}

fn lock_value_to_untyped_noun(value: JsonValue) -> Result<Option<UntypedNoun>, String> {
    match value {
        JsonValue::Null => Ok(None),
        JsonValue::Object(_) => {
            let spend_condition = parse_lock_from_json(&value)?;
            let lock_data = LockData::V0(spend_condition);
            Ok(Some(lock_data_to_untyped_noun(&lock_data)))
        }
        JsonValue::String(encoded) => {
            let decoded = decode_base64(&encoded, "noteData.lock")?;
            Ok(Some(UntypedNoun {
                p: Bytes::from(decoded),
            }))
        }
        other => Err(format!(
            "noteData.lock must be an object or base64 string, found {other:?}"
        )),
    }
}

fn base64_json_value_to_untyped_noun(
    value: JsonValue,
    label: &str,
) -> Result<Option<UntypedNoun>, String> {
    match value {
        JsonValue::Null => Ok(None),
        JsonValue::String(encoded) => {
            let decoded = decode_base64(&encoded, &format!("noteData.{label}"))?;
            Ok(Some(UntypedNoun {
                p: Bytes::from(decoded),
            }))
        }
        other => Err(format!(
            "noteData.{label} must be a base64 string, found {other:?}"
        )),
    }
}

pub(crate) fn validate_based_noun(noun: nockvm::noun::Noun) -> Result<(), String> {
    let mut pending = vec![noun];
    let mut count = 0usize;
    while let Some(noun) = pending.pop() {
        count += 1;
        if count > 100_000 {
            return Err("noun exceeds size limit".into());
        }
        if let Ok(cell) = noun.as_cell() {
            pending.push(cell.head());
            pending.push(cell.tail());
        } else {
            let atom = noun.as_atom().map_err(|_| "invalid noun atom")?;
            let value = atom.as_u64().map_err(|_| "noun atom exceeds field size")?;
            if u128::from(value) >= BASE_FIELD_PRIME {
                return Err("noun atom exceeds field size".into());
            }
        }
    }
    Ok(())
}

fn decode_base64(encoded: &str, label: &str) -> Result<Vec<u8>, String> {
    let trimmed = encoded.trim();
    if trimmed.len() > 1_400_000 {
        return Err(format!("{label} exceeds size limit"));
    }
    let bytes = general_purpose::STANDARD
        .decode(trimmed)
        .map_err(|err| format!("failed to decode base64 for {label}: {err}"))?;
    let mut slab = NounSlab::<NockJammer>::new();
    let noun = slab
        .cue_into(Bytes::from(bytes.clone()))
        .map_err(|e| format!("invalid noun for {label}: {e}"))?;
    validate_based_noun(noun)?;
    Ok(bytes)
}

fn convert_recipients(
    recipients: Vec<TransactionRecipient>,
) -> Result<Vec<crate::planner::Output>, String> {
    if recipients.len() != 1 && recipients.iter().any(|r| r.bridge_evm_address.is_some()) {
        return Err("bridge transactions require one deposit and no additional recipients".into());
    }
    let mut outputs = Vec::new();
    let mut has_bridge = false;
    for recipient in recipients {
        if recipient.gift == 0 || recipient.gift > 9_007_199_254_740_991 {
            return Err("recipient gift must be a positive safe integer".into());
        }
        if let Some(evm) = recipient.bridge_evm_address {
            if recipient.address != BRIDGE_LOCK_ROOT {
                return Err("bridge recipient must use the canonical bridge lock root".into());
            }
            if has_bridge {
                return Err("only one bridge recipient is supported per transaction".into());
            }
            has_bridge = true;
            if recipient.gift < BRIDGE_MINIMUM_NICKS {
                return Err("bridge deposit requires at least 100,000 NOCK".into());
            }
            outputs.push(crate::planner::Output {
                lock_root: Hash::from_b58(BRIDGE_LOCK_ROOT)?,
                note_data: build_bridge_note_data(&evm)?,
                amount: recipient.gift,
            });
        } else {
            if recipient.address == BRIDGE_LOCK_ROOT {
                return Err("use Bridge to Base and supply a Base destination".into());
            }
            outputs.push(crate::planner::payment(
                &parse_hash(&recipient.address)?,
                recipient.gift,
                false,
            ));
        }
    }
    Ok(outputs)
}

/// Check the serialized bridge transaction against the reviewed destinations, amounts and fee.
pub(crate) fn verify_bridge_intent(
    transaction: &str,
    source: &Hash,
    recipients: Vec<TransactionRecipient>,
    network_fee: u64,
) -> Result<(), String> {
    if recipients.iter().filter(|r| r.bridge_evm_address.is_some()).count() != 1 {
        return Err("one bridge destination is required".into());
    }
    let expected = convert_recipients(recipients)?;
    let refund = crate::planner::payment(source, 0, false);
    let raw = crate::tx_jam::to_raw_tx_jam(transaction)?;
    let mut slab = NounSlab::<NockJammer>::new();
    let noun = slab.cue_into(Bytes::from(general_purpose::STANDARD.decode(raw).map_err(|e| e.to_string())?))
        .map_err(|e| e.to_string())?;
    let raw = tx_types::transaction_types_v1::RawTransactionV1::from_noun(&noun)
        .map_err(|e| e.to_string())?;
    let mut totals = vec![0u64; expected.len()];
    let mut fee = 0u64;
    for (_, spend) in raw.spends.map.tap() {
        let (seeds, spend_fee) = match spend.body {
            SpendBody::V1(body) => (body.seeds, body.fee.value),
            SpendBody::V0ToV1(body) => (body.seeds, body.fee.value),
            _ => return Err("bridge requires V1 outputs".into()),
        };
        fee = fee.checked_add(spend_fee).ok_or("fee overflow")?;
        for seed in seeds.set.tap() {
            if seed.output_source.is_some() {
                return Err("unexpected bridge output source".into());
            }
            if let Some(index) = expected.iter().position(|output|
                output.lock_root == seed.lock_root && output.note_data.to_hash() == seed.note_data.to_hash()) {
                totals[index] = totals[index].checked_add(seed.gift.value).ok_or("output overflow")?;
            } else if seed.lock_root != refund.lock_root || seed.note_data.to_hash() != refund.note_data.to_hash() {
                return Err("transaction contains an unreviewed output or bridge destination".into());
            }
        }
    }
    if fee != network_fee || expected.iter().zip(totals).any(|(output, total)| output.amount != total) {
        return Err("bridge transaction does not match the reviewed amounts and fee".into());
    }
    Ok(())
}

fn encode_wallet_transaction(tx: &WalletTransaction) -> String {
    let mut slab = NounSlab::<NockJammer>::new();
    let noun = tx.to_noun(&mut slab);
    slab.set_root(noun);
    let jammed = slab.jam();
    general_purpose::STANDARD.encode(jammed.as_ref())
}

#[cfg(test)]
fn decode_wallet_transaction_base64(value: &str) -> Result<WalletTransaction, String> {
    let trimmed = value.trim();
    let bytes = general_purpose::STANDARD
        .decode(trimmed)
        .map_err(|err| format!("failed to decode wallet base64: {err}"))?;
    let mut slab = NounSlab::<NockJammer>::new();
    let noun = slab
        .cue_into(Bytes::from(bytes))
        .map_err(|err| format!("failed to cue wallet noun: {err}"))?;
    WalletTransaction::from_noun(&noun)
        .map_err(|err| format!("failed to decode wallet transaction noun: {err}"))
}

fn parse_lock_from_json(value: &JsonValue) -> Result<SpendCondition, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "noteData.lock must be an object".to_string())?;
    let version = obj
        .get("version")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "lock.version missing".to_string())?;
    if version != 0 {
        return Err(format!("unsupported lock version: {}", version));
    }
    let primitives = obj
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "lock.data missing".to_string())?;
    if primitives.is_empty() {
        return Err("lock.data must contain at least one primitive".into());
    }
    let mut parsed = Vec::with_capacity(primitives.len());
    for item in primitives {
        parsed.push(parse_lock_primitive(item)?);
    }
    Ok(SpendCondition { p: parsed })
}

fn parse_lock_primitive(value: &JsonValue) -> Result<LockPrimitive, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "lock primitive must be an object".to_string())?;
    if let Some(pkh_val) = obj.get("pkh") {
        let body = parse_pkh_body(pkh_val)?;
        return Ok(LockPrimitive {
            header: "pkh".to_string(),
            body: LockPrimitiveBody::Pkh(body),
        });
    }
    if let Some(tim_val) = obj.get("tim") {
        let body = parse_tim_body(tim_val)?;
        return Ok(LockPrimitive {
            header: "tim".to_string(),
            body: LockPrimitiveBody::Tim(body),
        });
    }
    if let Some(hax_val) = obj.get("hax") {
        let body = parse_hax_body(hax_val)?;
        return Ok(LockPrimitive {
            header: "hax".to_string(),
            body: LockPrimitiveBody::Hax(body),
        });
    }
    if obj.contains_key("brn") {
        return Ok(LockPrimitive {
            header: "brn".to_string(),
            body: LockPrimitiveBody::Brn(tx_types::transaction_types_v1::Brn { value: 0 }),
        });
    }
    Err("unsupported lock primitive object".into())
}

fn parse_pkh_body(value: &JsonValue) -> Result<tx_types::transaction_types_v1::Pkh, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "pkh primitive must be an object".to_string())?;
    let m = obj
        .get("m")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "pkh.m missing".to_string())?;
    let hashes = obj
        .get("h")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "pkh.h missing".to_string())?;
    if hashes.is_empty() {
        return Err("pkh.h must contain at least one hash".into());
    }
    let mut set = ZSet::new();
    for entry in hashes {
        let addr = entry
            .as_str()
            .ok_or_else(|| "pkh.h entries must be strings".to_string())?;
        let hash =
            parse_hash(addr).map_err(|e| format!("invalid pk hash '{}' in lock: {e}", addr))?;
        set.put(hash);
    }
    Ok(tx_types::transaction_types_v1::Pkh { m, h: set })
}

fn parse_tim_body(value: &JsonValue) -> Result<Tim, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "tim primitive must be an object".to_string())?;
    let rel = obj
        .get("relative")
        .map(parse_timelock_range)
        .transpose()?
        .unwrap_or(TimelockRange {
            min: None,
            max: None,
        });
    let abs = obj
        .get("absolute")
        .map(parse_timelock_range)
        .transpose()?
        .unwrap_or(TimelockRange {
            min: None,
            max: None,
        });
    Ok(Tim { rel, abs })
}

fn parse_timelock_range(value: &JsonValue) -> Result<TimelockRange, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "timelock range must be an object".to_string())?;
    let min = obj
        .get("min")
        .and_then(|v| v.as_i64())
        .map(|v| {
            if v < 0 {
                Err("timelock min must be non-negative".to_string())
            } else {
                Ok(PageNumber { value: v as u64 })
            }
        })
        .transpose()?;
    let max = obj
        .get("max")
        .and_then(|v| v.as_i64())
        .map(|v| {
            if v < 0 {
                Err("timelock max must be non-negative".to_string())
            } else {
                Ok(PageNumber { value: v as u64 })
            }
        })
        .transpose()?;
    Ok(TimelockRange { min, max })
}

fn parse_hax_body(value: &JsonValue) -> Result<tx_types::transaction_types_v1::Hax, String> {
    let hashes = value
        .as_array()
        .ok_or_else(|| "hax primitive must be an array".to_string())?;
    let mut set = ZSet::new();
    for entry in hashes {
        let addr = entry
            .as_str()
            .ok_or_else(|| "hax entries must be strings".to_string())?;
        let hash =
            parse_hash(addr).map_err(|e| format!("invalid hax hash '{}' in lock: {e}", addr))?;
        set.put(hash);
    }
    Ok(tx_types::transaction_types_v1::Hax { set })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::Wallet;
    use crate::TransactionRecipient;
    use nockblocks_api_v1::models::note_v1_object::Version as ApiNoteVersion;
    use serde_json::json;
    use std::collections::HashMap;

    const TEST_SEED: &str = "fluid ordinary worth width spatial program evoke defense fade unveil large dress comfort reason invest urge step fitness bleak worth pole eagle gap float";
    const SAMPLE_LAST_NAME: &str = "4jJAKoAcjNBAiN744wD7SvATBsGxdER71oVaW7hMfRhCLRD1RLqFeTX";
    const SAMPLE_RECIPIENT: &str = "94qwbi47JzTySWXpNpYkqDDvuTKLdP8mPDdZTZYnphNCLKSWYeEuFdt";
    const SAMPLE_CHANGE_PKH: &str = "4Lu3cSW34WPwvDkTwKh7xB6yMZrvh3bFW7w26UDJVdboxADGkr2bTnL";

    fn sample_note() -> NoteV1Object {
        let mut note_data = HashMap::new();
        note_data.insert(
            "lock".to_string(),
            json!({
                "version": 0,
                "data": [
                    {
                        "pkh": {
                            "h": [SAMPLE_CHANGE_PKH],
                            "m": 1
                        }
                    }
                ]
            }),
        );
        NoteV1Object {
            version: ApiNoteVersion::Variant1,
            origin_page: 40635,
            first_name: NName::first_v1(
                crate::planner::pkh_lock(&Hash::from_b58(SAMPLE_CHANGE_PKH).unwrap()).to_hash(),
            )
            .to_b58(),
            last_name: SAMPLE_LAST_NAME.to_string(),
            note_data,
            assets: 6_553_600,
            is_coinbase: false,
            spent_on: None,
        }
    }

    fn sample_recipients() -> Vec<TransactionRecipient> {
        vec![TransactionRecipient {
            address: SAMPLE_RECIPIENT.to_string(),
            gift: 655_360,
            bridge_evm_address: None,
        }]
    }

    /// Create a large note with enough assets for bridge testing (>100k NOCK)
    fn sample_large_note() -> NoteV1Object {
        let mut note_data = HashMap::new();
        note_data.insert(
            "lock".to_string(),
            json!({
                "version": 0,
                "data": [
                    {
                        "pkh": {
                            "h": [SAMPLE_CHANGE_PKH],
                            "m": 1
                        }
                    }
                ]
            }),
        );
        NoteV1Object {
            version: ApiNoteVersion::Variant1,
            origin_page: 40635,
            first_name: NName::first_v1(
                crate::planner::pkh_lock(&Hash::from_b58(SAMPLE_CHANGE_PKH).unwrap()).to_hash(),
            )
            .to_b58(),
            last_name: SAMPLE_LAST_NAME.to_string(),
            note_data,
            // 200k NOCK = 200,000 * 65536 nicks
            assets: 200_000 * 65536,
            is_coinbase: false,
            spent_on: None,
        }
    }

    #[test]
    fn reconstructed_lock_is_excluded_from_input_commitment() {
        let mut api_note = sample_note();
        let disclosed = convert_note_v1(api_note.clone()).unwrap();
        api_note.note_data.get_mut("lock").unwrap()["source"] = json!("reconstructed");
        let inferred = convert_note_v1(api_note.clone()).unwrap();
        api_note.note_data.clear();
        let on_chain = convert_note_v1(api_note).unwrap();
        assert_eq!(inferred.to_hash(), on_chain.to_hash());
        assert_ne!(disclosed.to_hash(), on_chain.to_hash());
        assert_eq!(inferred.note_data.map.wyt(), 0);
        assert_eq!(disclosed.note_data.map.wyt(), 1);
    }

    #[test]
    fn sends_and_bridges_bind_seeds_to_notes_without_reconstructed_metadata() {
        let wallet = Wallet::from_seedphrase(TEST_SEED.into()).unwrap();
        let source = wallet.public_key_hash().unwrap();
        for coinbase in [false, true] {
            let mut api_note = sample_large_note();
            if coinbase {
                let mut lock = crate::planner::pkh_lock(&source);
                lock.p.push(LockPrimitive {
                    header: "tim".into(),
                    body: LockPrimitiveBody::Tim(tx_types::Tim {
                        rel: tx_types::TimelockRange {
                            min: Some(PageNumber { value: 100 }),
                            max: None,
                        },
                        abs: tx_types::TimelockRange { min: None, max: None },
                    }),
                });
                api_note.first_name = NName::first_v1(lock.to_hash()).to_b58();
                api_note.is_coinbase = true;
                api_note.note_data.get_mut("lock").unwrap()["data"]
                    .as_array_mut().unwrap().push(json!({
                        "tim": {"relative": {"min": 100, "max": null},
                                "absolute": {"min": null, "max": null}}
                    }));
            }
            api_note.note_data.get_mut("lock").unwrap()["source"] = json!("reconstructed");
            let mut on_chain_api = api_note.clone();
            on_chain_api.note_data.clear();
            let on_chain = convert_note_v1(on_chain_api).unwrap();
            for bridge in [false, true] {
                let recipients = if bridge {
                    vec![TransactionRecipient {
                        address: BRIDGE_LOCK_ROOT.into(),
                        gift: 100_000 * 65536,
                        bridge_evm_address: Some("0x1234567890abcdef1234567890abcdef12345678".into()),
                    }]
                } else { sample_recipients() };
                let built = build_transaction_from_api_with_options(
                    &wallet, vec![api_note.clone()], recipients,
                    BuildTxOptions { private_outputs: false, chain_height: Some(153880) },
                ).unwrap();
                let tx = decode_wallet_transaction_base64(&built.base64_tx).unwrap();
                assert_eq!(tx.spends.map.wyt(), 1);
                for (name, spend) in tx.spends.map.tap() {
                    assert_eq!(name, on_chain.name);
                    let SpendBody::V1(body) = spend.body else { panic!("expected V1 spend") };
                    assert!(body.seeds.set.wyt() > 0);
                    for seed in body.seeds.set.tap() {
                        assert_eq!(seed.parent_hash, on_chain.to_hash());
                    }
                }
                let inspected = crate::tx_jam::inspect_tx_jam(&built.base64_tx).unwrap();
                assert!(inspected.spends.iter().all(|spend| spend.is_fully_signed));
            }
        }
    }

    #[test]
    fn build_transaction_round_trip() {
        let wallet =
            Wallet::from_seedphrase(TEST_SEED.to_string()).expect("seed phrase should parse");
        let notes = vec![sample_note()];
        let recipients = sample_recipients();

        let built = build_transaction_from_api(&wallet, notes, recipients)
            .expect("transaction should build");

        assert!(!built.base64_tx.trim().is_empty());

        let wallet_tx = decode_wallet_transaction_base64(&built.base64_tx).expect("decode wallet");
        assert_eq!(wallet_tx.name, built.tx_id);
        let inspected =
            crate::tx_jam::inspect_tx_jam(&built.base64_tx).expect("inspect signed transaction");
        assert!(inspected.spends.iter().all(|spend| spend.is_fully_signed));
        assert_eq!(compute_tx_id_v1(&wallet_tx.spends).to_b58(), wallet_tx.name);
        assert_eq!(wallet_tx.spends.map.wyt(), 1);

        let reencoded = encode_wallet_transaction(&wallet_tx);
        assert_eq!(reencoded.trim(), built.base64_tx.trim());
    }

    #[test]
    fn max_payment_covers_fee_and_leaves_no_change() {
        let wallet = Wallet::from_seedphrase(TEST_SEED.into()).unwrap();
        let notes = vec![sample_note()];
        let source = wallet.public_key_hash().unwrap();
        let destination = Hash::from_b58(SAMPLE_RECIPIENT).unwrap();
        let amount = max_payment(
            &source,
            notes.clone(),
            &destination,
            BuildTxOptions::default(),
        )
        .unwrap();
        let built = build_transaction_from_api(
            &wallet,
            notes.clone(),
            vec![TransactionRecipient {
                address: SAMPLE_RECIPIENT.into(),
                gift: amount,
                bridge_evm_address: None,
            }],
        )
        .unwrap();
        assert_eq!(amount + built.fee_paid, notes[0].assets as u64);
        let inspected = crate::tx_jam::inspect_tx_jam(&built.base64_tx).unwrap();
        assert_eq!(inspected.outputs.len(), 1);
        assert_eq!(inspected.outputs[0].gift, amount);
    }

    #[test]
    fn hardware_draft_requires_valid_signatures_and_unchanged_payments() {
        let wallet = Wallet::from_seedphrase(TEST_SEED.into()).unwrap();
        let source = wallet.public_key_hash().unwrap();
        let draft =
            build_unsigned_from_api(&source, vec![sample_note()], sample_recipients(), None)
                .unwrap();
        let signed = crate::tx_jam::sign_tx_jam(&wallet, &draft.base64_tx).unwrap();
        crate::tx_jam::verify_signed_draft(&draft.base64_tx, &signed.base64_tx).unwrap();
        assert!(crate::tx_jam::verify_signed_draft(&draft.base64_tx, &draft.base64_tx).is_err());
        let other = build_transaction_from_api(
            &wallet,
            vec![sample_note()],
            vec![TransactionRecipient {
                address: SAMPLE_RECIPIENT.into(),
                gift: 500_000,
                bridge_evm_address: None,
            }],
        )
        .unwrap();
        assert!(crate::tx_jam::verify_signed_draft(&draft.base64_tx, &other.base64_tx).is_err());
    }

    #[test]
    fn build_bridge_transaction() {
        let wallet =
            Wallet::from_seedphrase(TEST_SEED.to_string()).expect("seed phrase should parse");
        let notes = vec![sample_large_note()];

        // Bridge recipient with EVM address
        let bridge_amount: u64 = 100_000 * 65536; // 100k NOCK
        let evm_address = "0x1234567890abcdef1234567890abcdef12345678".to_string();
        let recipients = vec![TransactionRecipient {
            address: BRIDGE_LOCK_ROOT.to_string(),
            gift: bridge_amount,
            bridge_evm_address: Some(evm_address.clone()),
        }];

        let built = build_transaction_from_api(&wallet, notes, recipients)
            .expect("bridge transaction should build");

        assert!(!built.base64_tx.trim().is_empty());

        let wallet_tx = decode_wallet_transaction_base64(&built.base64_tx).expect("decode wallet");
        assert_eq!(wallet_tx.name, built.tx_id);

        // Verify the bridge output has correct lock root and noteData
        let bridge_lock_root = Hash::from_b58(BRIDGE_LOCK_ROOT).expect("parse bridge lock root");

        let mut found_bridge_output = false;
        for (_, spend) in wallet_tx.spends.map.tap() {
            match spend.body {
                SpendBody::V1(ref body) => {
                    for seed in body.seeds.set.tap() {
                        if seed.lock_root == bridge_lock_root && seed.gift.value == bridge_amount {
                            found_bridge_output = true;
                            // Verify noteData contains "bridge" key
                            assert!(
                                seed.note_data.map.has(&"bridge".to_string()),
                                "bridge output should have noteData.bridge"
                            );
                            // Verify noteData does NOT contain "lock" key
                            assert!(
                                !seed.note_data.map.has(&"lock".to_string()),
                                "bridge output should not have noteData.lock"
                            );
                        }
                    }
                }
                SpendBody::V0ToV1(ref body) => {
                    for seed in body.seeds.set.tap() {
                        if seed.lock_root == bridge_lock_root && seed.gift.value == bridge_amount {
                            found_bridge_output = true;
                            assert!(
                                seed.note_data.map.has(&"bridge".to_string()),
                                "bridge output should have noteData.bridge"
                            );
                        }
                    }
                }
                _ => {}
            }
        }

        assert!(
            found_bridge_output,
            "transaction should contain a bridge output with correct lock root"
        );
    }

    #[test]
    fn bridge_transaction_rejects_insufficient_amount() {
        let recipients = vec![TransactionRecipient {
            address: BRIDGE_LOCK_ROOT.to_string(),
            gift: 50_000 * 65536, // 50k NOCK, less than 100k minimum
            bridge_evm_address: Some("0x1234567890abcdef1234567890abcdef12345678".to_string()),
        }];

        let result = convert_recipients(recipients);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("100,000 NOCK"),
            "error should mention minimum amount"
        );
    }

    #[test]
    fn bridge_transaction_rejects_invalid_evm_address() {
        let recipients = vec![TransactionRecipient {
            address: BRIDGE_LOCK_ROOT.to_string(),
            gift: 100_000 * 65536,
            bridge_evm_address: Some("0xinvalid".to_string()),
        }];

        let result = convert_recipients(recipients);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().contains("invalid EVM address"),
            "error should mention invalid EVM address"
        );
    }

    #[test]
    fn bridge_payload_matches_rpc_production_fixture() {
        // nockblocks-api service/converters_v1.rs: test_process_bridge_notedata_known_good.
        let data = build_bridge_note_data("0xB74f4D97d40f49EafF06c3a9E97907362C9063Fa").unwrap();
        assert_eq!(
            general_purpose::STANDARD.encode(&data.map.get(&"bridge".into()).unwrap().p),
            "GfDFwubKAXozK72soz6YOMC//2u8sVHeKgQEmU1Ptw=="
        );
    }

    #[test]
    fn bridge_review_verifies_serialized_intent_and_rejects_mutations() {
        let wallet = Wallet::from_seedphrase(TEST_SEED.into()).unwrap();
        let source = wallet.public_key_hash().unwrap();
        let address = "0xb74f4d97d40f49eaff06c3a9e97907362c9063fa";
        let recipients = |evm: &str, gift: u64| vec![TransactionRecipient {
            address: BRIDGE_LOCK_ROOT.into(), gift, bridge_evm_address: Some(evm.into()),
        }];
        let tx = build_transaction_from_api(&wallet, vec![sample_large_note()], recipients(address, BRIDGE_MINIMUM_NICKS)).unwrap();
        verify_bridge_intent(&tx.base64_tx, &source, recipients(address, BRIDGE_MINIMUM_NICKS), tx.fee_paid).unwrap();
        assert!(verify_bridge_intent(&tx.base64_tx, &source, recipients("0x1111111111111111111111111111111111111111", BRIDGE_MINIMUM_NICKS), tx.fee_paid).is_err());
        assert!(verify_bridge_intent(&tx.base64_tx, &source, recipients(address, BRIDGE_MINIMUM_NICKS+1), tx.fee_paid).is_err());
        assert!(verify_bridge_intent(&tx.base64_tx, &source, recipients(address, BRIDGE_MINIMUM_NICKS), tx.fee_paid+1).is_err());
        assert!(verify_bridge_intent(&tx.base64_tx, &Hash::from_b58(SAMPLE_RECIPIENT).unwrap(), recipients(address, BRIDGE_MINIMUM_NICKS), tx.fee_paid).is_err());
        assert!(build_transaction_from_api_with_options(&wallet, vec![sample_large_note()], recipients(address, BRIDGE_MINIMUM_NICKS), BuildTxOptions { private_outputs: true, chain_height: None }).is_err());
    }

    #[test]
    fn bridge_rejects_missing_metadata_wrong_lock_and_zero_destination() {
        for (address, evm) in [
            (BRIDGE_LOCK_ROOT, None),
            (BRIDGE_LOCK_ROOT, Some("0x0000000000000000000000000000000000000000")),
            ("94qwbi47JzTySWXpNpYkqDDvuTKLdP8mPDdZTZYnphNCLKSWYeEuFdt", Some("0x1234567890abcdef1234567890abcdef12345678")),
        ] {
            assert!(convert_recipients(vec![TransactionRecipient {
                address: address.into(), gift: BRIDGE_MINIMUM_NICKS,
                bridge_evm_address: evm.map(str::to_owned),
            }]).is_err());
        }
        assert!(convert_recipients(vec![
            TransactionRecipient {
                address: BRIDGE_LOCK_ROOT.into(), gift: BRIDGE_MINIMUM_NICKS,
                bridge_evm_address: Some("0x1234567890abcdef1234567890abcdef12345678".into()),
            },
            TransactionRecipient {
                address: SAMPLE_RECIPIENT.into(), gift: 1, bridge_evm_address: None,
            },
        ]).is_err());
    }

    #[test]
    fn evm_address_to_based_conversion() {
        // Test with a smaller EVM address that fits verification
        // Use an address with mostly zeros to make verification easier
        let evm_addr = "0x0000000000000000000000000000000000000001";
        let based = evm_address_to_based(evm_addr).expect("should convert");

        // For address 0x1, the based representation should be [1, 0, 0]
        assert_eq!(based[0], 1);
        assert_eq!(based[1], 0);
        assert_eq!(based[2], 0);

        // Test with a larger address
        let evm_addr2 = "0xdead000000000000000000000000000000000000";
        let based2 = evm_address_to_based(evm_addr2).expect("should convert");

        // Verify the conversion produces valid based field elements
        // Each element should be < p
        let p = BASE_FIELD_PRIME as u64;
        assert!(based2[0] < p, "a should be < p");
        assert!(based2[1] < p, "b should be < p");
        assert!(based2[2] < p, "c should be < p");

        // Verify round-trip by reconstructing and checking it matches
        // the original when encoded back to hex
        // Note: Full verification would require big integer math
        // For now, just verify the structure is correct
        assert!(
            based2[0] != 0 || based2[1] != 0 || based2[2] != 0,
            "at least one element should be non-zero"
        );
    }
}
