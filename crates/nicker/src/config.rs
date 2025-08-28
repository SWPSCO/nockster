use clap::{Parser, Subcommand, Args as ClapArgs};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub rpc_url: String,
    #[serde(default = "default_origin")]
    pub origin_header: String,

    /// Used when --source is not provided
    pub default_source: Option<String>,

    /// Used when --fee-per-input is not provided
    #[serde(default = "default_fee")]
    pub default_fee_per_input: u64,

    /// Used when --seed is not provided
    pub default_sign_seed: Option<String>,
}

fn default_origin() -> String { "https://nockblocks.com".into() }
fn default_fee() -> u64 { 10 }

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Args {
    /// Path to TOML with rpc_url, default_source (opt), default_fee_per_input (opt)
    #[arg(long, value_name = "FILE", default_value = "config.toml")]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Create(CreateArgs),
    Sign(SignArgs),
}

#[derive(ClapArgs, Debug)]
pub struct SignArgs {
    /// Path to the .draft file to sign
    #[arg(short, long)]
    pub draft: String,
    
    /// Seed phrase for signing (overrides config)
    #[arg(short, long)]
    pub seed: Option<String>,
    
    /// Output filename (defaults to input with .tx extension)
    #[arg(short, long)]
    pub output: Option<String>,
}

#[derive(ClapArgs, Debug)]
pub struct CreateArgs {
    /// Override source (otherwise uses config.default_source)
    #[arg(long)]
    pub source: Option<String>,

    /// One or more payouts: ADDRESS:AMOUNT (required)
    #[arg(long = "payout", value_parser = parse_payout, required = true)]
    pub payouts: Vec<PayoutArg>,

    /// Override fee-per-input (otherwise uses config.default_fee_per_input)
    #[arg(long)]
    pub fee_per_input: Option<u64>,

    /// Optional filename prefix override
    #[arg(long)]
    pub filename: Option<String>,

    /// getNotes limit
    #[arg(long, default_value_t = 50)]
    pub rpc_limit: u64,

    /// Seed phrase for signing transactions
    #[arg(long)]
    pub sign_seed: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PayoutArg {
    pub address: String,
    pub amount: u64,
}

fn parse_payout(s: &str) -> Result<PayoutArg, String> {
    let Some((a, b)) = s.split_once(':') else {
        return Err("payout must be ADDRESS:AMOUNT".into());
    };
    let amount: u64 = b.parse().map_err(|_| "invalid AMOUNT u64".to_string())?;
    Ok(PayoutArg { address: a.to_string(), amount })
}