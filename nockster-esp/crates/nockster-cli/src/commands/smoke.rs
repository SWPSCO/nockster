use crate::cli::SmokeArgs;
use crate::commands::info::format_features;
use crate::commands::security;
use crate::commands::sign_draft;
use crate::serial::{open, send_call, Link};
use crate::ui;
use nockster_core::{CheetahPub, Request, Response, ERR_NO_SEED, FEATURE_SECURITY_STATUS};
use std::fmt::Write as _;

pub fn run(args: &SmokeArgs) -> anyhow::Result<()> {
    let mut sp = open(&args.port, args.baud)?;
    ui::header("smoke");

    match send_call(&mut *sp, 0x5300, Request::Hello)? {
        Response::Hello(caps) => {
            ui::kv("proto", ui::strong(&caps.proto_v.to_string()));
            ui::kv("compressed pk", ui::yesno(caps.compressed_pk));
        }
        other => anyhow::bail!("unexpected hello response: {other:?}"),
    }

    let (features, has_seed, cheetah_pubs) = match send_call(&mut *sp, 0x5301, Request::GetInfo)? {
        Response::Info {
            proto_v,
            fw_major,
            fw_minor,
            features,
            has_seed,
            cheetah_pubs,
        } => {
            ui::kv("firmware", ui::strong(&format!("{fw_major}.{fw_minor}")));
            ui::kv("proto", ui::strong(&proto_v.to_string()));
            ui::kv("pubkeys", ui::strong(&cheetah_pubs.len().to_string()));
            ui::kv("has seed", ui::yesno(has_seed));
            ui::kv("features", format_features(features));
            (features, has_seed, cheetah_pubs)
        }
        other => anyhow::bail!("unexpected info response: {other:?}"),
    };

    let locked = match send_call(&mut *sp, 0x5302, Request::GetLockStatus)? {
        Response::OkLockStatus {
            locked,
            attempts_remaining,
        } => {
            let dot = if locked {
                ui::dot(ui::Health::Bad, "locked")
            } else {
                ui::dot(ui::Health::Good, "unlocked")
            };
            ui::kv(
                "lock",
                format!(
                    "{}  {}",
                    dot,
                    ui::dim(&format!("attempts {attempts_remaining}"))
                ),
            );
            locked
        }
        other => anyhow::bail!("unexpected lock-status response: {other:?}"),
    };

    if features & FEATURE_SECURITY_STATUS != 0 {
        match send_call(&mut *sp, 0x5303, Request::GetSecurityStatus)? {
            Response::OkSecurityStatus(status) => security::print_status(&status),
            other => anyhow::bail!("unexpected security response: {other:?}"),
        }
    } else {
        ui::subhead("security");
        ui::note("skipped (firmware does not advertise security-status)");
    }

    ui::subhead("checks");
    if has_seed && !locked {
        check_seed_pubkeys(&mut *sp, &cheetah_pubs)?;

        match send_call(&mut *sp, 0x5304, Request::Health)? {
            Response::OkCheetahSig { .. } => ui::ok("health"),
            Response::Err { code: ERR_NO_SEED } => {
                ui::note("health: skipped (seed unavailable)");
            }
            Response::Err { code } => anyhow::bail!("health failed with error code {code}"),
            other => anyhow::bail!("unexpected health response: {other:?}"),
        }
    } else if has_seed {
        ui::note("health: skipped (device locked)");
    } else {
        ui::note("health: skipped (device has no seed)");
    }

    if let Some(draft_path) = args.sign_draft.as_deref() {
        if !has_seed {
            anyhow::bail!("sign-draft smoke requested, but device has no seed");
        }
        if locked {
            anyhow::bail!("sign-draft smoke requested, but device is locked");
        }
        drop(sp);
        ui::info(&format!(
            "sign-draft: requesting on-device approval for {draft_path}"
        ));
        sign_draft::run(
            &args.port,
            args.baud,
            draft_path,
            args.out.as_deref(),
            args.slot,
            args.host_txid,
        )?;
    }

    ui::rule();
    ui::ok("smoke ok");
    Ok(())
}

fn check_seed_pubkeys(sp: &mut dyn Link, cheetah_pubs: &[CheetahPub]) -> anyhow::Result<()> {
    if cheetah_pubs.is_empty() {
        anyhow::bail!("pubkey: unlocked device reported no seed pubkeys");
    }

    for (idx, pubinfo) in cheetah_pubs.iter().enumerate() {
        let id = 0x5310u32.saturating_add(idx as u32);
        match send_call(
            sp,
            id,
            Request::GetCheetahPub {
                slot: pubinfo.slot,
                path: pubinfo.path.clone(),
            },
        )? {
            Response::OkCheetahPub { x, y } if x == pubinfo.x && y == pubinfo.y => {
                ui::ok(&format!(
                    "pubkey slot {} {}",
                    pubinfo.slot,
                    ui::dim(&format_path(pubinfo.path.as_slice()))
                ));
            }
            Response::OkCheetahPub { .. } => anyhow::bail!(
                "pubkey: slot[{}] path={} mismatch",
                pubinfo.slot,
                format_path(pubinfo.path.as_slice())
            ),
            Response::Err { code } => anyhow::bail!(
                "pubkey: slot[{}] path={} failed with code {code}",
                pubinfo.slot,
                format_path(pubinfo.path.as_slice())
            ),
            other => anyhow::bail!("unexpected pubkey response: {other:?}"),
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
