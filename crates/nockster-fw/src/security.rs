#[cfg(feature = "chip-security")]
use esp_hal::efuse::{
    Efuse, DIS_DIRECT_BOOT, DIS_DOWNLOAD_MODE, DIS_PAD_JTAG, DIS_USB_JTAG,
    DIS_USB_OTG_DOWNLOAD_MODE, DIS_USB_SERIAL_JTAG, DIS_USB_SERIAL_JTAG_DOWNLOAD_MODE,
    DIS_USB_SERIAL_JTAG_ROM_PRINT, ENABLE_SECURITY_DOWNLOAD, KEY_PURPOSE_0, KEY_PURPOSE_1,
    KEY_PURPOSE_2, KEY_PURPOSE_3, KEY_PURPOSE_4, KEY_PURPOSE_5, POWERGLITCH_EN, RD_DIS,
    SECURE_BOOT_EN, SECURE_VERSION, SOFT_DIS_JTAG, SPI_BOOT_CRYPT_CNT, WR_DIS,
};
#[cfg(feature = "chip-security")]
use esp_hal::peripherals::EFUSE;
#[cfg(not(feature = "chip-security"))]
use nockster_core::SecurityStatus;
#[cfg(feature = "chip-security")]
use nockster_core::{
    SecurityStatus, HMAC_KEY_PURPOSE_DOWN_ALL, HMAC_KEY_PURPOSE_DOWN_DS,
    HMAC_KEY_PURPOSE_DOWN_JTAG, HMAC_KEY_PURPOSE_UP,
};

use crate::nvs_store::NvsStore;
#[cfg(feature = "chip-security")]
use zeroize::Zeroize;

#[cfg(feature = "chip-security")]
const WR_DIS_RD_DIS: u32 = 1 << 0;
#[cfg(feature = "chip-security")]
const WR_DIS_JTAG_HARD: u32 = 1 << 2;
#[cfg(feature = "chip-security")]
const WR_DIS_POWER_GLITCH: u32 = 1 << 17;
#[cfg(feature = "chip-security")]
const WR_DIS_DOWNLOAD: u32 = 1 << 18;
#[cfg(feature = "chip-security")]
const WR_DIS_USB_OTG_DOWNLOAD: u32 = 1 << 19;
#[cfg(feature = "chip-security")]
const WR_DIS_JTAG_SOFT: u32 = 1 << 31;
#[cfg(feature = "chip-security")]
const WR_DIS_KEY_PURPOSE_BASE: u32 = 8;
#[cfg(feature = "chip-security")]
const WR_DIS_KEY_BLOCK_BASE: u32 = 23;

#[cfg(feature = "chip-security")]
const EFUSE_BLOCK_KEY0: u32 = 4;
#[cfg(feature = "chip-security")]
const EFUSE_BLOCK_KEY5: u32 = 9;

// BLOCK0 programming-register masks. BLOCK0 bit N is programmed through
// PGM_DATA[N / 32] bit N % 32. Keep these in one place so the irreversible
// write below can be reviewed directly against the ESP32-S3 eFuse table.
#[cfg(feature = "chip-security")]
const PRODUCTION_DATA0_ALL: u32 =
    (0x3f << WR_DIS_KEY_PURPOSE_BASE) | (0x3f << WR_DIS_KEY_BLOCK_BASE);
#[cfg(feature = "chip-security")]
const PRODUCTION_DATA1_ALL: u32 = 0x3f | (0x7 << 16) | (1 << 19);
#[cfg(feature = "chip-security")]
const LOCKDOWN_DATA3_ALL: u32 = (1 << 22) | (1 << 23);
#[cfg(feature = "chip-security")]
const LOCKDOWN_DATA4_ALL: u32 = (1 << 0) | (1 << 1) | (1 << 2) | (1 << 4) | (1 << 30) | (1 << 31);

#[cfg(feature = "chip-security")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaLockdownError {
    MissingSecureBoot,
    MissingFlashEncryption,
    MissingHmacForInitializedNvs,
    NoUnusedHmacKeyBlock,
    RandomFailed,
    HmacKeyProgramFailed,
    HmacKeyPurposeInvalid,
    WriteProtected,
    ProgramFailed,
    ReadFailed,
    VerifyFailed,
}

#[cfg(feature = "chip-security")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtaLockdownOutcome {
    NotProductionBuild,
    NotRunningFromOta,
    AlreadyLocked,
    Locked,
}

#[cfg(feature = "chip-security")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LockdownProgramWords {
    data0: u32,
    data1: u32,
    data3: u32,
    data4: u32,
}

#[cfg(feature = "chip-security")]
impl LockdownProgramWords {
    fn is_empty(self) -> bool {
        self.data0 == 0 && self.data1 == 0 && self.data3 == 0 && self.data4 == 0
    }
}

