use crate::serial::{open, send_call};
use nockster_core::{Request, Response};

pub fn unlock(port: &str, baud: u32, pin: &str) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;

    match send_call(
        &mut *sp,
        0x43,
        Request::Unlock {
            pin: pin.to_string(),
        },
    )? {
        Response::Ok => {
            println!("✔ device unlocked");
            Ok(())
        }
        Response::Err { code } => {
            match code {
                nockster_core::ERR_WRONG_PIN => {
                    // Get remaining attempts
                    if let Response::OkLockStatus {
                        locked: _,
                        attempts_remaining,
                    } = send_call(&mut *sp, 0x44, Request::GetLockStatus)?
                    {
                        anyhow::bail!("wrong PIN ({} attempts remaining)", attempts_remaining);
                    } else {
                        anyhow::bail!("wrong PIN");
                    }
                }
                nockster_core::ERR_PIN_LOCKED_OUT => {
                    anyhow::bail!("device locked out (too many failed attempts)");
                }
                nockster_core::ERR_NO_SEED => {
                    anyhow::bail!("device not initialized (use 'seed --pin' first)");
                }
                _ => anyhow::bail!("unlock failed with error code 0x{:04x}", code),
            }
        }
        _ => anyhow::bail!("unexpected response"),
    }
}

pub fn lock(port: &str, baud: u32) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;

    match send_call(&mut *sp, 0x45, Request::Lock)? {
        Response::Ok => {
            println!("✔ device locked");
            Ok(())
        }
        Response::Err { code } => {
            anyhow::bail!("lock failed with error code 0x{:04x}", code)
        }
        _ => anyhow::bail!("unexpected response"),
    }
}
