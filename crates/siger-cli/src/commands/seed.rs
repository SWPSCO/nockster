use crate::cli::SeedArgs;
use crate::keys;
use crate::serial::{open, send_blob, send_call};
use crate::util::parse_64;
use siger_core::{FragKind, Request, Response};

/// Behavior:
/// - If --seed-hex is provided: seeds device with the 64B value, and if --out is set, writes key files
///   using (--path) for derivation.
/// - If --mnemonic is provided: derives 64B seed (PBKDF2), seeds device, and if --out is set, writes
///   key files for (--path).
/// - If --sk-b58 or --sk-hex is provided (and no seed/mnemonic): DOES NOT seed device; only writes files
///   if --out is set. (Feature-parity with prior `keys import`.)
pub fn run(args: SeedArgs) -> anyhow::Result<()> {
    let mut did_seed_device = false;
    let mut seeded_len = 0usize;

    // Open serial eagerly, but only used when we actually seed the device.
    let mut sp = open(&args.port, args.baud)?;

    // Case 1: seed via mnemonic
    if let Some(m) = args.mnemonic.as_deref() {
        let seed64 = keys::bip39_seed_from_mnemonic(m, &args.passphrase);
        send_blob(&mut *sp, 0x42, FragKind::SetSeed, &seed64)?;
        did_seed_device = true;
        seeded_len = seed64.len();

        // Optional file outputs
        if let Some(out) = args.out.as_ref() {
            let (key, blob) =
                keys::import_from_seed(&seed64, &args.path).map_err(|e| anyhow::anyhow!(e))?;
            let (json_path, bin_path) =
                keys::write_key_files(out, &key, &blob).map_err(|e| anyhow::anyhow!(e))?;
            println!("✔ wrote key JSON to {}", json_path.display());
            println!("✔ wrote device blob to {}", bin_path.display());
            println!("pubkey (b58): {}", key.pk_b58);
            if let Some(p) = &key.path {
                println!("path: {}", p);
            }
        }

    // Case 2: seed via seed_hex
    } else if let Some(hx) = args.seed_hex.as_deref() {
        let seed64 = parse_64(hx)?;
        send_blob(&mut *sp, 0x42, FragKind::SetSeed, &seed64)?;
        did_seed_device = true;
        seeded_len = seed64.len();

        if let Some(out) = args.out.as_ref() {
            let (key, blob) =
                keys::import_from_seed(&seed64, &args.path).map_err(|e| anyhow::anyhow!(e))?;
            let (json_path, bin_path) =
                keys::write_key_files(out, &key, &blob).map_err(|e| anyhow::anyhow!(e))?;
            println!("✔ wrote key JSON to {}", json_path.display());
            println!("✔ wrote device blob to {}", bin_path.display());
            println!("pubkey (b58): {}", key.pk_b58);
            if let Some(p) = &key.path {
                println!("path: {}", p);
            }
        }

    // Case 3: private key input (file export only)
    } else if let Some(b58) = args.sk_b58.as_deref() {
        let (key, blob) = keys::import_from_b58_priv(b58).map_err(|e| anyhow::anyhow!(e))?;
        if let Some(out) = args.out.as_ref() {
            let (json_path, bin_path) =
                keys::write_key_files(out, &key, &blob).map_err(|e| anyhow::anyhow!(e))?;
            println!("✔ wrote key JSON to {}", json_path.display());
            println!("✔ wrote device blob to {}", bin_path.display());
        }
        println!("pubkey (b58): {}", key.pk_b58);
        if let Some(p) = &key.path {
            println!("path: {}", p);
        }
        println!("(note) no device seed was set from a raw private key input");
    } else if let Some(hx) = args.sk_hex.as_deref() {
        let (key, blob) = keys::import_from_hex_priv(hx).map_err(|e| anyhow::anyhow!(e))?;
        if let Some(out) = args.out.as_ref() {
            let (json_path, bin_path) =
                keys::write_key_files(out, &key, &blob).map_err(|e| anyhow::anyhow!(e))?;
            println!("✔ wrote key JSON to {}", json_path.display());
            println!("✔ wrote device blob to {}", bin_path.display());
        }
        println!("pubkey (b58): {}", key.pk_b58);
        if let Some(p) = &key.path {
            println!("path: {}", p);
        }
        println!("(note) no device seed was set from a raw private key input");
    } else {
        // Clap’s conflicts prevent multiple, but we still ensure “one of” is present.
        anyhow::bail!("provide one of --mnemonic, --seed-hex, --sk-b58, or --sk-hex");
    }

    // Print device info after seeding (mirrors your test flow)
    if did_seed_device {
        println!("seed: set ({} bytes via frag)", seeded_len);
        if let Response::Info {
            proto_v,
            fw_major,
            fw_minor,
            features,
            has_seed,
            cheetah_x,
            cheetah_y,
        } = send_call(&mut *sp, 0x100, Request::GetInfo)?
        {
            println!("info(after): proto_v={proto_v}, fw={fw_major}.{fw_minor}, features=0x{features:08x}, has_seed={has_seed}, pubkey=");
        }
    }
    Ok(())
}
