mod config;
mod tracer;
mod words;
mod rpc;
mod signer;

use clap::Parser;
use tracing::{info};
use anyhow::Result;
use std::fs;
use crate::rpc::{retrieve_notes_for_address};
use config::{Args, Config, Commands, CreateArgs, SignArgs};
use tx_types::transaction_types::*;
use tx_types::collections::{ZMap, ZSet};
use nockapp::noun::slab::NounSlab;
use noun_serde::{NounEncode, NounDecode};

#[derive(Clone, Debug)]
struct Payout {
    payout_address: String,
    amount: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracer::init();
    let args = Args::parse();
    let config_str = fs::read_to_string(&args.config)?;
    let config: Config = toml::from_str(&config_str)?;

    match args.command {
        Commands::Create(cargs) => {
            create_transaction_from_args(&config, cargs).await?;
        }
        Commands::Sign(sargs) => {
            sign_draft_transaction(&config, sargs).await?;
        }
    }

    Ok(())
}

async fn create_transaction_from_args(cfg: &Config, cargs: CreateArgs) -> Result<()> {
  let source = match (cargs.source.clone(), cfg.default_source.clone()) {
      (Some(s), _) => s,
      (None, Some(s)) => s,
      (None, None) => {
          return Err(anyhow::anyhow!(
              "No source address provided. Pass --source or set default_source in config.toml"
          ))
      }
  };

  let fee_per_input = cargs.fee_per_input.unwrap_or(cfg.default_fee_per_input);

  // Build payouts from CLI
  let payouts: Vec<Payout> = cargs
      .payouts
      .iter()
      .cloned()
      .map(|p| Payout {
          payout_address: p.address,
          amount: p.amount,
      })
      .collect();

  let total_amount: u64 = payouts.iter().map(|p| p.amount).sum();

  let input_notes = retrieve_notes_for_address(
      cfg,
      &source,
      total_amount,
      fee_per_input,
      cargs.rpc_limit,
  )
  .await?;

  let (transaction, tx_id) =
      build_wallet_transaction_with_fee(input_notes, payouts, fee_per_input).await?;

  // jam & write .draft
  let mut slab: NounSlab = NounSlab::new();
  let encoded = transaction.to_noun(&mut slab);
  slab.copy_into(encoded);
  let jammed_transaction = slab.jam();

  // compute tx_name
  let tx_name: String = cargs
      .filename
      .clone()
      .unwrap_or_else(|| words::generate_tx_name(tx_id.clone()));

  let draft_path = format!("{}.draft", tx_name);
  {
      let mut f = std::fs::File::create(&draft_path)?;
      use std::io::Write;
      f.write_all(jammed_transaction.as_ref())?;
  }
  info!("Transaction saved to file: {}", draft_path);

  // optionally sign
  if let Some(seed) = cfg.default_sign_seed.as_ref() {
      let signed_jam = signer::sign_transaction(
          transaction.clone(),
          signer::SecretSource::Seed(seed.as_bytes()),
      )
      .map_err(|e| anyhow::anyhow!(e))?;

      let signed_path = format!("{}.signed", &tx_name);
      let mut f = std::fs::File::create(&signed_path)?;
      use std::io::Write;
      f.write_all(signed_jam.as_ref())?;
      info!("Signed transaction saved to: {}", signed_path);
  }

  Ok(())
}