/// Enforce the irreversible production lockdown when a production image is
/// running from an OTA app partition.
///
/// Factory/debug images never write lockdown eFuses. OTA production images
/// validate secure boot and flash encryption, provision a missing HMAC_UP key,
/// preflight every relevant write-protection group, burn only missing bits,
/// and verify them by readback.
#[cfg(feature = "chip-security")]
pub fn enforce_ota_production_lockdown(
    running_from_ota: bool,
    wallet_initialized: bool,
) -> Result<OtaLockdownOutcome, OtaLockdownError> {
    if option_env!("NOCKSTER_BUILD_PROFILE") != Some("production") {
        return Ok(OtaLockdownOutcome::NotProductionBuild);
    }
    if !running_from_ota {
        return Ok(OtaLockdownOutcome::NotRunningFromOta);
    }
    if !Efuse::read_bit(SECURE_BOOT_EN) {
        return Err(OtaLockdownError::MissingSecureBoot);
    }
    if !Efuse::flash_encryption() {
        return Err(OtaLockdownError::MissingFlashEncryption);
    }

    let (hmac_slot, hmac_provisioned) = ensure_hmac_up_key(wallet_initialized)?;
    let words = missing_lockdown_words(hmac_slot);
    if words.is_empty() {
        return Ok(if hmac_provisioned {
            OtaLockdownOutcome::Locked
        } else {
            OtaLockdownOutcome::AlreadyLocked
        });
    }
    ensure_missing_fields_are_writable(words)?;

    // The TRM directs callers to retry BLOCK0 programming when readback does
    // not contain all requested bits. Reprogramming an already-set eFuse bit
    // is safe, and each attempt recomputes the remaining mask.
    for _ in 0..3 {
        let remaining = missing_lockdown_words(hmac_slot);
        if remaining.is_empty() {
            return Ok(OtaLockdownOutcome::Locked);
        }
        program_block0(remaining)?;
    }

    if missing_lockdown_words(hmac_slot).is_empty() {
        Ok(OtaLockdownOutcome::Locked)
    } else {
        Err(OtaLockdownError::VerifyFailed)
    }
}

#[cfg(feature = "chip-security")]
fn ensure_hmac_up_key(wallet_initialized: bool) -> Result<(u32, bool), OtaLockdownError> {
    if let Some(slot) = hmac_up_slot() {
        return Ok((slot, false));
    }
    // A newly generated device secret cannot decrypt storage that was bound to
    // a different key. Only self-provision before the wallet has been created.
    if wallet_initialized {
        return Err(OtaLockdownError::MissingHmacForInitializedNvs);
    }
    // Do not create a software-readable HMAC key if its read-protection bit can
    // no longer be programmed.
    if Efuse::read_field_le::<u32>(WR_DIS) & WR_DIS_RD_DIS != 0 {
        return Err(OtaLockdownError::WriteProtected);
    }

    unsafe extern "C" {
        fn ets_efuse_key_block_unused(key_block: u32) -> bool;
        fn ets_efuse_write_key(
            key_block: u32,
            purpose: u32,
            data: *const core::ffi::c_void,
            data_len: usize,
        ) -> i32;
        fn ets_efuse_read() -> i32;
    }

    // Preserve the factory layout by preferring KEY5, but safely fall back to
    // any of the six genuinely unused key blocks if KEY5 is already occupied.
    let key_block = (EFUSE_BLOCK_KEY0..=EFUSE_BLOCK_KEY5)
        .rev()
        .find(|block| unsafe { ets_efuse_key_block_unused(*block) })
        .ok_or(OtaLockdownError::NoUnusedHmacKeyBlock)?;

    let mut key = [0u8; 32];
    if getrandom::getrandom(&mut key).is_err() || key.iter().all(|byte| *byte == 0) {
        key.zeroize();
        return Err(OtaLockdownError::RandomFailed);
    }

    let program_result = critical_section::with(|_| {
        let result = unsafe {
            ets_efuse_write_key(
                key_block,
                HMAC_KEY_PURPOSE_UP as u32,
                key.as_ptr().cast(),
                key.len(),
            )
        };
        if result != 0 {
            return Err(OtaLockdownError::HmacKeyProgramFailed);
        }
        if unsafe { ets_efuse_read() } != 0 {
            return Err(OtaLockdownError::ReadFailed);
        }
        Ok(())
    });
    key.zeroize();
    program_result?;

    let slot = key_block - EFUSE_BLOCK_KEY0;
    if key_purposes()[slot as usize] != HMAC_KEY_PURPOSE_UP {
        return Err(OtaLockdownError::HmacKeyPurposeInvalid);
    }
    Ok((slot, true))
}

#[cfg(feature = "chip-security")]
fn hmac_up_slot() -> Option<u32> {
    key_purposes()
        .iter()
        .position(|purpose| *purpose == HMAC_KEY_PURPOSE_UP)
        .map(|slot| slot as u32)
}

#[cfg(feature = "chip-security")]
fn key_purposes() -> [u8; 6] {
    [
        Efuse::read_field_le::<u8>(KEY_PURPOSE_0),
        Efuse::read_field_le::<u8>(KEY_PURPOSE_1),
        Efuse::read_field_le::<u8>(KEY_PURPOSE_2),
        Efuse::read_field_le::<u8>(KEY_PURPOSE_3),
        Efuse::read_field_le::<u8>(KEY_PURPOSE_4),
        Efuse::read_field_le::<u8>(KEY_PURPOSE_5),
    ]
}

