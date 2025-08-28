use crate::config::{Config};
use reqwest::header::{ORIGIN, CONTENT_TYPE};
use tracing::{info};
use anyhow::Result;
use serde::{Deserialize};
use serde_json::json;
use tx_types::transaction_types::*;
use tx_types::collections::{ZSet};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RPCTimelockRange { min: Option<u64>, max: Option<u64> }

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RPCTimelock { absolute: RPCTimelockRange, relative: RPCTimelockRange }

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RPCName { first_name: String, last_name: String }

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RPCLock { m: u64, pubkeys: Vec<String> }

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RPCNote {
    version: u64,
    origin_page: u64,
    timelock: Option<RPCTimelock>,
    name: RPCName,
    lock: RPCLock,
    source_hash: String,
    is_coinbase: bool,
    assets: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RPCResult { _nicks: u64, notes: Vec<RPCNote> }

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RPCNotes { result: RPCResult }

pub async fn retrieve_notes_for_address(
    cfg: &Config,
    source_address: &str,
    total_amount: u64,
    fee_per_input: u64,
    rpc_limit: u64,
) -> Result<Vec<NNote>> {
    info!("retrieving notes for {source_address}");
    let client = reqwest::Client::new();
    let query = json!({
        "jsonrpc":"2.0",
        "method":"getNotes",
        "params":[{ "address": source_address, "limit": rpc_limit, "offset": 0 }],
        "id":"nicker"
    });

    let resp = client
        .post(cfg.rpc_url.clone())
        .header(CONTENT_TYPE, "application/json")
        .header(ORIGIN, &cfg.origin_header)
        .body(query.to_string())
        .send()
        .await?;

    let body = resp.text().await?;
    let res: RPCNotes = serde_json::from_str(&body)?;
    let mut notes = res.result.notes;

    // Sort: high assets first, then smallest origin_page
    notes.sort_by(|a, b| b.assets.cmp(&a.assets).then_with(|| a.origin_page.cmp(&b.origin_page)));

    // Estimate overhead: assume up to 10 inputs by default, each paying fee_per_input
    // (keeps spirit of original code where fee was per input)
    let fee_estimate = fee_per_input.saturating_mul(10);
    let need = total_amount.saturating_add(fee_estimate);

    // Pick enough notes to cover need (we later compute exact change + exact per-input fees)
    let mut picked = Vec::new();
    let mut acc = 0u64;
    for n in notes {
        acc = acc.saturating_add(n.assets);
        picked.push(n);
        if acc >= need { break; }
    }
    if acc < need {
        return Err(anyhow::anyhow!("Not enough notes to cover total payouts + estimated fees"));
    }

    // Convert RPCNote -> NNote
    let mut out = Vec::new();
    for note in picked {
        let version = note.version;
        let origin_page = PageNumber { value: note.origin_page };

        // Option<u64> -> Option<PageNumber>
        let to_page = |x: Option<u64>| x.map(|v| PageNumber { value: v });

        let timelock = Timelock::new(
            note.timelock.map(|t| (
                TimelockRange { min: to_page(t.absolute.min), max: to_page(t.absolute.max) },
                TimelockRange { min: to_page(t.relative.min), max: to_page(t.relative.max) },
            ))
        ).map_err(|e| anyhow::anyhow!(e))?;
        let meta = NNoteHead { version, origin_page, timelock };

        let first = Hash::from_base58(&note.name.first_name).map_err(|e| anyhow::anyhow!(e))?;
        let last  = Hash::from_base58(&note.name.last_name).map_err(|e| anyhow::anyhow!(e))?;
        let name  = NName { p: vec![first, last] };

        let mut pubkeys = ZSet::new();
        if let Some(pk) = note.lock.pubkeys.get(0) {
            pubkeys.put(SchnorrPubkey::from_base58(pk));
        } else {
            return Err(anyhow::anyhow!("Note lock has no pubkeys"));
        }
        let lock   = Lock { m: note.lock.m, pubkeys };

        let source_hash = Hash::from_base58(&note.source_hash).map_err(|e| anyhow::anyhow!(e))?;
        let source = Source { p: source_hash, is_coinbase: note.is_coinbase };

        let assets = Coins { value: note.assets };

        out.push(NNote { meta, name, lock, source, assets });
    }

    Ok(out)
}