async fn sign_draft_transaction(cfg: &Config, sargs: SignArgs) -> Result<()> {
    // read the draft file
    let draft_bytes = std::fs::read(&sargs.draft)?;
    
    // cue transaction
    let mut slab: NounSlab = NounSlab::new();
    let noun = slab.cue_into(draft_bytes.into())?;
    let transaction = Transaction::from_noun(&mut slab, &noun)
        .map_err(|e| anyhow::anyhow!("Failed to decode transaction: {:?}", e))?;
    
    // get seed
    let seed = match (sargs.seed, cfg.default_sign_seed.clone()) {
        (Some(s), _) => s,
        (None, Some(s)) => s,
        (None, None) => {
            return Err(anyhow::anyhow!(
                "No seed provided. Pass --seed or set default_sign_seed in config.toml"
            ))
        }
    };
    
    // sign the transaction
    let signed_jam = signer::sign_transaction(
        transaction,
        signer::SecretSource::Seed(seed.as_bytes()),
    )
    .map_err(|e| anyhow::anyhow!("Signing failed: {}", e))?;
    
    // determine output filename
    let output_path = sargs.output.unwrap_or_else(|| {
        sargs.draft.replace(".draft", ".tx")
    });
    
    // write signed transaction
    std::fs::write(&output_path, signed_jam.as_ref())?;
    info!("Signed transaction saved to: {}", output_path);
    
    Ok(())
}

async fn build_wallet_transaction_with_fee(input_notes: Vec<NNote>, payouts: Vec<Payout>, fee_per_input: u64)
    -> Result<(Transaction, String)>
{
    let (inputs, name) = build_inputs_with_fee(input_notes, payouts, fee_per_input).await?;
    let transaction = Transaction { name: name.clone(), p: inputs };
    Ok((transaction, name))
}

async fn build_inputs_with_fee(input_notes: Vec<NNote>, payouts: Vec<Payout>, fee_per_input: u64)
    -> Result<(Inputs, String)>
{
    let mut inputs = ZMap::new();
    let mut remaining = payouts.clone();

    for note in input_notes {
        let nname = note.name.clone();
        let (spend, rest) = build_spend_from_payouts(note.clone(), remaining, fee_per_input).await?;
        inputs.put(nname, Input { note, spend });
        remaining = rest;

        if remaining.is_empty() {
            break;
        }
    }

    if !remaining.is_empty() {
        return Err(anyhow::anyhow!("Selected inputs cannot cover all payouts + fees"));
    }

    let name = tx_types::tx_to_noun::generate_tx_id(inputs.clone()).to_base58();
    Ok((Inputs { p: inputs }, name))
}

async fn build_spend_from_payouts(note: NNote, payouts: Vec<Payout>, fee_per_input: u64)
    -> Result<(Spend, Vec<Payout>)>
{
    // Max assets available in this note after fee
    if note.assets.value < fee_per_input {
        return Err(anyhow::anyhow!("Note cannot cover per-input fee"));
    }
    let max_assets = note.assets.value - fee_per_input;

    // largest payouts first
    let mut entries = payouts.clone();
    entries.sort_by(|a, b| b.amount.cmp(&a.amount));

    let mut seed_set = ZSet::new();
    let mut spent_amount = 0u64;
    let mut paid = Vec::new();
    let mut remaining = Vec::new();
    let parent_hash = note.to_hash();

    for e in entries {
        if e.amount > 0 && (spent_amount + e.amount) <= max_assets {
            spent_amount += e.amount;
            paid.push(e.clone());

            // Recipient lock
            let mut pubkeys = ZSet::new();
            let pk = SchnorrPubkey::from_base58(&e.payout_address);
            pubkeys.put(pk);

            seed_set.put(Seed {
                output_source: None,
                recipient: Lock { m: 1, pubkeys },
                timelock_intent: None,
                gift: Coins { value: e.amount },
                parent_hash: parent_hash.clone(),
            });
        } else {
            remaining.push(e);
        }
    }

    // Change
    let change_amount = max_assets - spent_amount;
    if change_amount > 0 {
        seed_set.put(Seed {
            output_source: None,
            recipient: Lock { m: note.lock.m, pubkeys: note.lock.pubkeys.clone() },
            timelock_intent: None,
            gift: Coins { value: change_amount },
            parent_hash: parent_hash.clone(),
        });
    }

    let spend = Spend {
        signature: None,
        seeds: Seeds { set: seed_set },
        fee: Coins { value: fee_per_input },
    };

    Ok((spend, remaining))
}