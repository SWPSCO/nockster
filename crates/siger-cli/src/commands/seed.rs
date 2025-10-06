use crate::cli::SeedArgs;
use crate::keys;
use crate::serial::{open, send_blob, send_call};
use crate::util::parse_64;
use siger_core::{
    FragKind, Request, Response, ERR_ALREADY_INITIALIZED, ERR_DEVICE_LOCKED, ERR_OVERFLOW,
    ERR_PIN_LOCKED_OUT, ERR_WRONG_PIN,
};
use std::fmt::Write as _;

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
            match send_call(
                &mut *sp,
                0x42,
                Request::InitializePIN {
                    pin: pin.clone(),
                    seed64,
                },
            )? {
                Response::Ok => {
                    println!("initialized device with PIN (encrypted NVS storage)");
                }
                Response::Err { code } if code == ERR_ALREADY_INITIALIZED => {
                    match send_call(&mut *sp, 0x41, Request::Unlock { pin: pin.clone() })? {
                        Response::Ok => {}
                        Response::Err {
                            code: ERR_WRONG_PIN,
                        } => anyhow::bail!("incorrect PIN"),
                        Response::Err {
                            code: ERR_PIN_LOCKED_OUT,
                        } => {
                            anyhow::bail!("pin locked out")
                        }
                        other => anyhow::bail!("unexpected unlock response: {other:?}"),
                    }

                    match send_call(&mut *sp, 0x43, Request::AddSeed { seed64 })? {
                        Response::Ok => println!("added additional seed slot"),
                        Response::Err {
                            code: ERR_DEVICE_LOCKED,
                        } => {
                            anyhow::bail!("device locked; unlock before adding seed")
                        }
                        Response::Err { code: ERR_OVERFLOW } => {
                            anyhow::bail!("seed storage is full")
                        }
                        Response::Err { code } => {
                            anyhow::bail!("add-seed failed with code {code}")
                        }
                        other => anyhow::bail!("unexpected add-seed response: {other:?}"),
                    }
                }
                Response::Err {
                    code: ERR_WRONG_PIN,
                } => anyhow::bail!("incorrect PIN"),
                Response::Err {
                    code: ERR_PIN_LOCKED_OUT,
                } => {
                    anyhow::bail!("pin locked out")
                }
                Response::Err { code } => anyhow::bail!("initialize failed with code {code}"),
                other => anyhow::bail!("unexpected initialize response: {other:?}"),
            }
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
            match send_call(
                &mut *sp,
                0x42,
                Request::InitializePIN {
                    pin: pin.clone(),
                    seed64,
                },
            )? {
                Response::Ok => println!("initialized device with PIN (encrypted NVS storage)"),
                Response::Err { code } if code == ERR_ALREADY_INITIALIZED => {
                    match send_call(&mut *sp, 0x41, Request::Unlock { pin: pin.clone() })? {
                        Response::Ok => {}
                        Response::Err {
                            code: ERR_WRONG_PIN,
                        } => anyhow::bail!("incorrect PIN"),
                        Response::Err {
                            code: ERR_PIN_LOCKED_OUT,
                        } => {
                            anyhow::bail!("pin locked out")
                        }
                        other => anyhow::bail!("unexpected unlock response: {other:?}"),
                    }

                    match send_call(&mut *sp, 0x43, Request::AddSeed { seed64 })? {
                        Response::Ok => println!("added additional seed slot"),
                        Response::Err {
                            code: ERR_DEVICE_LOCKED,
                        } => {
                            anyhow::bail!("device locked; unlock before adding seed")
                        }
                        Response::Err { code: ERR_OVERFLOW } => {
                            anyhow::bail!("seed storage is full")
                        }
                        Response::Err { code } => anyhow::bail!("add-seed failed with code {code}"),
                        other => anyhow::bail!("unexpected add-seed response: {other:?}"),
                    }
                }
                Response::Err {
                    code: ERR_WRONG_PIN,
                } => anyhow::bail!("incorrect PIN"),
                Response::Err {
                    code: ERR_PIN_LOCKED_OUT,
                } => {
                    anyhow::bail!("pin locked out")
                }
                Response::Err { code } => anyhow::bail!("initialize failed with code {code}"),
                other => anyhow::bail!("unexpected initialize response: {other:?}"),
            }
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
            cheetah_pubs,
        } = send_call(&mut *sp, 0x100, Request::GetInfo)?
        {
            println!(
                "info(after): proto_v={proto_v}, fw={fw_major}.{fw_minor}, features=0x{features:08x}, has_seed={has_seed}"
            );
            if has_seed {
                if cheetah_pubs.is_empty() {
                    println!("  (device locked; pubkeys withheld)");
                } else {
                    for (idx, pubinfo) in cheetah_pubs.iter().enumerate() {
                        let pk_xy = (pubinfo.x, pubinfo.y);
                        let b58 = keys::pubkey_to_b58(&pk_xy);
                        println!(
                            "  slot[{slot}] key[{idx:02}]: path={} pubkey={}",
                            format_path(pubinfo.path.as_slice()),
                            b58,
                            slot = pubinfo.slot
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

fn format_path(path: &[u32]) -> String {
    let mut out = String::from("m");
    for &component in path {
        let hardened = (component & 0x8000_0000) != 0;
        let index = component & 0x7FFF_FFFF;
        out.push('/');
        let _ = write!(out, "{}", index);
        if hardened {
            out.push('\'');
        }
    }
    out
}
