use crate::cli::PortArgs;
use crate::serial::{open, send_call};
use nockster_core::{Request, Response};

pub fn run(args: &PortArgs) -> anyhow::Result<()> {
    let mut sp = open(&args.port, args.baud)?;
    match send_call(&mut *sp, 0x0500, Request::Reset)? {
        Response::Ok => {
            println!("device reset: seed and persistent state cleared");
            Ok(())
        }
        Response::Err { code } => anyhow::bail!("reset failed with error code {code}"),
        other => anyhow::bail!("unexpected reset response: {other:?}"),
    }
}
