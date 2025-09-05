use siger_core::{Request, Response};
use crate::serial::{open, send_recv};

pub fn run(port: &str, baud: u32) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;
    let resp: Response = send_recv(&mut *sp, 2, siger_core::Frame::One(Request::Health))?;
    println!("{resp:?}");
    Ok(())
}
