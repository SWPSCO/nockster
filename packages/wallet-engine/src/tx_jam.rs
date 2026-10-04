use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use nockapp::noun::slab::{NockJammer, NounSlab};
use noun_serde::{NounDecode, NounEncode};
use serde::Serialize;
use std::collections::HashSet;
use tx_types::{
    compute_tx_id_v1,
    signer::schnorr_sign_digest,
    transaction_types::{Chal, RawTransaction, Sig, Spend, SpendBody},
    transaction_types_v1::{
        LockPrimitiveBody, PkhSignatureValue, RawTransactionV1, SeedV1, SpendsV1,
    },
    Hash, LockData, SpendCondition, ZMap,
};

use crate::tx_engine::WalletTransaction;
use crate::wallet::Wallet;

#[derive(Debug, Clone, NounEncode, NounDecode)]
struct WalletTransactionV1Bundle {
    pub version: u64,
    pub name: String,
    pub spends: SpendsV1,
    pub display: tx_types::UntypedNoun,
    pub witness_data: tx_types::UntypedNoun,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum JamFormat {
    WalletTransaction,
    RawTransactionV1,
    RawTransactionV0,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JamLockPkh {
    pub m: u64,
    pub pubkey_hashes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JamLockSummary {
    pub pkh: Option<JamLockPkh>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JamOutputSummary {
    pub lock_root: String,
    pub gift: u64,
    pub lock: Option<JamLockSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JamInspectResult {
    pub format: JamFormat,
    pub tx_id: String,
    pub outputs: Vec<JamOutputSummary>,
    pub fee_paid: Option<u64>,
    pub spends: Vec<JamSpendSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JamSignResult {
    pub format: JamFormat,
    pub tx_id: String,
    pub base64_tx: String,
    pub spends_signed: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JamSpendSummary {
    pub name: String,
    pub lock: Option<JamLockSummary>,
    pub signed_by: Vec<String>,
    pub is_fully_signed: bool,
}

enum DecodedJamTx {
    Wallet(WalletTransaction),
    WalletV1Bundle(WalletTransactionV1Bundle),
    RawV1(tx_types::transaction_types_v1::RawTransactionV1),
    RawV0(tx_types::transaction_types_v0::RawTransactionV0),
}

pub(crate) fn inspect_tx_jam(base64_jam: &str) -> Result<JamInspectResult, String> {
    let decoded = decode_tx(base64_jam)?;
    match decoded {
        DecodedJamTx::Wallet(wallet_tx) => {
            let tx_id = compute_tx_id_v1(&wallet_tx.spends).to_b58();
            let outputs = summarize_outputs(&wallet_tx.spends)?;
            let spends = summarize_spends(&wallet_tx.spends)?;
            Ok(JamInspectResult {
                format: JamFormat::WalletTransaction,
                tx_id,
                outputs,
                fee_paid: Some(total_fee(&wallet_tx.spends)?),
                spends,
            })
        }
        DecodedJamTx::WalletV1Bundle(wallet_tx) => {
            let tx_id = compute_tx_id_v1(&wallet_tx.spends).to_b58();
            let outputs = summarize_outputs(&wallet_tx.spends)?;
            let spends = summarize_spends(&wallet_tx.spends)?;
            Ok(JamInspectResult {
                format: JamFormat::WalletTransaction,
                tx_id,
                outputs,
                fee_paid: Some(total_fee(&wallet_tx.spends)?),
                spends,
            })
        }
        DecodedJamTx::RawV1(raw_tx) => {
            let tx_id = raw_tx.id.to_b58();
            let outputs = summarize_outputs(&raw_tx.spends)?;
            let spends = summarize_spends(&raw_tx.spends)?;
            Ok(JamInspectResult {
                format: JamFormat::RawTransactionV1,
                tx_id,
                outputs,
                fee_paid: Some(total_fee(&raw_tx.spends)?),
                spends,
            })
        }
        DecodedJamTx::RawV0(raw_tx) => Ok(JamInspectResult {
            format: JamFormat::RawTransactionV0,
            tx_id: raw_tx.id.to_b58(),
            outputs: Vec::new(),
            fee_paid: None,
            spends: Vec::new(),
        }),
    }
}

pub(crate) fn sign_tx_jam(wallet: &Wallet, base64_jam: &str) -> Result<JamSignResult, String> {
    sign_tx_jam_inner(wallet, base64_jam, None)
}

pub(crate) fn sign_tx_jam_selected(
    wallet: &Wallet,
    base64_jam: &str,
    selected_spends: Vec<String>,
) -> Result<JamSignResult, String> {
    let selection = selected_spends
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect::<HashSet<_>>();

    if selection.is_empty() {
        return Err("select at least one spend to sign".into());
    }
    let selection_ref = Some(&selection);

    sign_tx_jam_inner(wallet, base64_jam, selection_ref)
}

pub(crate) fn to_raw_tx_jam(base64_jam: &str) -> Result<String, String> {
    let trimmed = base64_jam.trim();
    if trimmed.is_empty() {
        return Err("transaction payload is missing".into());
    }

    let decoded = decode_tx(trimmed)?;
    match decoded {
        DecodedJamTx::Wallet(wallet_tx) => {
            let spends = wallet_tx.spends;
            let tx_id_hash = compute_tx_id_v1(&spends);
            let raw_tx = RawTransactionV1 {
                version: 1,
                id: tx_id_hash,
                spends,
            };
            Ok(encode_base64(&raw_tx))
        }
        DecodedJamTx::WalletV1Bundle(wallet_tx) => {
            let spends = wallet_tx.spends;
            let tx_id_hash = compute_tx_id_v1(&spends);
            let raw_tx = RawTransactionV1 {
                version: 1,
                id: tx_id_hash,
                spends,
            };
            Ok(encode_base64(&raw_tx))
        }
        DecodedJamTx::RawV1(_raw_tx) => Ok(trimmed.to_string()),
        DecodedJamTx::RawV0(_raw_tx) => Ok(trimmed.to_string()),
    }
}

pub(crate) fn to_wallet_tx_jam(base64_jam: &str) -> Result<String, String> {
    let trimmed = base64_jam.trim();
    if trimmed.is_empty() {
        return Err("transaction payload is missing".into());
    }

    let decoded = decode_tx(trimmed)?;
    match decoded {
        DecodedJamTx::Wallet(mut wallet_tx) => {
            let tx_id = compute_tx_id_v1(&wallet_tx.spends).to_b58();
            wallet_tx.name = tx_id;
            Ok(encode_base64(&wallet_tx))
        }
        DecodedJamTx::WalletV1Bundle(mut wallet_tx) => {
            let tx_id = compute_tx_id_v1(&wallet_tx.spends).to_b58();
            wallet_tx.name = tx_id;
            Ok(encode_base64(&wallet_tx))
        }
        DecodedJamTx::RawV1(raw_tx) => {
            let tx_id = compute_tx_id_v1(&raw_tx.spends).to_b58();
            let wallet_tx = WalletTransaction {
                name: tx_id,
                spends: raw_tx.spends,
            };
            Ok(encode_base64(&wallet_tx))
        }
        DecodedJamTx::RawV0(_raw_tx) => {
            Err("toWalletTxJam does not yet support V0 raw transactions".to_string())
        }
    }
}

fn sign_tx_jam_inner(
    wallet: &Wallet,
    base64_jam: &str,
    selected_spends: Option<&HashSet<String>>,
) -> Result<JamSignResult, String> {
    let signing_key = wallet.private_key_t8()?;
    let schnorr_pubkey = wallet.schnorr_pubkey()?;
    let wallet_pubkey_hash = schnorr_pubkey.to_hash();

    let decoded = decode_tx(base64_jam)?;
    match decoded {
        DecodedJamTx::Wallet(mut wallet_tx) => {
            let (spends, spends_signed) = sign_spends_v1(
                wallet_tx.spends.clone(),
                &signing_key,
                &schnorr_pubkey,
                &wallet_pubkey_hash,
                selected_spends,
            )?;
            let tx_id = compute_tx_id_v1(&spends).to_b58();
            wallet_tx.name = tx_id.clone();
            wallet_tx.spends = spends;
            Ok(JamSignResult {
                format: JamFormat::WalletTransaction,
                tx_id,
                base64_tx: encode_base64(&wallet_tx),
                spends_signed,
            })
        }
        DecodedJamTx::WalletV1Bundle(mut wallet_tx) => {
            let (spends, spends_signed) = sign_spends_v1(
                wallet_tx.spends.clone(),
                &signing_key,
                &schnorr_pubkey,
                &wallet_pubkey_hash,
                selected_spends,
            )?;
            let tx_id = compute_tx_id_v1(&spends).to_b58();
            wallet_tx.name = tx_id.clone();
            wallet_tx.spends = spends;
            Ok(JamSignResult {
                format: JamFormat::WalletTransaction,
                tx_id,
                base64_tx: encode_base64(&wallet_tx),
                spends_signed,
            })
        }
        DecodedJamTx::RawV1(mut raw_tx) => {
            let (spends, spends_signed) = sign_spends_v1(
                raw_tx.spends.clone(),
                &signing_key,
                &schnorr_pubkey,
                &wallet_pubkey_hash,
                selected_spends,
            )?;
            let tx_id_hash = compute_tx_id_v1(&spends);
            let tx_id = tx_id_hash.to_b58();
            raw_tx.id = tx_id_hash;
            raw_tx.spends = spends;
            Ok(JamSignResult {
                format: JamFormat::RawTransactionV1,
                tx_id,
                base64_tx: encode_base64(&raw_tx),
                spends_signed,
            })
        }
        DecodedJamTx::RawV0(_raw_tx) => {
            Err("signTxJam does not yet support V0 raw transactions".into())
        }
    }
}

fn decode_tx(base64_jam: &str) -> Result<DecodedJamTx, String> {
    let trimmed = base64_jam.trim();
    if trimmed.len() > 1_400_000 {
        return Err("transaction exceeds size limit".into());
    }
    let bytes = general_purpose::STANDARD
        .decode(trimmed)
        .map_err(|err| format!("failed to decode base64 jam: {err}"))?;

    let mut slab = NounSlab::<NockJammer>::new();
    let noun = slab
        .cue_into(Bytes::from(bytes))
        .map_err(|err| format!("failed to cue jammed noun: {err}"))?;

    // Attempt decodes in an order that avoids panics from mismatched shapes.
    if let Ok(raw_tx) = RawTransactionV1::from_noun(&noun) {
        validate_spends(&raw_tx.spends)?;
        if raw_tx.version != 1 || raw_tx.id != compute_tx_id_v1(&raw_tx.spends) {
            return Err("invalid raw transaction version or ID".into());
        }
        return Ok(DecodedJamTx::RawV1(raw_tx));
    }

    if let Ok(raw_tx) = RawTransaction::from_noun(&noun) {
        return match raw_tx {
            RawTransaction::V1(v1) => {
                validate_spends(&v1.spends)?;
                if v1.version != 1 || v1.id != compute_tx_id_v1(&v1.spends) {
                    return Err("invalid raw transaction ID".into());
                }
                Ok(DecodedJamTx::RawV1(v1))
            }
            RawTransaction::V0(v0) => Ok(DecodedJamTx::RawV0(v0)),
        };
    }

    if let Ok(wallet_tx) = WalletTransactionV1Bundle::from_noun(&noun) {
        if wallet_tx.version != 1 {
            return Err("unsupported wallet transaction version".into());
        }
        validate_spends(&wallet_tx.spends)?;
        return Ok(DecodedJamTx::WalletV1Bundle(wallet_tx));
    }

    if let Ok(wallet_tx) = WalletTransaction::from_noun(&noun) {
        validate_spends(&wallet_tx.spends)?;
        return Ok(DecodedJamTx::Wallet(wallet_tx));
    }

    Err(
        "unrecognized noun shape; cannot decode as wallet transaction or raw transaction"
            .to_string(),
    )
}

fn validate_spends(spends: &SpendsV1) -> Result<(), String> {
    if spends.map.wyt() == 0 {
        return Err("transaction has no spends".into());
    }
    let mut slab = NounSlab::<NockJammer>::new();
    crate::tx_engine::validate_based_noun(spends.to_noun(&mut slab))?;
    for (name, spend) in spends.map.tap() {
        if name.p.len() != 2 {
            return Err("invalid input note name".into());
        }
        let SpendBody::V1(body) = spend.body else {
            continue;
        };
        if spend.version != 1 {
            return Err("invalid spend version".into());
        }
        let proof = &body.witness.lmp;
        let mut axis = proof.axis();
        let mut root = proof.spend_condition().to_hash();
        if axis == 0 || (matches!(proof, tx_types::LockMerkleProof::Stub(_)) && axis != 1) {
            return Err("invalid proof axis".into());
        }
        for sibling in &proof.proof().path {
            if axis <= 1 {
                return Err("invalid proof path".into());
            }
            root = if axis & 1 == 0 {
                tx_types::hashing::hasher::hash_ten_cell(root, sibling.clone())
            } else {
                tx_types::hashing::hasher::hash_ten_cell(sibling.clone(), root)
            };
            axis >>= 1;
        }
        if axis != 1 || root != proof.proof().root || name.p[0] != tx_types::NName::first_v1(root) {
            return Err("invalid input lock proof".into());
        }
    }
    Ok(())
}

pub(crate) fn verify_signed_draft(draft: &str, signed: &str) -> Result<(), String> {
    fn spends(tx: DecodedJamTx) -> Result<SpendsV1, String> {
        match tx {
            DecodedJamTx::Wallet(tx) => Ok(tx.spends),
            DecodedJamTx::WalletV1Bundle(tx) => Ok(tx.spends),
            DecodedJamTx::RawV1(tx) => Ok(tx.spends),
            _ => Err("expected v1 transaction".into()),
        }
    }
    let expected = spends(decode_tx(draft)?)?;
    let actual = spends(decode_tx(signed)?)?;
    if summarize_spends(&actual)?
        .iter()
        .any(|s| !s.is_fully_signed)
    {
        return Err("transaction is not fully signed".into());
    }
    fn without_signatures(spends: SpendsV1) -> SpendsV1 {
        let mut map = ZMap::new();
        for (name, mut spend) in spends.map.tap() {
            if let SpendBody::V1(ref mut body) = spend.body {
                body.witness.pkh.map = ZMap::new();
            }
            map.put(name, spend);
        }
        SpendsV1 { map }
    }
    if compute_tx_id_v1(&without_signatures(expected))
        != compute_tx_id_v1(&without_signatures(actual))
    {
        return Err("signed transaction differs from the approved draft".into());
    }
    Ok(())
}

fn encode_base64<T: NounEncode>(value: &T) -> String {
    let mut slab = NounSlab::<NockJammer>::new();
    let noun = value.to_noun(&mut slab);
    slab.set_root(noun);
    let jammed = slab.jam();
    general_purpose::STANDARD.encode(jammed.as_ref())
}

fn format_note_name(name: &tx_types::NName) -> String {
    name.p
        .iter()
        .map(|hash| hash.to_b58())
        .collect::<Vec<_>>()
        .join("/")
}

fn total_fee(spends: &SpendsV1) -> Result<u64, String> {
    spends.map.tap().iter().try_fold(0u64, |sum, (_, spend)| {
        let fee = match &spend.body {
            SpendBody::V1(b) => b.fee.value,
            SpendBody::V0ToV1(b) => b.fee.value,
            SpendBody::V0(b) => b.fee.value,
        };
        sum.checked_add(fee).ok_or_else(|| "fee overflow".into())
    })
}

fn summarize_spends(spends: &SpendsV1) -> Result<Vec<JamSpendSummary>, String> {
    let mut out = Vec::new();

    for (name, spend) in spends.map.tap() {
        let SpendBody::V1(body) = spend.body.clone() else {
            continue;
        };

        let lock = extract_pkh_summary(body.witness.lmp.spend_condition())
            .map(|pkh| JamLockSummary { pkh: Some(pkh) });

        let mut signed_by = body
            .witness
            .pkh
            .map
            .tap()
            .into_iter()
            .filter(|(hash, value)| {
                value.pk.to_hash() == *hash
                    && tx_types::validation::schnorr_verify_digest(
                        value.pk.clone(),
                        body.compute_sig_hash(),
                        value.sig.clone(),
                    )
            })
            .map(|(hash, _)| hash.to_b58())
            .collect::<Vec<_>>();
        signed_by.sort();
        signed_by.dedup();

        let conditions = &body.witness.lmp.spend_condition().p;
        let is_fully_signed = conditions
            .iter()
            .any(|p| matches!(p.body, LockPrimitiveBody::Pkh(_)))
            && conditions.iter().all(|primitive| match &primitive.body {
                LockPrimitiveBody::Pkh(pkh) => {
                    pkh.m > 0
                        && pkh.m <= pkh.h.wyt() as u64
                        && pkh
                            .h
                            .iter()
                            .filter(|hash| signed_by.contains(&hash.to_b58()))
                            .count() as u64
                            >= pkh.m
                }
                LockPrimitiveBody::Tim(_) => true,
                _ => false,
            });

        out.push(JamSpendSummary {
            name: format_note_name(&name),
            lock,
            signed_by,
            is_fully_signed,
        });
    }

    Ok(out)
}

fn summarize_outputs(spends: &SpendsV1) -> Result<Vec<JamOutputSummary>, String> {
    let mut grouped: std::collections::BTreeMap<String, JamOutputSummary> =
        std::collections::BTreeMap::new();

    for (_, spend) in spends.map.tap() {
        let seeds = match spend.body {
            SpendBody::V1(body) => body.seeds.set.tap(),
            SpendBody::V0ToV1(body) => body.seeds.set.tap(),
            SpendBody::V0(_) => continue,
        };

        for seed in seeds {
            let lock_root = seed.lock_root.to_b58();
            let entry = grouped
                .entry(lock_root.clone())
                .or_insert_with(|| JamOutputSummary {
                    lock_root,
                    gift: 0,
                    lock: None,
                });

            entry.gift = entry
                .gift
                .checked_add(seed.gift.value)
                .ok_or("output amount overflow")?;

            if entry.lock.is_none() {
                if let Some(lock) = extract_seed_lock_summary(&seed)? {
                    entry.lock = Some(lock);
                }
            }
        }
    }

    Ok(grouped.into_values().collect())
}

fn extract_seed_lock_summary(seed: &SeedV1) -> Result<Option<JamLockSummary>, String> {
    let lock_key = "lock".to_string();
    let Some(lock_noun) = seed.note_data.map.get(&lock_key) else {
        return Ok(None);
    };

    let condition = decode_lock_condition(lock_noun)?;
    if condition.to_hash() != seed.lock_root {
        return Err("output lock data does not match its lock root".into());
    }
    Ok(Some(JamLockSummary {
        pkh: extract_pkh_summary(&condition),
    }))
}

fn decode_lock_condition(lock_noun: &tx_types::UntypedNoun) -> Result<SpendCondition, String> {
    let mut slab = NounSlab::<NockJammer>::new();
    let noun = slab
        .cue_into(lock_noun.p.clone())
        .map_err(|err| format!("failed to cue noteData.lock noun: {err}"))?;
    let lock_data = LockData::from_noun(&noun)
        .map_err(|err| format!("failed to decode lock-data noun: {err}"))?;
    match lock_data {
        LockData::V0(condition) => Ok(condition),
    }
}

fn extract_pkh_summary(condition: &SpendCondition) -> Option<JamLockPkh> {
    for primitive in &condition.p {
        if let LockPrimitiveBody::Pkh(pkh) = &primitive.body {
            let pubkey_hashes = pkh.h.iter().map(|hash| hash.to_b58()).collect::<Vec<_>>();
            return Some(JamLockPkh {
                m: pkh.m,
                pubkey_hashes,
            });
        }
    }
    None
}

fn spend_is_signable(spend: &Spend, wallet_pubkey_hash: &Hash) -> bool {
    let SpendBody::V1(body) = &spend.body else {
        return false;
    };

    for primitive in &body.witness.lmp.spend_condition().p {
        if let LockPrimitiveBody::Pkh(pkh) = &primitive.body {
            if pkh.h.has(wallet_pubkey_hash) {
                return true;
            }
        }
    }

    false
}

fn sign_spends_v1(
    spends: SpendsV1,
    signing_key: &tx_types::transaction_types::T8,
    schnorr_pubkey: &tx_types::transaction_types::SchnorrPubkey,
    wallet_pubkey_hash: &Hash,
    selected_spends: Option<&HashSet<String>>,
) -> Result<(SpendsV1, u64), String> {
    let mut out = ZMap::new();
    let mut spends_signed = 0u64;

    for (name, mut spend) in spends.map.tap() {
        let should_sign = selected_spends.map_or(true, |selection| {
            selection.contains(&format_note_name(&name))
        });

        if should_sign && spend_is_signable(&spend, wallet_pubkey_hash) {
            let sig_hash = spend.body.sig_hash();
            let (chal, sig) = schnorr_sign_digest(
                signing_key.clone(),
                schnorr_pubkey.clone(),
                sig_hash.clone(),
            );
            let schnorr_sig = tx_types::transaction_types::SchnorrSignature {
                chal: Chal { values: chal },
                sig: Sig { values: sig },
            };

            if let SpendBody::V1(mut body) = spend.body.clone() {
                body.witness.pkh.map.put(
                    wallet_pubkey_hash.clone(),
                    PkhSignatureValue {
                        pk: schnorr_pubkey.clone(),
                        sig: schnorr_sig,
                    },
                );
                spend.body = SpendBody::V1(body);
                spends_signed += 1;
            }
        }

        out.put(name, spend);
    }

    Ok((SpendsV1 { map: out }, spends_signed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::{payment, pkh_lock, plan_spends};
    use tx_types::{Coins, NName, NNoteV1, NoteData, PageNumber};

    fn draft() -> SpendsV1 {
        let owner = Hash { values: [1; 5] };
        let note = NNoteV1 {
            version: 1,
            origin_page: PageNumber { value: 1000 },
            name: NName {
                p: vec![
                    NName::first_v1(pkh_lock(&owner).to_hash()),
                    Hash { values: [2; 5] },
                ],
            },
            note_data: NoteData { map: ZMap::new() },
            assets: Coins { value: 1_000_000 },
        };
        plan_spends(
            &[note],
            &[payment(&owner, 800_000, false)],
            200_000,
            &owner,
            false,
            None,
        )
        .unwrap()
    }

    #[test]
    fn raw_id_and_input_proof_are_verified() {
        let spends = draft();
        let raw = RawTransactionV1 {
            version: 1,
            id: compute_tx_id_v1(&spends),
            spends: spends.clone(),
        };
        assert!(inspect_tx_jam(&encode_base64(&raw)).is_ok());
        let forged = RawTransactionV1 {
            id: Hash { values: [9; 5] },
            ..raw
        };
        assert!(inspect_tx_jam(&encode_base64(&forged)).is_err());
        let mut changed = ZMap::new();
        for (mut name, spend) in spends.map.tap() {
            name.p[0] = Hash { values: [8; 5] };
            changed.put(name, spend);
        }
        let forged = WalletTransaction {
            name: "display name".into(),
            spends: SpendsV1 { map: changed },
        };
        assert!(inspect_tx_jam(&encode_base64(&forged)).is_err());
    }

    #[test]
    fn wallet_display_name_is_not_the_transaction_id() {
        let spends = draft();
        let expected = compute_tx_id_v1(&spends).to_b58();
        let tx = WalletTransaction {
            name: "payment".into(),
            spends,
        };
        let inspected = inspect_tx_jam(&encode_base64(&tx)).unwrap();
        assert_eq!(inspected.tx_id, expected);
        assert_eq!(inspected.fee_paid, Some(200_000));
        assert!(!inspected.spends[0].is_fully_signed);
    }

    #[test]
    fn forged_signature_is_not_counted() {
        let mut map = ZMap::new();
        for (name, mut spend) in draft().map.tap() {
            if let SpendBody::V1(ref mut body) = spend.body {
                body.witness.pkh.map.put(
                    Hash { values: [1; 5] },
                    PkhSignatureValue {
                        pk: tx_types::SchnorrPubkey {
                            x: tx_types::F6LT { values: [1; 6] },
                            y: tx_types::F6LT { values: [1; 6] },
                            inf: false,
                        },
                        sig: tx_types::SchnorrSignature {
                            chal: Chal {
                                values: tx_types::T8 { values: [0; 8] },
                            },
                            sig: Sig {
                                values: tx_types::T8 { values: [0; 8] },
                            },
                        },
                    },
                );
            }
            map.put(name, spend);
        }
        let summaries = summarize_spends(&SpendsV1 { map }).unwrap();
        assert!(summaries[0].signed_by.is_empty());
        assert!(!summaries[0].is_fully_signed);
    }
}
