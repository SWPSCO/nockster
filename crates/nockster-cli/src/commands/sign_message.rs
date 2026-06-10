//! `sign-message` — sign an arbitrary message on-device.
//!
//! The device hashes the message itself (nockchain `++page-msg`, Tip5),
//! displays the text for review, and Cheetah-schnorr signs the digest after
//! on-screen confirmation. Domain-separated from spend signing by the prompt.

use anyhow::anyhow;

use crate::cli::SignMessageArgs;
use crate::keys::parse_path;
use crate::serial::{open, send_call};
use crate::ui;
use crate::util::format_path;
use nockster_core::{describe_error, Request, Response};

pub fn run(args: SignMessageArgs) -> anyhow::Result<()> {
    let message = match (args.message.as_deref(), args.file.as_ref()) {
        (Some(text), None) => text.as_bytes().to_vec(),
        (None, Some(path)) => std::fs::read(path)?,
        _ => anyhow::bail!("provide the message with --message or --file"),
    };
    let path = parse_path(&args.path).map_err(|err| anyhow!("invalid path: {err}"))?;

    let mut sp = open(&args.port, args.baud)?;
    ui::header("sign message");
    ui::kv("slot", ui::strong(&args.slot.to_string()));
    ui::kv("path", ui::strong(&format_path(&path)));
    if let Ok(text) = core::str::from_utf8(&message) {
        ui::kv("message", ui::accent(text));
    } else {
        ui::kv("message", ui::accent(&format!("{} bytes (binary)", message.len())));
    }
    ui::info("review the message on the device screen, then approve on-device");

    match send_call(
        &mut *sp,
        0x5710,
        Request::SignMessage {
            slot: args.slot,
            path,
            message,
        },
    )? {
        Response::OkCheetahSig { chal, sig } => {
            ui::ok("device signed the message");
            ui::kv("chal", ui::dim(&fmt_u64x8(&chal)));
            ui::kv("sig", ui::dim(&fmt_u64x8(&sig)));
            Ok(())
        }
        Response::Err { code } => Err(anyhow!(
            "sign-message failed: {} (code {code})",
            describe_error(code)
        )),
        other => Err(anyhow!("unexpected sign-message response: {other:?}")),
    }
}

fn fmt_u64x8(v: &[u64; 8]) -> String {
    v.iter()
        .map(|x| format!("{x:016x}"))
        .collect::<Vec<_>>()
        .join("")
}
