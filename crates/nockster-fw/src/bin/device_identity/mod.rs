use core::fmt::Write as _;

use esp_hal::efuse::{Efuse, BLOCK_USR_DATA};
use heapless::String as HString;

// Reject erased eFuse data and obviously non-timestamp contents. Unix time
// reaches u32::MAX in 2106, so this range covers the useful life of the device.
const MIN_SERIAL_TIMESTAMP: u64 = 1_577_836_800; // 2020-01-01T00:00:00Z
const MAX_SERIAL_TIMESTAMP: u64 = u32::MAX as u64;

pub fn provisioned_serial_timestamp() -> Option<u64> {
    let timestamp = Efuse::read_field_le::<u64>(BLOCK_USR_DATA);
    (MIN_SERIAL_TIMESTAMP..=MAX_SERIAL_TIMESTAMP)
        .contains(&timestamp)
        .then_some(timestamp)
}

/// New production wallets must have the immutable provisioning identity.
///
/// This is deliberately an initialization-only policy. Production firmware
/// predating the serial eFuse shipped without `BLOCK_USR_DATA`, and refusing
/// to unlock those already-initialized wallets would both strand their keys
/// and turn a provisioning error into consumed PIN attempts.
pub fn production_identity_ready_for_initialization() -> bool {
    !is_production_build() || provisioned_serial_timestamp().is_some()
}

pub fn usb_serial() -> HString<20> {
    let mut out = HString::new();
    match provisioned_serial_timestamp() {
        Some(timestamp) => {
            let _ = write!(out, "{timestamp}");
        }
        None => {
            let _ = out.push_str("UNPROVISIONED");
        }
    }
    out
}

fn is_production_build() -> bool {
    option_env!("NOCKSTER_BUILD_PROFILE") == Some("production")
}
