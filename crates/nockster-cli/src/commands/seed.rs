use crate::cli::SeedArgs;
use crate::keys;
use crate::serial::{open, send_call, Link};
use crate::ui;
use crate::util::parse_64;
use nockster_core::{
    Request, Response, SeedSlotLabel, ERR_ALREADY_INITIALIZED, ERR_BAD_COBS_OR_POSTCARD,
    ERR_CRYPTO, ERR_DEVICE_LOCKED, ERR_FLASH, ERR_NO_SEED, ERR_OVERFLOW, ERR_PIN_LOCKED_OUT,
    ERR_REJECTED_BY_USER, ERR_WRONG_PIN, MAX_SEED_LABEL_LEN,
};
use std::fmt::Write as _;

pub fn run(args: SeedArgs) -> anyhow::Result<()> {
    let adding_seed = args.seedphrase.is_some() || args.seed_hex.is_some();
    let labeling = args.label.is_some();
    let managing_slot = args.list || args.select.is_some() || args.delete.is_some() || labeling;
    if !adding_seed && !managing_slot {
        anyhow::bail!(
            "provide one of --seedphrase, --seed-hex, --list, --select, --delete, or --label"
        );
    }
    if adding_seed && args.pin.is_none() {
        anyhow::bail!("must provide --pin when adding or initializing a seed");
    }
    if let Some(label) = args.label.as_deref() {
        validate_label(label)?;
        if args.list || args.delete.is_some() {
            anyhow::bail!("use --label when adding a seed or with --select <slot>");
        }
        if !adding_seed && args.select.is_none() {
            anyhow::bail!("use --label with --select <slot> to label an existing seed slot");
        }
    }
    if let Some(slot) = args.delete {
        if !args.yes {
            anyhow::bail!(
                "refusing to delete seed slot {slot} without --yes; the device will also ask for confirmation"
            );
        }
    }

    let mut sp = open(&args.port, args.baud)?;
    ui::header("seed");

    if args.list {
        return print_seed_slots(&mut *sp, args.version);
    }

    if let Some(slot) = args.select {
        match send_call(&mut *sp, 0x44, Request::SelectSeed { slot })? {
            Response::Ok => {
                ui::ok(&format!("selected seed slot {slot}"));
                if let Some(label) = args.label.as_deref() {
                    set_seed_label(&mut *sp, slot, label)?;
                }
                return print_seed_slots(&mut *sp, args.version);
            }
            Response::Err {
                code: ERR_DEVICE_LOCKED,
            } => anyhow::bail!("device locked; unlock before selecting a seed slot"),
            Response::Err { code: ERR_NO_SEED } => {
                anyhow::bail!("seed slot {slot} is not available")
            }
            Response::Err { code } => anyhow::bail!("select-seed failed with code {code}"),
            other => anyhow::bail!("unexpected select-seed response: {other:?}"),
        }
    }

    if let Some(slot) = args.delete {
        match send_call(&mut *sp, 0x45, Request::DeleteSeed { slot })? {
            Response::Ok => {
                ui::ok(&format!("deleted seed slot {slot}"));
                return print_seed_slots(&mut *sp, args.version);
            }
            Response::Err {
                code: ERR_REJECTED_BY_USER,
            } => anyhow::bail!("delete rejected on device"),
            Response::Err {
                code: ERR_DEVICE_LOCKED,
            } => anyhow::bail!("device locked; unlock before deleting a seed slot"),
            Response::Err { code: ERR_NO_SEED } => {
                anyhow::bail!("seed slot {slot} is not available")
            }
            Response::Err {
                code: ERR_WRONG_PIN,
            } => anyhow::bail!("stored master key failed verification; lock and unlock again"),
            Response::Err {
                code: ERR_PIN_LOCKED_OUT,
            } => anyhow::bail!("pin locked out"),
            Response::Err { code } if code == ERR_FLASH => {
                anyhow::bail!("delete failed: device flash error (code {code})")
            }
            Response::Err { code } => anyhow::bail!("delete-seed failed with code {code}"),
            other => anyhow::bail!("unexpected delete-seed response: {other:?}"),
        }
    }

    let Some(pin) = args.pin.as_ref() else {
        anyhow::bail!("must provide --pin when adding or initializing a seed");
    };

    // seed via mnemonic
    if let Some(m) = args.seedphrase.as_deref() {
        let seed64 = keys::bip39_seed_from_mnemonic(m, &args.passphrase);

        let slot = initialize_or_add_seed(&mut *sp, pin, seed64)?;
        if let Some(label) = args.label.as_deref() {
            set_seed_label(&mut *sp, slot, label)?;
        }
        // optional file outputs
        if let Some(out) = args.out.as_ref() {
            let (key, blob) = keys::import_from_seed(&seed64, &args.path, args.version)
                .map_err(|e| anyhow::anyhow!(e))?;
            let (json_path, bin_path) =
                keys::write_key_files(out, &key, &blob).map_err(|e| anyhow::anyhow!(e))?;
            ui::kv("pubkey", ui::accent(&key.pk_b58));
            ui::kv("version", ui::strong(&format!("v{}", args.version)));
            if let Some(p) = &key.path {
                ui::kv("path", ui::strong(p));
            }
            ui::ok(&format!("wrote key JSON to {}", json_path.display()));
            ui::ok(&format!("wrote device blob to {}", bin_path.display()));
        }

    // seed via seed_hex
    } else if let Some(hx) = args.seed_hex.as_deref() {
        let seed64 = parse_64(hx)?;
        let slot = initialize_or_add_seed(&mut *sp, pin, seed64)?;
        if let Some(label) = args.label.as_deref() {
            set_seed_label(&mut *sp, slot, label)?;
        }
        if let Some(out) = args.out.as_ref() {
            let (key, blob) = keys::import_from_seed(&seed64, &args.path, args.version)
                .map_err(|e| anyhow::anyhow!(e))?;
            let (json_path, bin_path) =
                keys::write_key_files(out, &key, &blob).map_err(|e| anyhow::anyhow!(e))?;
            ui::kv("pubkey", ui::accent(&key.pk_b58));
            ui::kv("version", ui::strong(&format!("v{}", args.version)));
            if let Some(p) = &key.path {
                ui::kv("path", ui::strong(p));
            }
            ui::ok(&format!("wrote key JSON to {}", json_path.display()));
            ui::ok(&format!("wrote device blob to {}", bin_path.display()));
        }
    } else {
        // clap’s conflicts and the preflight above should make this unreachable.
        anyhow::bail!("provide one of --seedphrase, --seed-hex, --list, --select, or --delete");
    }

    // Print device info after seeding
    // Give firmware time to finish NVS writes and display updates.
    std::thread::sleep(std::time::Duration::from_millis(500));
    print_seed_slots(&mut *sp, args.version)?;
    Ok(())
}

