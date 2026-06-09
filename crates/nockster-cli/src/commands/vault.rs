//! `vault` — manage the on-device preimage vault (%hax lock secrets), and
//! `export-master-pubkey` — produce a nockchain-wallet watch-only keyfile.
//!
//! Store, reveal, delete, and the pubkey export all wait for on-screen
//! confirmation; the default call deadline (120s) covers it.

use std::path::PathBuf;

use crate::cli::{ExportMasterPubkeyArgs, VaultAction, VaultArgs};
use crate::serial::{open, send_call};
use crate::ui;
use nockster_core::draft_sign::tip5_digest_b58;
use nockster_core::wallet_keyfile;
use nockster_core::{
    describe_error, Request, Response, VaultEntryInfo, ERR_DEVICE_LOCKED, MAX_SEED_LABEL_LEN,
    MAX_VAULT_ENTRIES, MAX_VAULT_PREIMAGE_LEN,
};

pub fn run(args: VaultArgs) -> anyhow::Result<()> {
    let mut sp = open(&args.port, args.baud)?;
    ui::header("vault");

    match args.action {
        VaultAction::List => {
            let entries = expect_entries(send_call(&mut *sp, 0x60, Request::VaultList)?)?;
            print_entries(&entries);
            Ok(())
        }
        VaultAction::Store {
            label,
            hex,
            file,
            jam,
        } => {
            if label.len() > MAX_SEED_LABEL_LEN {
                anyhow::bail!("label too long (max {MAX_SEED_LABEL_LEN} chars)");
            }
            let raw = match (hex, file) {
                (Some(hex), None) => decode_hex(&hex)?,
                (None, Some(path)) => std::fs::read(&path)?,
                _ => anyhow::bail!("provide the secret with --hex or --file"),
            };
            if raw.is_empty() {
                anyhow::bail!("secret is empty");
            }
            let preimage = if jam {
                raw
            } else {
                wallet_keyfile::jam_atom(&raw)
            };
            if preimage.len() > MAX_VAULT_PREIMAGE_LEN {
                anyhow::bail!(
                    "preimage too large ({} > {MAX_VAULT_PREIMAGE_LEN} bytes jammed)",
                    preimage.len()
                );
            }
            // Preview the commitment so the user can compare it against the
            // device screen before approving.
            let commitment = nockster_core::draft_sign::noun_commitment_v1(&preimage)
                .map_err(|_| anyhow::anyhow!("input is not a valid jammed noun"))?;
            ui::kv("commitment", &tip5_digest_b58(commitment));
            ui::note("confirm on device (the same commitment must be shown)");

            let mut request = Request::VaultStore {
                label: Default::default(),
                preimage,
            };
            if let Request::VaultStore { label: slot, .. } = &mut request {
                slot.push_str(&label)
                    .map_err(|_| anyhow::anyhow!("label too long"))?;
            }
            let entries = expect_entries(send_call(&mut *sp, 0x61, request)?)?;
            ui::note("stored");
            print_entries(&entries);
            Ok(())
        }
        VaultAction::Reveal { slot, out } => {
            ui::note("confirm reveal on device");
            match send_call(&mut *sp, 0x62, Request::VaultReveal { slot })? {
                Response::OkVaultPreimage {
                    commitment,
                    preimage,
                } => {
                    ui::kv("commitment", &tip5_digest_b58(commitment));
                    match out {
                        Some(path) => {
                            std::fs::write(&path, &preimage)?;
                            ui::kv("written", &path.display().to_string());
                        }
                        None => {
                            ui::kv("preimage (jam)", &hex_string(&preimage));
                            if let Ok(atom) = wallet_keyfile::cue_atom(&preimage) {
                                ui::kv("atom bytes", &hex_string(&atom));
                            }
                        }
                    }
                    Ok(())
                }
                other => bail_vault("reveal", other),
            }
        }
        VaultAction::Delete { slot } => {
            ui::note("confirm delete on device");
            let entries =
                expect_entries(send_call(&mut *sp, 0x63, Request::VaultDelete { slot })?)?;
            ui::note("deleted");
            print_entries(&entries);
            Ok(())
        }
    }
}

pub fn run_export_master_pubkey(args: ExportMasterPubkeyArgs) -> anyhow::Result<()> {
    let mut sp = open(&args.port, args.baud)?;
    ui::header("export master pubkey");
    ui::note("confirm watch-only export on device");

    match send_call(
        &mut *sp,
        0x64,
        Request::GetMasterPubkey { slot: args.slot },
    )? {
        Response::OkMasterPubkey { x, y, chain_code } => {
            let bytes = wallet_keyfile::build_master_pubkey_export(x, y, &chain_code);
            std::fs::write(&args.out, &bytes)?;
            ui::kv("slot", &args.slot.to_string());
            ui::kv("written", &args.out.display().to_string());
            ui::note("import with: nockchain-wallet import-master-pubkey --file <path>");
            Ok(())
        }
        other => bail_vault("export-master-pubkey", other),
    }
}

fn expect_entries(resp: Response) -> anyhow::Result<Vec<VaultEntryInfo>> {
    match resp {
        Response::OkVaultEntries(entries) => Ok(entries),
        other => bail_vault("vault", other),
    }
}

fn bail_vault<T>(what: &str, resp: Response) -> anyhow::Result<T> {
    match resp {
        Response::Err {
            code: ERR_DEVICE_LOCKED,
        } => anyhow::bail!("device locked; unlock first"),
        Response::Err { code } => {
            anyhow::bail!("{what} failed: {} (code {code})", describe_error(code))
        }
        other => anyhow::bail!("unexpected {what} response: {other:?}"),
    }
}

fn print_entries(entries: &[VaultEntryInfo]) {
    if entries.is_empty() {
        ui::note("vault is empty");
        return;
    }
    for entry in entries {
        let label = if entry.label.is_empty() {
            "(unnamed)"
        } else {
            entry.label.as_str()
        };
        ui::item(format!(
            "slot {}  {}  {}  ({} bytes)",
            entry.slot,
            ui::strong(label),
            ui::accent(&tip5_digest_b58(entry.commitment)),
            entry.preimage_len,
        ));
    }
    ui::note(&format!("{}/{} slots used", entries.len(), MAX_VAULT_ENTRIES));
}

fn decode_hex(input: &str) -> anyhow::Result<Vec<u8>> {
    let cleaned = input.trim().trim_start_matches("0x");
    if cleaned.len() % 2 != 0 {
        anyhow::bail!("hex input must have an even number of digits");
    }
    (0..cleaned.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&cleaned[i..i + 2], 16)
                .map_err(|_| anyhow::anyhow!("invalid hex at offset {i}"))
        })
        .collect()
}

fn hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Resolve a `keys.export` file to the seed phrase it carries (used by
/// `seed --keyfile`).
pub fn seedphrase_from_keyfile(path: &PathBuf) -> anyhow::Result<String> {
    let bytes = std::fs::read(path)?;
    let summary = wallet_keyfile::parse_keyfile(&bytes)
        .map_err(|e| anyhow::anyhow!("keyfile parse failed: {e}"))?;
    match summary.seedphrases.first() {
        Some(phrase) => {
            ui::note(&format!(
                "keyfile: {} entries, {} private / {} public coils",
                summary.entry_count, summary.coil_prv_count, summary.coil_pub_count
            ));
            Ok(phrase.clone())
        }
        None => anyhow::bail!(
            "keyfile contains no seed phrase (only derived keys: {} private, {} public); \
             nockster slots store BIP39 seeds, so import the original phrase instead",
            summary.coil_prv_count,
            summary.coil_pub_count
        ),
    }
}
