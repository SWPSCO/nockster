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

/// Every ESP32-S3 has an immutable factory MAC, so wallet initialization never
/// depends on the optional operator-provisioned serial timestamp.
pub fn production_identity_ready_for_initialization() -> bool {
    true
}

pub fn usb_serial() -> HString<20> {
    let mut out = HString::new();
    match provisioned_serial_timestamp() {
        Some(timestamp) => {
            let _ = write!(out, "{timestamp}");
        }
        None => {
            let mac = Efuse::read_base_mac_address();
            let _ = write!(
                out,
                "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            );
        }
    }
    out
}
