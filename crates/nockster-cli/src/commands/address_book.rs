//! `address-book` — read the device's stored address book (label → PKH).
//!
//! Surfaces a firmware capability the CLI didn't previously expose. Read-only;
//! entries are added on-device.

use crate::serial::{open, send_call};
use crate::ui;
use nockster_core::{
    describe_error, Request, Response, ERR_DEVICE_LOCKED, FEATURE_DEVICE_ADDRESS_BOOK,
    MAX_DEVICE_ADDRESS_BOOK_ENTRIES,
};

pub fn run(port: &str, baud: u32) -> anyhow::Result<()> {
    let mut sp = open(port, baud)?;
    ui::header("address book");

    // Capability gate: give a clear "update firmware" message instead of a
    // cryptic unsupported-request error on older firmware.
    if let Response::Info { features, .. } = send_call(&mut *sp, 0x01, Request::GetInfo)? {
        if features & FEATURE_DEVICE_ADDRESS_BOOK == 0 {
            anyhow::bail!("device firmware has no address book (update firmware to use this)");
        }
    }

    match send_call(&mut *sp, 0x4a, Request::GetAddressBook)? {
        Response::OkAddressBook(entries) => {
            if entries.is_empty() {
                ui::note("address book is empty");
                return Ok(());
            }
            for entry in &entries {
                ui::item(format!(
                    "{}  {}",
                    ui::strong(entry.label.as_str()),
                    ui::accent(entry.pkh.as_str()),
                ));
            }
            ui::note(&format!(
                "{}/{} entries",
                entries.len(),
                MAX_DEVICE_ADDRESS_BOOK_ENTRIES
            ));
            Ok(())
        }
        Response::Err {
            code: ERR_DEVICE_LOCKED,
        } => anyhow::bail!("device locked; unlock before reading the address book"),
        Response::Err { code } => {
            anyhow::bail!(
                "address-book read failed: {} (code {code})",
                describe_error(code)
            )
        }
        other => anyhow::bail!("unexpected address-book response: {other:?}"),
    }
}
