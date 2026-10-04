//! `derive` — compute addresses from a seed entirely offline (no device).
//!
//! Shows both the v0 (raw Schnorr pubkey) and v1 (obfuscated PKH) base58 forms
//! so the distinction between them is visible. Useful for verifying what an
//! imported seed will produce and as a building block toward draft composition.

use crate::keys;
use crate::ui;
use crate::util::{format_path, parse_64};

pub fn run(
    seed_hex: Option<&str>,
    seedphrase: Option<&str>,
    passphrase: &str,
    path: &str,
    version: Option<u8>,
    count: u32,
) -> anyhow::Result<()> {
    let seed64 = match (seed_hex, seedphrase) {
        (Some(hx), None) => parse_64(hx)?,
        (None, Some(m)) => keys::bip39_seed_from_mnemonic(m, passphrase),
        (Some(_), Some(_)) => anyhow::bail!("provide only one of --seed-hex or --seedphrase"),
        (None, None) => anyhow::bail!("provide --seed-hex or --seedphrase"),
    };

    ui::header("derive");
    ui::kv("base path", ui::strong(path));
    if count != 1 {
        ui::kv("count", ui::strong(&count.to_string()));
    }

    for i in 0..count.max(1) {
        // For a single address use the path as-is; for a range, append the child
        // index so `--path m/44'/0'/0'/0 --count 3` walks /0../2.
        let child_path = if count <= 1 {
            path.to_string()
        } else {
            format!("{}/{i}", path.trim_end_matches('/'))
        };

        let pretty = derived_path_label(&child_path);
        ui::subhead(&pretty);

        if version != Some(1) {
            let (k0, _) =
                keys::import_from_seed(&seed64, &child_path, 0).map_err(|e| anyhow::anyhow!(e))?;
            ui::kv("v0", ui::accent(&k0.pk_b58));
        }
        if version != Some(0) {
            let (k1, _) =
                keys::import_from_seed(&seed64, &child_path, 1).map_err(|e| anyhow::anyhow!(e))?;
            ui::kv("v1", ui::accent(&k1.pk_b58));
        }
    }

    Ok(())
}

/// Normalize a path for display via the shared `[u32]` formatter, falling back to
/// the raw string if it doesn't parse.
fn derived_path_label(path: &str) -> String {
    match keys::parse_path(path) {
        Ok(components) => format_path(&components),
        Err(_) => path.to_string(),
    }
}
