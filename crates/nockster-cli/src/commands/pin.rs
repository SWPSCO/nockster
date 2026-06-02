use crate::serial::{open, send_call};
use crate::ui;
use anyhow::bail;
use nockster_core::{
    Request, Response, ERR_BUSY, ERR_CRYPTO, ERR_FLASH, ERR_NO_SEED, ERR_PIN_LOCKED_OUT,
    ERR_PIN_MISMATCH, ERR_UNSUPPORTED_VERSION, ERR_WRONG_PIN,
};

pub fn run(port: &str, baud: u32, current_pin: &str) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;

    ui::info("enter the new PIN twice on the device");
    match send_call(
        &mut *sp,
        0x47,
        Request::ChangePinOnDevice {
            current_pin: current_pin.to_string(),
        },
    )? {
        Response::Ok => {
            ui::ok("changed PIN");
            Ok(())
        }
        Response::Err {
            code: ERR_WRONG_PIN,
        } => {
            if let Response::OkLockStatus {
                locked: _,
                attempts_remaining,
            } = send_call(&mut *sp, 0x48, Request::GetLockStatus)?
            {
                bail!(
                    "current PIN is incorrect ({} attempts remaining)",
                    attempts_remaining
                );
            }
            bail!("current PIN is incorrect")
        }
        Response::Err {
            code: ERR_PIN_MISMATCH,
        } => bail!("new PIN entries did not match"),
        Response::Err {
            code: ERR_PIN_LOCKED_OUT,
        } => bail!("device locked out (too many failed attempts)"),
        Response::Err { code: ERR_NO_SEED } => {
            bail!("device not initialized (use 'seed --pin' first)")
        }
        Response::Err { code: ERR_BUSY } => bail!("device is busy"),
        Response::Err {
            code: ERR_UNSUPPORTED_VERSION,
        } => bail!("firmware does not support on-device PIN changes"),
        Response::Err { code: ERR_FLASH } => bail!("flash write failed while changing PIN"),
        Response::Err { code: ERR_CRYPTO } => bail!("crypto failure while changing PIN"),
        Response::Err { code } => bail!("PIN change failed with error code 0x{code:04x}"),
        other => bail!("unexpected response: {other:?}"),
    }
}
