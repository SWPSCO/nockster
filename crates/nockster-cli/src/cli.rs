use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use crate::commands;

#[derive(Parser)]
#[command(name = "nockster-cli")]
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

    /// report or assert ESP32-S3 secure boot, flash encryption, eFuse, and NVS status
    Security(SecurityArgs),

    /// non-destructive hardware smoke check
    Smoke(SmokeArgs),

    /// get or set persisted touch calibration
    Touch(TouchArgs),

    /// create and verify signed firmware update bundles
    Update(UpdateArgs),

    /// Seed management and optional key file export (replaces old Seed + Keys::Import)
    Seed(SeedArgs),

    /// send a jammed transaction noun and have the device parse + sign it (FragKind::SignDraft)
    SignDraft(SignDraftArgs),

    /// inspect a .draft/.tx file
    Inspect(InspectArgs),

    /// unlock the device with pin
    Unlock(UnlockArgs),

    /// change the PIN by entering the new PIN twice on the device
    Pin(PinArgs),

    /// lock the device (clear ram)
    Lock(PortArgs),

    /// reboot the device without clearing seed or PIN state
    Reboot(PortArgs),

    /// factory reset (clears seed + persistent PIN state)
    Reset(PortArgs),
}

#[derive(Args, Clone)]
pub struct PortArgs {
    /// Serial port path (e.g. `/dev/ttyACM0`) or HID selector (`hid` or `hid:VID:PID`)
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    /// address version: 0 for legacy, 1 for v1 (default: 1)
    #[arg(long, default_value_t = 1)]
    pub version: u8,
}

#[derive(Args, Clone)]
pub struct SecurityArgs {
    /// Serial port path (e.g. `/dev/ttyACM0`) or HID selector (`hid` or `hid:VID:PID`)
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    /// Require firmware built with chip-security status enabled
    #[arg(long)]
    pub expect_chip_security: bool,
    /// Require at least one eFuse key slot with purpose HMAC_UP
    #[arg(long)]
    pub expect_hmac_up: bool,
    /// Require an HMAC_UP key slot to also be read-protected
    #[arg(long)]
    pub expect_hmac_up_read_protected: bool,
    /// Require initialized schema-v2 NVS storage
    #[arg(long)]
    pub expect_nvs_v2: bool,
    /// Require secure boot to be enabled
    #[arg(long)]
    pub expect_secure_boot: bool,
    /// Require flash encryption to be enabled
    #[arg(long)]
    pub expect_flash_encryption: bool,
    /// Require pad, USB, software JTAG, and USB serial/JTAG disable eFuses to be set
    #[arg(long)]
    pub expect_jtag_disabled: bool,
    /// Require download-mode entry paths to be disabled
    #[arg(long)]
    pub expect_download_disabled: bool,
    /// Require direct boot to be disabled
    #[arg(long)]
    pub expect_direct_boot_disabled: bool,
    /// Require USB ROM printing to be disabled
    #[arg(long)]
    pub expect_usb_rom_print_disabled: bool,
    /// Require power-glitch protection to be enabled
    #[arg(long)]
    pub expect_power_glitch_protection: bool,
    /// Require the current production lockdown checklist except power-glitch protection
    #[arg(long)]
    pub expect_production_lockdown: bool,
    /// Rewrite existing NVS storage to schema v2 using the current PIN and HMAC_UP pepper
    #[arg(long, requires = "current_pin")]
    pub migrate_nvs_v2: bool,
    /// Current device PIN for explicit NVS schema migration
    #[arg(long)]
    pub current_pin: Option<String>,
}

#[derive(Args, Clone)]
pub struct TestArgs {
    /// Serial port path (e.g. `/dev/ttyACM0`) or HID selector (`hid` or `hid:VID:PID`)
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
pub struct SignDraftArgs {
    /// Serial port path (e.g. `/dev/ttyACM0`) or HID selector (`hid` or `hid:VID:PID`)
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
    /// required for seeding the device (ignored for pure file export from sk);
    /// can be a serial port (e.g. `/dev/ttyACM0`) or HID selector (`hid` or `hid:VID:PID`)
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,

    // one of these input sources:
    /// 64-byte seed in hex
    #[arg(long, conflicts_with_all=&["seedphrase","list","select","delete"])]
    pub seed_hex: Option<String>,

