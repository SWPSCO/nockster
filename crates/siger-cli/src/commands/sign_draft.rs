use std::fs;
use anyhow::Context;
use siger_core::FragKind;
use crate::serial::{open, send_blob_and_recv_outbound};

pub fn run(port: &str, baud: u32, draft_path: &str, out_path: Option<&str>) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;
    let bytes = fs::read(draft_path).with_context(|| format!("read {draft_path}"))?;
    let ret = send_blob_and_recv_outbound(&mut *sp, 0x99, FragKind::SignDraft, &bytes)?;
    if let Some(p) = out_path {
        fs::write(p, &ret).with_context(|| format!("write {p}"))?;
        println!("wrote {} bytes to {}", ret.len(), p);
    } else {
        println!("received {} bytes:", ret.len());
        println!("{}", hex::encode(&ret));
    }
    Ok(())
}
