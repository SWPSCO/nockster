use crate::serial::{open, send_call};
use siger_core::{Request, Response};

pub fn run(port: &str, baud: u32) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;
    let resp: Response = send_call(&mut *sp, 2, Request::Health)?;
    println!("{resp:?}");
    Ok(())
}
