use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use crate::commands;

#[derive(Parser)]
#[command(name = "siger-cli")]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// self test (seed -> child key -> self-check signatures)
    Test(TestArgs),

    /// get device info, capabilities, and lock status
    Info(PortArgs),

    /// Device health (firmware's well-known test)
    Health(PortArgs),

    /// Seed management and optional key file export (replaces old Seed + Keys::Import)
    Seed(SeedArgs),

    /// parse a .draft jam and print inputs + signing plans
    Plan(PlanArgs),

    /// send a .draft jam as FragKind::SignTx and receive a blob back
    SignTx(SignTxArgs),

    /// send a jammed transaction noun and have the device parse + sign it (FragKind::SignDraft)
    SignDraft(SignDraftArgs),

    /// inspect a .draft/.tx file
    Inspect(InspectArgs),

    /// unlock the device with pin
    Unlock(UnlockArgs),

    /// lock the device (clear ram)
    Lock(PortArgs),

    /// factory reset (clears seed + persistent PIN state)
    Reset(PortArgs),
}

#[derive(Args, Clone)]
pub struct PortArgs {
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    /// address version: 0 for legacy, 1 for v1 (default: 1)
    #[arg(long, default_value_t = 1)]
    pub version: u8,
}

#[derive(Args, Clone)]
pub struct TestArgs {
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    /// Optional: 64-byte seed in hex (overrides default)
    #[arg(long)]
    pub seed_hex: Option<String>,
    /// Derivation path: human ("m/44'/0'/0'/0/0") or comma u32s (MSB=hard)
    #[arg(long, default_value = "m")]
    pub path: String,
    /// address version: 0 for legacy, 1 for v1 (default: 1)
    #[arg(long, default_value_t = 1)]
    pub version: u8,
}

#[derive(Args, Clone)]
pub struct PlanArgs {
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    #[arg(long, required = true)]
    pub draft: String,
}

#[derive(Args, Clone)]
pub struct SignTxArgs {
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    #[arg(long, required = true)]
    pub draft: String,
    /// Where to write returned blob (stdout hex if omitted)
    #[arg(long)]
    pub out: Option<String>,
    /// path to signatures json file (apply these signatures instead of signing with device)
    #[arg(long)]
    pub signatures: Option<String>,
}

#[derive(Args, Clone)]
pub struct SignDraftArgs {
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    #[arg(long, required = true)]
    pub draft: String,
    /// Where to write returned blob (defaults to <draft>.tx)
    #[arg(long)]
    pub out: Option<String>,
    /// Seed slot to sign with (default: 0)
    #[arg(long, default_value_t = 0)]
    pub slot: u8,
    /// Recompute tx-id on the host and rewrite the wrapper/id before writing output
    #[arg(long, default_value_t = false)]
    pub host_txid: bool,
}

#[derive(Args, Clone)]
pub struct InspectArgs {
    /// Path to jammed transaction noun
    #[arg(long, required = true)]
    pub file: String,
    /// also dump the raw noun tree
    #[arg(long, default_value_t = false)]
    pub dump_noun: bool,
    /// max recursive depth for noun dump
    #[arg(long, default_value_t = 6)]
    pub max_depth: usize,
    /// max children shown per cell/list at each level
    #[arg(long, default_value_t = 16)]
    pub max_items: usize,
}

#[derive(Args, Clone)]
pub struct SeedArgs {
    /// required for seeding the device (ignored for pure file export from sk)
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,

    // one of these input sources:
    /// 64-byte seed in hex
    #[arg(long, conflicts_with_all=&["mnemonic","sk_b58","sk_hex"])]
    pub seed_hex: Option<String>,

    /// bip39 mnemonic
    #[arg(long, conflicts_with_all=&["seed_hex","sk_b58","sk_hex"])]
    pub seedphrase: Option<String>,

    /// optional passphrase (with --mnemonic)
    #[arg(long, default_value = "")]
    pub passphrase: String,

    /// derivation path to export key files for
    #[arg(long, default_value = "m")]
    pub path: String,

    /// if provided, write <out>.json and <out>.bin (device blob + metadata)
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// pin for hardware wallet
    #[arg(long, required = true)]
    pub pin: Option<String>,

    /// address version: 0 for legacy, 1 for v1 (default: 1)
    #[arg(long, default_value_t = 1)]
    pub version: u8,
}

#[derive(Args, Clone)]
pub struct UnlockArgs {
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    /// pin to unlock the device
    #[arg(long, required = true)]
    pub pin: String,
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Test(args) => {
            commands::test::run(&args.port, args.baud, args.seed_hex.as_deref(), &args.path, args.version)
        }
        Cmd::Info(args) => commands::info::run(&args.port, args.baud, args.version),
        Cmd::Health(args) => commands::health::run(&args.port, args.baud),
        Cmd::Seed(args) => commands::seed::run(args),
        Cmd::Plan(args) => commands::plan::run(&args.port, args.baud, &args.draft),
        Cmd::SignTx(args) => commands::sign_tx::run(
            &args.port,
            args.baud,
            &args.draft,
            args.out.as_deref(),
            args.signatures.as_deref(),
        ),
        Cmd::SignDraft(args) => commands::sign_draft::run(
            &args.port,
            args.baud,
            &args.draft,
            args.out.as_deref(),
            args.slot,
            args.host_txid,
        ),
        Cmd::Inspect(args) => {
            commands::inspect::run(&args.file, args.dump_noun, args.max_depth, args.max_items)
        }
        Cmd::Unlock(args) => commands::unlock::unlock(&args.port, args.baud, &args.pin),
        Cmd::Lock(args) => commands::unlock::lock(&args.port, args.baud),
        Cmd::Reset(args) => commands::reset::run(&args),
    }
}