    /// bip39 mnemonic
    #[arg(long, conflicts_with_all=&["seed_hex","list","select","delete"])]
    pub seedphrase: Option<String>,

    /// list seed slots and root PKHs
    #[arg(long, conflicts_with_all=&["seed_hex","seedphrase","select","delete"])]
    pub list: bool,

    /// select the active seed slot
    #[arg(long, conflicts_with_all=&["seed_hex","seedphrase","list","delete"])]
    pub select: Option<u8>,

    /// delete a seed slot after on-device confirmation
    #[arg(long, conflicts_with_all=&["seed_hex","seedphrase","list","select"])]
    pub delete: Option<u8>,

    /// label the newly added seed, or label the slot passed with --select
    #[arg(long)]
    pub label: Option<String>,

    /// acknowledge a destructive --delete request before the device asks on-screen
    #[arg(long, requires = "delete", default_value_t = false)]
    pub yes: bool,

    /// optional passphrase (with --seedphrase)
    #[arg(long, default_value = "")]
    pub passphrase: String,

    /// derivation path to export key files for
    #[arg(long, default_value = "m")]
    pub path: String,

    /// if provided, write <out>.json and <out>.bin (device blob + metadata)
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// PIN for adding or initializing a seed
    #[arg(long)]
    pub pin: Option<String>,

    /// address version: 0 for legacy, 1 for v1 (default: 1)
    #[arg(long, default_value_t = 1)]
    pub version: u8,
}

#[derive(Args, Clone)]
pub struct TouchArgs {
    /// Serial port path (e.g. `/dev/ttyACM0`) or HID selector (`hid` or `hid:VID:PID`)
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,

    /// Raw touch X minimum
    #[arg(long)]
    pub x_min: Option<u16>,
    /// Raw touch X maximum
    #[arg(long)]
    pub x_max: Option<u16>,
    /// Raw touch Y minimum
    #[arg(long)]
    pub y_min: Option<u16>,
    /// Raw touch Y maximum
    #[arg(long)]
    pub y_max: Option<u16>,
    /// Override X mirroring, for example `--mirror-x true`
    #[arg(long)]
    pub mirror_x: Option<bool>,
    /// Override Y mirroring, for example `--mirror-y false`
    #[arg(long)]
    pub mirror_y: Option<bool>,
    /// Start on-device touch calibration and wait for it to save
    #[arg(
        long,
        conflicts_with_all = [
            "x_min",
            "x_max",
            "y_min",
            "y_max",
            "mirror_x",
            "mirror_y",
            "diagnostics",
            "exit_diagnostics"
        ]
    )]
    pub calibrate: bool,
    /// Show live touch/display diagnostics on the device
    #[arg(long, conflicts_with = "exit_diagnostics")]
    pub diagnostics: bool,
    /// Exit live touch/display diagnostics on the device
    #[arg(long)]
    pub exit_diagnostics: bool,
}

#[derive(Args, Clone)]
pub struct UpdateArgs {
    #[command(subcommand)]
    pub cmd: UpdateCmd,
}

#[derive(Subcommand, Clone)]
pub enum UpdateCmd {
    /// generate a new local release signing key file
    Keygen(UpdateKeygenArgs),
    /// sign a firmware image and write a portable update bundle
    Sign(UpdateSignArgs),
    /// write a browser latest-release index for one-click site updates
    Index(UpdateIndexArgs),
    /// verify a bundle the same way firmware will before accepting it
    Verify(UpdateVerifyArgs),
    /// ask a connected device which release public key hash it trusts
    Trust(UpdateTrustArgs),
    /// ask a connected device to verify a bundle manifest signature
    DeviceVerify(UpdateDeviceVerifyArgs),
    /// read the current on-device update stream status
    Status(UpdateStatusArgs),
    /// stream a bundle image to the device for on-device digest verification without flashing
    DeviceStreamVerify(UpdateDeviceStreamVerifyArgs),
    /// install a signed firmware bundle into the inactive OTA slot and activate it for next boot
    DeviceInstall(UpdateDeviceInstallArgs),
    /// derive the compressed release public key and trust anchor hash
    Pubkey(UpdatePubkeyArgs),
}

