use crate::cli::PortArgs;
use crate::serial::{open, send_call, send_call_with_deadline, Link};
use anyhow::{Context, Result};
use nockster_core::{Request, Response, FEATURE_DEVICE_REBOOT};
use std::time::Duration;

pub fn run(args: &PortArgs) -> Result<()> {
    let mut sp = open(&args.port, args.baud)?;
    request_device_reboot(&mut *sp)
}

pub(crate) fn request_device_reboot(sp: &mut dyn Link) -> Result<()> {
    match send_call(sp, 0x0600, Request::GetInfo)? {
        Response::Info { features, .. } if (features & FEATURE_DEVICE_REBOOT) != 0 => {}
        Response::Info { .. } => {
            anyhow::bail!("device firmware does not advertise reboot support");
        }
        Response::Err { code } => {
            anyhow::bail!("reboot preflight failed: device returned error code {code}");
        }
        other => anyhow::bail!("reboot preflight failed: unexpected response: {other:?}"),
    }

    match send_call_with_deadline(sp, 0x0601, Request::Reboot, Duration::from_secs(5)) {
        Ok(Response::Ok) => {
            println!("device reboot requested");
            Ok(())
        }
        Ok(Response::Err { code }) => anyhow::bail!("reboot failed with error code {code}"),
        Ok(other) => anyhow::bail!("unexpected reboot response: {other:?}"),
        Err(err) if reboot_read_disconnect_indicates_success(&err) => {
            println!("device reboot command sent; device may already be restarting");
            Ok(())
        }
        Err(err) => Err(err).context("request device reboot"),
    }
}

fn reboot_read_disconnect_indicates_success(err: &anyhow::Error) -> bool {
    let message = err.to_string();
    message.contains("timed out waiting for response") || message.contains("serial read:")
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn reboot_disconnect_filter_only_accepts_read_side_failures() {
        assert!(reboot_read_disconnect_indicates_success(&anyhow!(
            "serial read: hid read timeout"
        )));
        assert!(reboot_read_disconnect_indicates_success(&anyhow!(
            "serial read: No such device"
        )));
        assert!(reboot_read_disconnect_indicates_success(&anyhow!(
            "serial read: timed out waiting for response (5000 ms)"
        )));

        assert!(!reboot_read_disconnect_indicates_success(&anyhow!(
            "write request: No such device"
        )));
        assert!(!reboot_read_disconnect_indicates_success(&anyhow!(
            "reboot failed with error code 110"
        )));
    }
}
