//! `show-address` — ask the device to render a receive PKH for verification.

use anyhow::{anyhow, Context};
use nockster_core::{describe_error, Request, Response};

use crate::keys::{parse_path, pubkey_to_b58};
use crate::serial::{open, send_call};
use crate::ui;
use crate::util::format_path;

pub fn run(port: &str, baud: u32, slot: u8, path_str: &str) -> anyhow::Result<()> {
    let path = parse_path(path_str).map_err(|err| anyhow!("invalid path: {err}"))?;
    let pretty_path = format_path(&path);

    let mut sp = open(port, baud)?;
    ui::header("show address");
    ui::kv("slot", ui::strong(&slot.to_string()));
    ui::kv("path", ui::strong(&pretty_path));

    let address = match send_call(
        &mut *sp,
        0x5700,
        Request::GetCheetahPub {
            slot,
            path: path.clone(),
        },
    )
    .context("read device pubkey")?
    {
        Response::OkCheetahPub { x, y } => pubkey_to_b58(&(x, y), 1),
        Response::Err { code } => {
            return Err(anyhow!(
                "device pubkey read failed: {} (code {code})",
                describe_error(code)
            ))
        }
        other => return Err(anyhow!("unexpected GetCheetahPub response: {other:?}")),
    };

    ui::kv("v1 pkh", ui::accent(&address));
    ui::info("compare this value with the device screen, then approve on-device");

    match send_call(&mut *sp, 0x5701, Request::ShowAddress { slot, path })? {
        Response::Ok => {
            ui::ok("device confirmed receive address display");
            Ok(())
        }
        Response::Err { code } => Err(anyhow!(
            "show-address failed: {} (code {code})",
            describe_error(code)
        )),
        other => Err(anyhow!("unexpected ShowAddress response: {other:?}")),
    }
}
