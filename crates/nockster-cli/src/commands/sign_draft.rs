use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use nockster_core::{draft_sign, FragKind, Request, Response};

use crate::serial::{open, send_blob_and_recv_outbound, send_call};
use crate::ui;

fn default_out_path_for(input: &str, explicit: Option<&str>) -> PathBuf {
    match explicit {
        Some(s) if !s.is_empty() => PathBuf::from(s),
        _ => {
            let p = Path::new(input);
            let mut out = p.to_path_buf();
            out.set_extension("tx");
            out
        }
    }
}

pub fn run(
    port: &str,
    baud: u32,
    draft_path: &str,
    out_opt: Option<&str>,
    slot: u8,
    host_txid: bool,
) -> Result<()> {
    let draft_bytes = fs::read(draft_path).with_context(|| format!("read {draft_path}"))?;

    let mut sp = open(port, baud)?;

    // Select seed slot (defaults to 0).
    match send_call(&mut *sp, 0x5001, Request::SelectSeed { slot })? {
        Response::Ok => {}
        Response::Err { code } => return Err(anyhow!("SelectSeed failed with code {code}")),
        other => return Err(anyhow!("unexpected SelectSeed response: {other:?}")),
    }

    let mut out_bytes =
        send_blob_and_recv_outbound(&mut *sp, 0xD001, FragKind::SignDraft, &draft_bytes)
            .context("device SignDraft")?;

    if host_txid {
        let rewritten = draft_sign::rewrite_txid_v1(&out_bytes).map_err(|err| {
            anyhow!("host tx-id rewrite failed: output is not a supported V1 transaction shape ({err:?})")
        })?;
        if let Some(bytes) = rewritten.rewritten {
            out_bytes = bytes;
        }
        ui::kv("host tx-id", ui::accent(&rewritten.name));
    }

    let out_path = default_out_path_for(draft_path, out_opt);
    fs::write(&out_path, &out_bytes).with_context(|| format!("write {}", out_path.display()))?;

    ui::ok(&format!(
        "wrote {} bytes to {}",
        out_bytes.len(),
        out_path.display()
    ));
    Ok(())
}