fn initialize_or_add_seed(sp: &mut dyn Link, pin: &str, seed64: [u8; 64]) -> anyhow::Result<u8> {
    if device_has_seed(sp)? {
        return unlock_and_add_seed(sp, pin, seed64);
    }

    match send_call(
        sp,
        0x42,
        Request::InitializePIN {
            pin: pin.to_string(),
            seed64,
        },
    )? {
        Response::Ok => {
            ui::ok("initialized device with PIN (encrypted NVS storage)");
            Ok(0)
        }
        Response::Err { code } if code == ERR_ALREADY_INITIALIZED => {
            unlock_and_add_seed(sp, pin, seed64)
        }
        Response::Err {
            code: ERR_WRONG_PIN,
        } => anyhow::bail!("incorrect PIN"),
        Response::Err {
            code: ERR_PIN_LOCKED_OUT,
        } => anyhow::bail!("pin locked out"),
        Response::Err { code } if code == ERR_FLASH => {
            anyhow::bail!("initialize failed: device flash error (code {code})")
        }
        Response::Err { code } if code == ERR_CRYPTO => {
            anyhow::bail!("initialize failed: device crypto/RNG error (code {code})")
        }
        Response::Err { code } => anyhow::bail!("initialize failed with code {code}"),
        other => anyhow::bail!("unexpected initialize response: {other:?}"),
    }
}

fn device_has_seed(sp: &mut dyn Link) -> anyhow::Result<bool> {
    match send_call(sp, 0x40, Request::GetInfo)? {
        Response::Info { has_seed, .. } => Ok(has_seed),
        other => anyhow::bail!("unexpected info response: {other:?}"),
    }
}

fn unlock_and_add_seed(sp: &mut dyn Link, pin: &str, seed64: [u8; 64]) -> anyhow::Result<u8> {
    match send_call(
        sp,
        0x41,
        Request::Unlock {
            pin: pin.to_string(),
        },
    )? {
        Response::Ok => {}
        Response::Err {
            code: ERR_WRONG_PIN,
        } => anyhow::bail!("incorrect PIN"),
        Response::Err {
            code: ERR_PIN_LOCKED_OUT,
        } => anyhow::bail!("pin locked out"),
        Response::Err { code: ERR_NO_SEED } => {
            anyhow::bail!("device not initialized; retry seed initialization")
        }
        other => anyhow::bail!("unexpected unlock response: {other:?}"),
    }

    let slot = seed_slot_count(sp)?;
    match send_call(sp, 0x43, Request::AddSeed { seed64 })? {
        Response::Ok => {
            ui::ok(&format!("added additional seed slot {slot}"));
            Ok(slot)
        }
        Response::Err {
            code: ERR_DEVICE_LOCKED,
        } => anyhow::bail!("device locked; unlock before adding seed"),
        Response::Err { code: ERR_OVERFLOW } => anyhow::bail!("seed storage is full"),
        Response::Err { code } if code == ERR_FLASH => {
            anyhow::bail!("add-seed failed: device flash error (code {code})")
        }
        Response::Err { code } if code == ERR_CRYPTO => {
            anyhow::bail!("add-seed failed: device crypto/RNG error (code {code})")
        }
        Response::Err { code } => anyhow::bail!("add-seed failed with code {code}"),
        other => anyhow::bail!("unexpected add-seed response: {other:?}"),
    }
}