#[derive(Args, Clone)]
pub struct UpdateKeygenArgs {
    /// Output key file path. Refuses to overwrite an existing file.
    #[arg(long)]
    pub out: PathBuf,
    /// Write raw 32-byte key material instead of hex text
    #[arg(long)]
    pub raw: bool,
}

#[derive(Args, Clone)]
pub struct UpdateSignArgs {
    /// Firmware binary to package
    #[arg(long)]
    pub firmware: PathBuf,
    /// Output bundle JSON path
    #[arg(long)]
    pub out: PathBuf,
    /// File containing the 32-byte release signing key as raw bytes or hex text
    #[arg(long)]
    pub signing_key_file: PathBuf,
    /// Monotonic release version for rollback policy
    #[arg(long)]
    pub release_version: u32,
    /// Hardware target this image is allowed to run on
    #[arg(long, default_value = "esp32s3-touch-lcd-1.47")]
    pub hardware_target: String,
    /// Firmware build profile
    #[arg(long, default_value = "production")]
    pub build_profile: String,
    /// Protocol version supported by this image
    #[arg(long, default_value_t = nockster_core::PROTO_V1)]
    pub protocol_v: u8,
    /// Firmware git commit included in the release manifest. Defaults to `git rev-parse HEAD`.
    #[arg(long)]
    pub git_commit: Option<String>,
    /// tx-types revision included in the release manifest. Defaults to the workspace Cargo.toml pin.
    #[arg(long)]
    pub tx_types_rev: Option<String>,
}

#[derive(Args, Clone)]
pub struct UpdateIndexArgs {
    /// Bundle JSON produced by `nockster-cli update sign`
    #[arg(long)]
    pub bundle: PathBuf,
    /// Firmware binary referenced by the bundle
    #[arg(long)]
    pub firmware: PathBuf,
    /// Output latest-release index JSON path
    #[arg(long)]
    pub out: PathBuf,
    /// URL written into the index for the bundle. Defaults to the bundle file name.
    #[arg(long)]
    pub bundle_url: Option<String>,
    /// URL written into the index for the firmware. Defaults to the firmware file name.
    #[arg(long)]
    pub firmware_url: Option<String>,
}

#[derive(Args, Clone)]
pub struct UpdateVerifyArgs {
    /// Bundle JSON produced by `nockster-cli update sign`
    #[arg(long)]
    pub bundle: PathBuf,
    /// Firmware binary referenced by the bundle
    #[arg(long)]
    pub firmware: PathBuf,
    /// Pinned SHA-256 hash of the trusted compressed SEC1 release public key
    #[arg(long)]
    pub trusted_pubkey_sha256: String,
}

#[derive(Args, Clone)]
pub struct UpdateTrustArgs {
    /// Serial port path (e.g. `/dev/ttyACM0`) or HID selector (`hid` or `hid:VID:PID`)
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
}

#[derive(Args, Clone)]
pub struct UpdateDeviceVerifyArgs {
    /// Serial port path (e.g. `/dev/ttyACM0`) or HID selector (`hid` or `hid:VID:PID`)
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    /// Bundle JSON produced by `nockster-cli update sign`
    #[arg(long)]
    pub bundle: PathBuf,
}

#[derive(Args, Clone)]
pub struct UpdateStatusArgs {
    /// Serial port path (e.g. `/dev/ttyACM0`) or HID selector (`hid` or `hid:VID:PID`)
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    /// Require no update stream to be active
    #[arg(long)]
    pub expect_idle: bool,
    /// Require the partition table, otadata, ota_0, and ota_1 to be present
    #[arg(long)]
    pub expect_ota_ready: bool,
    /// Require a specific selected slot: factory, ota0, ota1, none, or unknown
    #[arg(long)]
    pub expect_current_slot: Option<String>,
    /// Require a specific next update slot: ota0, ota1, factory, none, or unknown
    #[arg(long)]
    pub expect_next_slot: Option<String>,
    /// Require a specific OTA image state: new, pending-verify, valid, invalid, aborted, undefined, unavailable, or unknown
    #[arg(long)]
    pub expect_ota_state: Option<String>,
    /// Require the boot status endpoint to be supported
    #[arg(long)]
    pub require_boot_status: bool,
}

