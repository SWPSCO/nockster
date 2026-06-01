#[cfg(feature = "chip-security")]
use esp_hal::efuse::{
    Efuse, DIS_DIRECT_BOOT, DIS_DOWNLOAD_MODE, DIS_PAD_JTAG, DIS_USB_JTAG,
    DIS_USB_OTG_DOWNLOAD_MODE, DIS_USB_SERIAL_JTAG, DIS_USB_SERIAL_JTAG_DOWNLOAD_MODE,
    DIS_USB_SERIAL_JTAG_ROM_PRINT, ENABLE_SECURITY_DOWNLOAD, KEY_PURPOSE_0, KEY_PURPOSE_1,
    KEY_PURPOSE_2, KEY_PURPOSE_3, KEY_PURPOSE_4, KEY_PURPOSE_5, POWERGLITCH_EN, RD_DIS,
    SECURE_BOOT_EN, SECURE_VERSION, SOFT_DIS_JTAG, SPI_BOOT_CRYPT_CNT,
};
#[cfg(not(feature = "chip-security"))]
use nockster_core::SecurityStatus;
#[cfg(feature = "chip-security")]
use nockster_core::{
    SecurityStatus, HMAC_KEY_PURPOSE_DOWN_ALL, HMAC_KEY_PURPOSE_DOWN_DS,
    HMAC_KEY_PURPOSE_DOWN_JTAG, HMAC_KEY_PURPOSE_UP,
};

use crate::nvs_store::NvsStore;

pub fn read_security_status(nvs: &mut NvsStore) -> SecurityStatus {
    let nvs_status = nvs.storage_status();

    #[cfg(not(feature = "chip-security"))]
    {
        return SecurityStatus {
            chip_security_available: false,
            mac: [0; 6],
            flash_encryption: false,
            flash_crypt_cnt: 0,
            secure_boot: false,
            secure_version: 0,
            key_purposes: [0; 6],
            hmac_key_slots: 0,
            hmac_user_key_slots: 0,
            read_protected_key_slots: 0,
            pad_jtag_disabled: false,
            usb_jtag_disabled: false,
            soft_jtag_disabled: false,
            soft_jtag_disable_bits: 0,
            usb_serial_jtag_disabled: false,
            download_mode_disabled: false,
            usb_serial_jtag_download_disabled: false,
            usb_otg_download_disabled: false,
            secure_download_enabled: false,
            direct_boot_disabled: false,
            usb_rom_print_disabled: false,
            power_glitch_enabled: false,
            nvs_initialized: nvs_status.initialized,
            nvs_schema_version: nvs_status.schema_version,
            nvs_slot_count: nvs_status.slot_count,
        };
    }

    #[cfg(feature = "chip-security")]
    {
        let key_purposes = [
            Efuse::read_field_le::<u8>(KEY_PURPOSE_0),
            Efuse::read_field_le::<u8>(KEY_PURPOSE_1),
            Efuse::read_field_le::<u8>(KEY_PURPOSE_2),
            Efuse::read_field_le::<u8>(KEY_PURPOSE_3),
            Efuse::read_field_le::<u8>(KEY_PURPOSE_4),
            Efuse::read_field_le::<u8>(KEY_PURPOSE_5),
        ];
        let soft_jtag_disable_bits = Efuse::read_field_le::<u8>(SOFT_DIS_JTAG);
        let flash_crypt_cnt = Efuse::read_field_le::<u8>(SPI_BOOT_CRYPT_CNT);

        SecurityStatus {
            chip_security_available: true,
            mac: Efuse::read_base_mac_address(),
            flash_encryption: Efuse::flash_encryption(),
            flash_crypt_cnt,
            secure_boot: Efuse::read_bit(SECURE_BOOT_EN),
            secure_version: Efuse::read_field_le::<u16>(SECURE_VERSION),
            key_purposes,
            hmac_key_slots: key_slot_mask(&key_purposes, is_hmac_key_purpose),
            hmac_user_key_slots: key_slot_mask(&key_purposes, |purpose| {
                purpose == HMAC_KEY_PURPOSE_UP
            }),
            read_protected_key_slots: Efuse::read_field_le::<u8>(RD_DIS) & 0x3f,
            pad_jtag_disabled: Efuse::read_bit(DIS_PAD_JTAG),
            usb_jtag_disabled: Efuse::read_bit(DIS_USB_JTAG),
            soft_jtag_disabled: soft_jtag_disable_bits.count_ones() % 2 == 1,
            soft_jtag_disable_bits,
            usb_serial_jtag_disabled: Efuse::read_bit(DIS_USB_SERIAL_JTAG),
            download_mode_disabled: Efuse::read_bit(DIS_DOWNLOAD_MODE),
            usb_serial_jtag_download_disabled: Efuse::read_bit(DIS_USB_SERIAL_JTAG_DOWNLOAD_MODE),
            usb_otg_download_disabled: Efuse::read_bit(DIS_USB_OTG_DOWNLOAD_MODE),
            secure_download_enabled: Efuse::read_bit(ENABLE_SECURITY_DOWNLOAD),
            direct_boot_disabled: Efuse::read_bit(DIS_DIRECT_BOOT),
            usb_rom_print_disabled: Efuse::read_bit(DIS_USB_SERIAL_JTAG_ROM_PRINT),
            power_glitch_enabled: Efuse::read_bit(POWERGLITCH_EN),
            nvs_initialized: nvs_status.initialized,
            nvs_schema_version: nvs_status.schema_version,
            nvs_slot_count: nvs_status.slot_count,
        }
    }
}

#[cfg(feature = "chip-security")]
fn key_slot_mask(key_purposes: &[u8; 6], predicate: impl Fn(u8) -> bool) -> u8 {
    let mut mask = 0u8;
    for (idx, purpose) in key_purposes.iter().copied().enumerate() {
        if predicate(purpose) {
            mask |= 1 << idx;
        }
    }
    mask
}

#[cfg(feature = "chip-security")]
fn is_hmac_key_purpose(purpose: u8) -> bool {
    matches!(
        purpose,
        HMAC_KEY_PURPOSE_DOWN_ALL
            | HMAC_KEY_PURPOSE_DOWN_JTAG
            | HMAC_KEY_PURPOSE_DOWN_DS
            | HMAC_KEY_PURPOSE_UP
    )
}