fn print_seed_slots(sp: &mut dyn Link, version: u8) -> anyhow::Result<()> {
    let labels = read_seed_labels(sp)?;
    match send_call(sp, 0x100, Request::GetInfo)? {
        Response::Info {
            has_seed,
            cheetah_pubs,
            ..
        } => {
            ui::subhead("seeds");
            if !has_seed {
                ui::note("no seed slots");
                return Ok(());
            }
            if cheetah_pubs.is_empty() {
                ui::note("device locked; pubkeys withheld");
                for entry in labels.iter() {
                    ui::item(format!(
                        "slot {}  {}",
                        entry.slot,
                        ui::strong(&format!("\"{}\"", entry.label))
                    ));
                }
                return Ok(());
            }
            for pubinfo in cheetah_pubs.iter() {
                let pk_xy = (pubinfo.x, pubinfo.y);
                let b58 = keys::pubkey_to_b58(&pk_xy, version);
                let label = match label_for_slot(&labels, pubinfo.slot) {
                    Some(label) => format!("{}  ", ui::strong(&format!("\"{label}\""))),
                    None => String::new(),
                };
                ui::item(format!(
                    "slot {}  {}{}  {}",
                    pubinfo.slot,
                    label,
                    ui::dim(&format_path(pubinfo.path.as_slice())),
                    ui::accent(&b58),
                ));
            }
            Ok(())
        }
        other => anyhow::bail!("unexpected info response: {other:?}"),
    }
}

fn seed_slot_count(sp: &mut dyn Link) -> anyhow::Result<u8> {
    match send_call(sp, 0x101, Request::GetInfo)? {
        Response::Info { cheetah_pubs, .. } => Ok(cheetah_pubs.len() as u8),
        other => anyhow::bail!("unexpected info response: {other:?}"),
    }
}

fn read_seed_labels(sp: &mut dyn Link) -> anyhow::Result<Vec<SeedSlotLabel>> {
    match send_call(sp, 0x102, Request::GetSeedLabels)? {
        Response::OkSeedLabels(labels) => Ok(labels),
        Response::Err {
            code: ERR_BAD_COBS_OR_POSTCARD,
        } => Ok(Vec::new()),
        Response::Err { code } if code == ERR_FLASH => {
            anyhow::bail!("read seed labels failed: device flash error (code {code})")
        }
        Response::Err { code } => anyhow::bail!("read seed labels failed with code {code}"),
        other => anyhow::bail!("unexpected seed-label response: {other:?}"),
    }
}

fn set_seed_label(sp: &mut dyn Link, slot: u8, label: &str) -> anyhow::Result<()> {
    validate_label(label)?;
    let mut label_msg = SeedSlotLabel {
        slot,
        label: Default::default(),
    };
    label_msg
        .label
        .push_str(label)
        .map_err(|_| anyhow::anyhow!("label is too long"))?;

    match send_call(
        sp,
        0x103,
        Request::SetSeedLabel {
            slot,
            label: label_msg.label,
        },
    )? {
        Response::Ok => {
            if label.is_empty() {
                ui::ok(&format!("cleared label for seed slot {slot}"));
            } else {
                ui::ok(&format!("labeled seed slot {slot}: {label}"));
            }
            Ok(())
        }
        Response::Err {
            code: ERR_DEVICE_LOCKED,
        } => anyhow::bail!("device locked; unlock before labeling a seed slot"),
        Response::Err { code: ERR_NO_SEED } => anyhow::bail!("seed slot {slot} is not available"),
        Response::Err { code } if code == ERR_FLASH => {
            anyhow::bail!("label failed: device flash error (code {code})")
        }
        Response::Err { code } => anyhow::bail!("label failed with code {code}"),
        other => anyhow::bail!("unexpected seed-label response: {other:?}"),
    }
}

fn validate_label(label: &str) -> anyhow::Result<()> {
    if label.len() > MAX_SEED_LABEL_LEN {
        anyhow::bail!("label must be at most {MAX_SEED_LABEL_LEN} bytes");
    }
    if !label
        .bytes()
        .all(|byte| byte == b' ' || (0x21..=0x7e).contains(&byte))
    {
        anyhow::bail!("label must use printable ASCII characters");
    }
    Ok(())
}

fn label_for_slot(labels: &[SeedSlotLabel], slot: u8) -> Option<&str> {
    labels
        .iter()
        .find(|entry| entry.slot == slot)
        .map(|entry| entry.label.as_str())
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
