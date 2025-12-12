use crate::keys::pubkey_to_b58;
use crate::serial::{open, send_call};
use siger_core::{Request, Response};
use std::fmt::Write as _;

pub fn run(port: &str, baud: u32, version: u8) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;

    // Get device info
    let resp: Response = send_call(&mut *sp, 0x01, Request::GetInfo)?;
    match resp {
        Response::Info {
            proto_v,
            fw_major,
            fw_minor,
            features,
            has_seed,
            cheetah_pubs,
        } => {
            println!(
                "info: proto_v={proto_v}, fw={fw_major}.{fw_minor}, features=0x{features:08x}, has_seed={has_seed}"
            );
            if has_seed {
                if cheetah_pubs.is_empty() {
                    println!("  (device locked; pubkeys withheld)");
                } else {
                    for (idx, pubinfo) in cheetah_pubs.iter().enumerate() {
                        let pk_xy = (pubinfo.x, pubinfo.y);
                        let b58 = pubkey_to_b58(&pk_xy, version);
                        let path_display = format_path(pubinfo.path.as_slice());
                        println!(
                            "  slot[{slot}] key[{idx:02}]: path={} pubkey(v{})={}",
                            path_display,
                            version,
                            b58,
                            slot = pubinfo.slot
                        );
                    }
                }
            }
        }
        other => anyhow::bail!("unexpected info response: {other:?}"),
    }

    // Get lock status
    let resp: Response = send_call(&mut *sp, 0x02, Request::GetLockStatus)?;
    match resp {
        Response::OkLockStatus {
            locked,
            attempts_remaining,
        } => {
            println!(
                "status: {}",
                if locked {
                    "🔒 locked"
                } else {
                    "🔓 unlocked"
                }
            );
            println!("attempts remaining: {}", attempts_remaining);
        }
        other => anyhow::bail!("unexpected status response: {other:?}"),
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
