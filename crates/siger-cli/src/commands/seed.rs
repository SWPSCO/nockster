use crate::cli::SeedArgs;
use crate::keys;
use crate::serial::{open, send_blob, send_call};
use crate::util::parse_64;
use siger_core::{FragKind, Request, Response};

pub fn run(args: SeedArgs) -> anyhow::Result<()> {
    let mut did_seed_device = false;
    let mut seeded_len = 0usize;

    // open serial eagerly, but only used when we actually seed the device.
    let mut sp = open(&args.port, args.baud)?;

    // seed via mnemonic
    if let Some(m) = args.seedphrase.as_deref() {
        let seed64 = keys::bip39_seed_from_mnemonic(m, &args.passphrase);

        // use InitializePIN if PIN is provided, otherwise use SetSeed
        if let Some(pin) = &args.pin {
            send_call(
                &mut *sp,
                0x42,
                Request::InitializePIN {
                    pin: pin.clone(),
                    seed64,
                },
            )?;
            println!("initialized device with PIN (encrypted NVS storage)");
        } else {
            send_blob(&mut *sp, 0x42, FragKind::SetSeed, &seed64)?;
        }
        did_seed_device = true;
        seeded_len = seed64.len();

        // optional file outputs
        if let Some(out) = args.out.as_ref() {
            let (key, blob) =
                keys::import_from_seed(&seed64, &args.path).map_err(|e| anyhow::anyhow!(e))?;
            let (json_path, bin_path) =
                keys::write_key_files(out, &key, &blob).map_err(|e| anyhow::anyhow!(e))?;
            println!("wrote key JSON to {}", json_path.display());
            println!("wrote device blob to {}", bin_path.display());
            println!("pubkey (b58): {}", key.pk_b58);
            if let Some(p) = &key.path {
                println!("path: {}", p);
            }
        }

    // seed via seed_hex
    } else if let Some(hx) = args.seed_hex.as_deref() {
        let seed64 = parse_64(hx)?;
        if let Some(pin) = &args.pin {
            send_call(
                &mut *sp,
                0x42,
                Request::InitializePIN {
                    pin: pin.clone(),
                    seed64,
                },
            )?;
            println!("initialized device with pin");
        } else {
            anyhow::bail!("must provide a pin (--pin)");
        }
        did_seed_device = true;
        seeded_len = seed64.len();

        if let Some(out) = args.out.as_ref() {
            let (key, blob) =
                keys::import_from_seed(&seed64, &args.path).map_err(|e| anyhow::anyhow!(e))?;
            let (json_path, bin_path) =
                keys::write_key_files(out, &key, &blob).map_err(|e| anyhow::anyhow!(e))?;
            println!("pubkey: {}", key.pk_b58);
            println!("wrote key JSON to {}", json_path.display());
            println!("wrote device blob to {}", bin_path.display());
            if let Some(p) = &key.path {
                println!("path: {}", p);
            }
        }
    } else {
        // clap’s conflicts prevent multiple, but we still ensure “one of” is present.
        anyhow::bail!("provide one of --seedphrase or --seed-hex");
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
