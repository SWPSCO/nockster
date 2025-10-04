use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use crate::commands;

/// Unified top-level CLI with flat subcommands
#[derive(Parser)]
#[command(name = "siger-cli")]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// End-to-end self test (seed -> child key -> self-check signatures)
    Test(TestArgs),

    /// Get device info, capabilities, and lock status
    Info(PortArgs),

    /// Device health (firmware's well-known test)
    Health(PortArgs),

    /// Seed management and optional key file export (replaces old Seed + Keys::Import)
    Seed(SeedArgs),

    /// Parse a .draft (jam) and print inputs + signing plans
    Plan(PlanArgs),

    /// Send a .draft (jam) as FragKind::SignTx and receive a blob back
    SignTx(SignTxArgs),

    /// Inspect a .draft/.tx file: typed summary + optional raw noun dump
    Inspect(InspectArgs),

    /// Unlock the device with PIN
    Unlock(UnlockArgs),

    /// Lock the device (clear RAM seed)
    Lock(PortArgs),
}

#[derive(Args, Clone)]
pub struct PortArgs {
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
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
}

#[derive(Args, Clone)]
pub struct InspectArgs {
    /// Path to jammed noun (wallet tx, raw-tx, tx, or [name inputs])
    #[arg(long, required = true)]
    pub draft: String,
    /// Also dump the raw noun tree
    #[arg(long, default_value_t = false)]
    pub dump_noun: bool,
    /// Max recursive depth for noun dump
    #[arg(long, default_value_t = 6)]
    pub max_depth: usize,
    /// Max children shown per cell/list at each level
    #[arg(long, default_value_t = 16)]
    pub max_items: usize,
}

/// Unified seed/keys args (mutually exclusive inputs)
#[derive(Args, Clone)]
pub struct SeedArgs {
    /// Required for seeding the device (ignored for pure file export from sk)
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,

    // One of these input sources:
    /// 64-byte seed in hex (seeds device; can also write files if --out is set)
    #[arg(long, conflicts_with_all=&["mnemonic","sk_b58","sk_hex"])]
    pub seed_hex: Option<String>,

    /// BIP-39 mnemonic (seeds device)
    #[arg(long, conflicts_with_all=&["seed_hex","sk_b58","sk_hex"])]
    pub mnemonic: Option<String>,

    /// Optional BIP-39 passphrase (with --mnemonic)
    #[arg(long, default_value = "")]
    pub passphrase: String,

    /// Derivation path to export key files for (with --seed-hex or --mnemonic)
    /// Also used with --sk-b58/--sk-hex to compute pubkey and blob
    #[arg(long, default_value = "m")]
    pub path: String,

    /// Base58-encoded 32-byte private key (file export only; does NOT seed device)
    #[arg(long, conflicts_with_all=&["seed_hex","mnemonic","sk_hex"])]
    pub sk_b58: Option<String>,

    /// Hex-encoded 32-byte private key (file export only; does NOT seed device)
    #[arg(long, conflicts_with_all=&["seed_hex","mnemonic","sk_b58"])]
    pub sk_hex: Option<String>,

    /// If provided, write <out>.json and <out>.bin (device blob + metadata)
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// PIN for hardware wallet (if provided, initializes device with encrypted storage)
    #[arg(long)]
    pub pin: Option<String>,
}

#[derive(Args, Clone)]
pub struct UnlockArgs {
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    /// PIN to unlock the device
    #[arg(long, required = true)]
    pub pin: String,
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Test(args) => {
            commands::test::run(&args.port, args.baud, args.seed_hex.as_deref(), &args.path)
        }
        Cmd::Info(args) => commands::info::run(&args.port, args.baud),
        Cmd::Health(args) => commands::health::run(&args.port, args.baud),
        Cmd::Seed(args) => commands::seed::run(args),
        Cmd::Plan(args) => commands::plan::run(&args.port, args.baud, &args.draft),
        Cmd::SignTx(args) => {
            commands::sign_tx::run(&args.port, args.baud, &args.draft, args.out.as_deref())
        }
        Cmd::Inspect(args) => {
            commands::inspect::run(&args.draft, args.dump_noun, args.max_depth, args.max_items)
        }
        Cmd::Unlock(args) => commands::unlock::unlock(&args.port, args.baud, &args.pin),
        Cmd::Lock(args) => commands::unlock::lock(&args.port, args.baud),
    }
}
