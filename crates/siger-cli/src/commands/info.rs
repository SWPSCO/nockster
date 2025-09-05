use siger_core::{Request, Response, Frame};
use crate::util::{fmt_u64x6};
use crate::keys::{pubkey_to_b58};
use crate::serial::{open, send_recv};

pub fn run(port: &str, baud: u32) -> anyhow::Result<()> {
  let mut sp = open(port, baud)?;
  let resp: Response = send_recv(&mut *sp, 1, Frame::One(Request::GetInfo))?;
  match resp {
      Response::Info { proto_v, fw_major, fw_minor, features, has_seed, cheetah_x, cheetah_y } => {
          println!(
              "info: proto_v={proto_v}, fw={fw_major}.{fw_minor}, features=0x{features:08x}, has_seed={has_seed}"
          );
          if has_seed {
              let pk_xy = (cheetah_x, cheetah_y);
              let b58 = pubkey_to_b58(&pk_xy);
              println!("public key: {b58}");
          }
      }
      other => anyhow::bail!("unexpected: {other:?}"),
  }
  Ok(())
}