#[cfg(feature = "chip-security")]
fn missing_lockdown_words(hmac_slot: u32) -> LockdownProgramWords {
    let wr_dis = Efuse::read_field_le::<u32>(WR_DIS);
    let rd_dis = Efuse::read_field_le::<u8>(RD_DIS) as u32;
    let mut data0 = 0;
    for bit in [
        WR_DIS_KEY_PURPOSE_BASE + hmac_slot,
        WR_DIS_KEY_BLOCK_BASE + hmac_slot,
    ] {
        if wr_dis & (1 << bit) == 0 {
            data0 |= 1 << bit;
        }
    }

    let soft_jtag = Efuse::read_field_le::<u8>(SOFT_DIS_JTAG) as u32;
    let mut data1 = (0x7 ^ soft_jtag) << 16;
    if rd_dis & (1 << hmac_slot) == 0 {
        data1 |= 1 << hmac_slot;
    }
    if !Efuse::read_bit(DIS_PAD_JTAG) {
        data1 |= 1 << 19;
    }

    let mut data3 = 0;
    if !Efuse::read_bit(DIS_USB_JTAG) {
        data3 |= 1 << 22;
    }
    if !Efuse::read_bit(DIS_USB_SERIAL_JTAG) {
        data3 |= 1 << 23;
    }

    let mut data4 = 0;
    for (field_missing, bit) in [
        (!Efuse::read_bit(DIS_DOWNLOAD_MODE), 0),
        (!Efuse::read_bit(DIS_DIRECT_BOOT), 1),
        (!Efuse::read_bit(DIS_USB_SERIAL_JTAG_ROM_PRINT), 2),
        (!Efuse::read_bit(DIS_USB_SERIAL_JTAG_DOWNLOAD_MODE), 4),
        (!Efuse::read_bit(POWERGLITCH_EN), 30),
        (!Efuse::read_bit(DIS_USB_OTG_DOWNLOAD_MODE), 31),
    ] {
        if field_missing {
            data4 |= 1 << bit;
        }
    }

    LockdownProgramWords {
        data0: data0 & PRODUCTION_DATA0_ALL,
        data1: data1 & PRODUCTION_DATA1_ALL,
        data3: data3 & LOCKDOWN_DATA3_ALL,
        data4: data4 & LOCKDOWN_DATA4_ALL,
    }
}

#[cfg(feature = "chip-security")]
fn ensure_missing_fields_are_writable(words: LockdownProgramWords) -> Result<(), OtaLockdownError> {
    let wr_dis = Efuse::read_field_le::<u32>(WR_DIS);
    let hmac_read_protection_missing = words.data1 & 0x3f != 0;
    let hard_jtag_missing = words.data1 & (1 << 19) != 0 || words.data3 != 0;
    let soft_jtag_missing = words.data1 & (0x7 << 16) != 0;
    let download_missing = words.data4 & 0x1f != 0;
    let power_glitch_missing = words.data4 & (1 << 30) != 0;
    let usb_otg_download_missing = words.data4 & (1 << 31) != 0;

    if (hmac_read_protection_missing && wr_dis & WR_DIS_RD_DIS != 0)
        || (hard_jtag_missing && wr_dis & WR_DIS_JTAG_HARD != 0)
        || (soft_jtag_missing && wr_dis & WR_DIS_JTAG_SOFT != 0)
        || (download_missing && wr_dis & WR_DIS_DOWNLOAD != 0)
        || (power_glitch_missing && wr_dis & WR_DIS_POWER_GLITCH != 0)
        || (usb_otg_download_missing && wr_dis & WR_DIS_USB_OTG_DOWNLOAD != 0)
    {
        Err(OtaLockdownError::WriteProtected)
    } else {
        Ok(())
    }
}

#[cfg(feature = "chip-security")]
fn program_block0(words: LockdownProgramWords) -> Result<(), OtaLockdownError> {
    unsafe extern "C" {
        fn ets_efuse_clear_program_registers();
        fn ets_efuse_program(block: u32) -> i32;
        fn ets_efuse_read() -> i32;
    }

    critical_section::with(|_| {
        let efuse = EFUSE::regs();
        unsafe {
            ets_efuse_clear_program_registers();
            efuse.pgm_data0().write(|w| w.bits(words.data0));
            efuse.pgm_data1().write(|w| w.bits(words.data1));
            efuse.pgm_data3().write(|w| w.bits(words.data3));
            efuse.pgm_data4().write(|w| w.bits(words.data4));
        }

        let program_result = unsafe { ets_efuse_program(0) };
        unsafe { ets_efuse_clear_program_registers() };
        if program_result != 0 {
            return Err(OtaLockdownError::ProgramFailed);
        }
        if unsafe { ets_efuse_read() } != 0 {
            return Err(OtaLockdownError::ReadFailed);
        }
        Ok(())
    })
}

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
        let key_purposes = key_purposes();
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