#[derive(Args, Clone)]
pub struct UpdateDeviceStreamVerifyArgs {
    /// Serial port path (e.g. `/dev/ttyACM0`) or HID selector (`hid` or `hid:VID:PID`)
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    /// Bundle JSON produced by `nockster-cli update sign`
    #[arg(long)]
    pub bundle: PathBuf,
    /// Firmware binary referenced by the bundle
    #[arg(long)]
    pub firmware: PathBuf,
    /// Bytes per update chunk sent to the device
    #[arg(long, default_value_t = nockster_core::update::MAX_UPDATE_CHUNK_LEN)]
    pub chunk_size: usize,
}

#[derive(Args, Clone)]
pub struct UpdateDeviceInstallArgs {
    /// Serial port path (e.g. `/dev/ttyACM0`) or HID selector (`hid` or `hid:VID:PID`)
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    /// Bundle JSON produced by `nockster-cli update sign`
    #[arg(long)]
    pub bundle: PathBuf,
    /// Firmware binary referenced by the bundle
    #[arg(long)]
    pub firmware: PathBuf,
    /// Bytes per update chunk sent to the device
    #[arg(long, default_value_t = nockster_core::update::MAX_UPDATE_CHUNK_LEN)]
    pub chunk_size: usize,
    /// Reboot after post-install OTA activation validation succeeds
    #[arg(long)]
    pub reboot: bool,
}

#[derive(Args, Clone)]
pub struct UpdatePubkeyArgs {
    /// File containing the 32-byte release signing key as raw bytes or hex text
    #[arg(long)]
    pub signing_key_file: PathBuf,
}

#[derive(Args, Clone)]
pub struct SmokeArgs {
    /// Serial port path (e.g. `/dev/ttyACM0`) or HID selector (`hid` or `hid:VID:PID`)
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    /// address version: 0 for legacy, 1 for v1 (default: 1)
    #[arg(long, default_value_t = 1)]
    pub version: u8,
    /// Opt-in end-to-end SignDraft smoke check. Requires on-device approval.
    #[arg(long)]
    pub sign_draft: Option<String>,
    /// Where to write the optional --sign-draft output
    #[arg(long, requires = "sign_draft")]
    pub out: Option<String>,
    /// Seed slot for the optional --sign-draft check
    #[arg(long, default_value_t = 0)]
    pub slot: u8,
    /// Recompute tx-id on the host for the optional --sign-draft output
    #[arg(long, default_value_t = false)]
    pub host_txid: bool,
}

#[derive(Args, Clone)]
pub struct UnlockArgs {
    /// Serial port path (e.g. `/dev/ttyACM0`) or HID selector (`hid` or `hid:VID:PID`)
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    /// pin to unlock the device
    #[arg(long, required = true)]
    pub pin: String,
}

#[derive(Args, Clone)]
pub struct PinArgs {
    /// Serial port path (e.g. `/dev/ttyACM0`) or HID selector (`hid` or `hid:VID:PID`)
    #[arg(long, required = true)]
    pub port: String,
    #[arg(long, default_value_t = 115200)]
    pub baud: u32,
    /// current PIN; the new PIN is entered twice on the device
    #[arg(long, required = true)]
    pub current_pin: String,
}

pub fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Test(args) => commands::test::run(
            &args.port,
            args.baud,
            args.seed_hex.as_deref(),
            &args.path,
            args.version,
        ),
        Cmd::Info(args) => commands::info::run(&args.port, args.baud, args.version),
        Cmd::Health(args) => commands::health::run(&args.port, args.baud),
        Cmd::Security(args) => commands::security::run(&args),
        Cmd::Smoke(args) => commands::smoke::run(&args),
        Cmd::Touch(args) => commands::touch::run(&args),
        Cmd::Update(args) => commands::update::run(&args),
        Cmd::Seed(args) => commands::seed::run(args),
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
        Cmd::Pin(args) => commands::pin::run(&args.port, args.baud, &args.current_pin),
        Cmd::Lock(args) => commands::unlock::lock(&args.port, args.baud),
        Cmd::Reboot(args) => commands::reboot::run(&args),
        Cmd::Reset(args) => commands::reset::run(&args),
    }
}
