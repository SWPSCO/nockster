use crate::keys::pubkey_to_b58;
use crate::serial::{open, send_call};
use crate::util::fmt_u64x6;
use siger_core::{Request, Response};

pub fn run(port: &str, baud: u32) -> anyhow::Result<()> {
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
            cheetah_x,
            cheetah_y,
        } => {
            println!(
              "info: proto_v={proto_v}, fw={fw_major}.{fw_minor}, features=0x{features:08x}, has_seed={has_seed}"
          );
            if has_seed {
                eprintln!("DEBUG X: {:?}", cheetah_x);
                eprintln!("DEBUG Y: {:?}", cheetah_y);
                let pk_xy = (cheetah_x, cheetah_y);
                let b58 = pubkey_to_b58(&pk_xy);
                println!("public key: {b58}");
            }
        }
        other => anyhow::bail!("unexpected info response: {other:?}"),
    }

    // Get lock status
    let resp: Response = send_call(&mut *sp, 0x02, Request::GetLockStatus)?;
    match resp {
        Response::OkLockStatus { locked, attempts_remaining } => {
            println!("status: {}", if locked { "🔒 locked" } else { "🔓 unlocked" });
            println!("attempts remaining: {}", attempts_remaining);
        }
        other => anyhow::bail!("unexpected status response: {other:?}"),
    }

    Ok(())
}
