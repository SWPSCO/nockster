use crate::commands::info::format_features;
use crate::commands::security;
use crate::serial::{open, send_call};
use siger_core::{Request, Response, ERR_NO_SEED, FEATURE_SECURITY_STATUS};

pub fn run(port: &str, baud: u32) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;

    match send_call(&mut *sp, 0x5300, Request::Hello)? {
        Response::Hello(caps) => {
            println!("hello: proto_v={}, compressed_pk={}", caps.proto_v, caps.compressed_pk);
        }
        other => anyhow::bail!("unexpected hello response: {other:?}"),
    }

    let (features, has_seed) = match send_call(&mut *sp, 0x5301, Request::GetInfo)? {
        Response::Info {
            proto_v,
            fw_major,
            fw_minor,
            features,
            has_seed,
            cheetah_pubs,
        } => {
            println!(
                "info: proto_v={proto_v}, fw={fw_major}.{fw_minor}, features=0x{features:08x}, has_seed={has_seed}, pubkeys={}",
                cheetah_pubs.len()
            );
            println!("features: {}", format_features(features));
            (features, has_seed)
        }
        other => anyhow::bail!("unexpected info response: {other:?}"),
    };

    let locked = match send_call(&mut *sp, 0x5302, Request::GetLockStatus)? {
        Response::OkLockStatus {
            locked,
            attempts_remaining,
        } => {
            println!("lock: locked={}, attempts_remaining={attempts_remaining}", yes_no(locked));
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
        println!("security: skipped (firmware does not advertise security-status)");
    }

    if has_seed && !locked {
        match send_call(&mut *sp, 0x5304, Request::Health)? {
            Response::OkCheetahSig { .. } => println!("health: ok"),
            Response::Err { code: ERR_NO_SEED } => {
                println!("health: skipped (seed unavailable)");
            }
            Response::Err { code } => anyhow::bail!("health failed with error code {code}"),
            other => anyhow::bail!("unexpected health response: {other:?}"),
        }
    } else if has_seed {
        println!("health: skipped (device locked)");
    } else {
        println!("health: skipped (device has no seed)");
    }

    println!("smoke: ok");
    Ok(())
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}
