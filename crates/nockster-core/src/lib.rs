#![cfg_attr(not(test), no_std)]
extern crate alloc;

pub mod cheetah;
pub mod draft_sign;
pub mod math;
pub mod noun;
pub mod update;
// Re-export crypto types - use conditional compilation for std vs no_std differences
#[cfg(not(feature = "std"))]
pub use cheetah::{
    cheetah_pub_from_sk, master_from_seed, schnorr_sign_tx, xprv_derive_child, xpub_derive_child,
    Hash, XKey, T8,
};

#[cfg(feature = "std")]
pub use cheetah::{
    cheetah_pub_from_sk, master_from_seed, schnorr_sign_tx, xprv_derive_child, Hash, XKey, T8,
};

use alloc::vec::Vec;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

pub enum Curve {
    Secp256k1,
}

pub struct DerivedKey {
    pub pubkey: Vec<u8>,
    pub chain_code: [u8; 32],
}

pub trait Signer {
    fn master_fingerprint(&self) -> [u8; 4];
    fn derive_pubkey(&self, path: &[u32]) -> DerivedKey;
    fn sign_digest32(&self, path: &[u32], digest32: [u8; 32]) -> [u8; 64];
}

pub const PROTO_V1: u8 = 1;

// Error codes
pub const ERR_BAD_COBS_OR_POSTCARD: u16 = 100;
pub const ERR_OVERFLOW: u16 = 102;
pub const ERR_ENCODE_TOO_BIG: u16 = 103;
pub const ERR_UNSUPPORTED_VERSION: u16 = 110;
pub const ERR_NO_SEED: u16 = 120;
pub const ERR_WRONG_PUBKEY: u16 = 0x0103;
pub const ERR_DEVICE_LOCKED: u16 = 130;
pub const ERR_WRONG_PIN: u16 = 131;
pub const ERR_PIN_LOCKED_OUT: u16 = 132;
pub const ERR_ALREADY_INITIALIZED: u16 = 133;
pub const ERR_REJECTED_BY_USER: u16 = 134;
pub const ERR_BUSY: u16 = 135;
pub const ERR_PIN_MISMATCH: u16 = 136;
pub const ERR_FLASH: u16 = 140;
pub const ERR_CRYPTO: u16 = 141;

/// Short, human-readable name for a device error code. Single source of truth so
/// hosts don't drift from these numbers; safe in `no_std` (returns `&'static str`).
pub fn describe_error(code: u16) -> &'static str {
    match code {
        ERR_BAD_COBS_OR_POSTCARD => "malformed message framing (COBS/postcard)",
        ERR_OVERFLOW => "buffer overflow",
        ERR_ENCODE_TOO_BIG => "response too large to encode",
        ERR_UNSUPPORTED_VERSION => "unsupported request or protocol version",
        ERR_NO_SEED => "no seed available",
        ERR_WRONG_PUBKEY => "public key mismatch",
        ERR_DEVICE_LOCKED => "device locked",
        ERR_WRONG_PIN => "incorrect PIN",
        ERR_PIN_LOCKED_OUT => "PIN locked out",
        ERR_ALREADY_INITIALIZED => "device already initialized",
        ERR_REJECTED_BY_USER => "rejected on device",
        ERR_BUSY => "device busy",
        ERR_PIN_MISMATCH => "PIN confirmation mismatch",
        ERR_FLASH => "device flash/NVS error",
        ERR_CRYPTO => "device crypto/RNG error",
        _ => "unknown error",
    }
}

pub const FEATURE_CHEETAH: u32 = 1 << 0;
pub const FEATURE_FRAG: u32 = 1 << 1;
pub const FEATURE_XPUB: u32 = 1 << 2;
pub const FEATURE_SECURITY_STATUS: u32 = 1 << 3;
pub const FEATURE_BUILD_INFO: u32 = 1 << 4;
pub const FEATURE_TOUCH_CALIBRATION: u32 = 1 << 5;
pub const FEATURE_TOUCH_DIAGNOSTICS: u32 = 1 << 6;
pub const FEATURE_SEED_LABELS: u32 = 1 << 7;
pub const FEATURE_PIN_CHANGE_UI: u32 = 1 << 8;
pub const FEATURE_TOUCH_CALIBRATION_UI: u32 = 1 << 9;
pub const FEATURE_SECURE_UPDATE: u32 = 1 << 10;
pub const FEATURE_RELEASE_INFO: u32 = 1 << 11;
pub const FEATURE_UPDATE_BOOT_STATUS: u32 = 1 << 12;
pub const FEATURE_DEVICE_REBOOT: u32 = 1 << 13;
pub const FEATURE_DEVICE_ADDRESS_BOOK: u32 = 1 << 14;
pub const FEATURE_ALL_KNOWN: u32 = FEATURE_CHEETAH
    | FEATURE_FRAG
    | FEATURE_XPUB
    | FEATURE_SECURITY_STATUS
    | FEATURE_BUILD_INFO
    | FEATURE_TOUCH_CALIBRATION
    | FEATURE_TOUCH_DIAGNOSTICS
    | FEATURE_SEED_LABELS
    | FEATURE_PIN_CHANGE_UI
    | FEATURE_TOUCH_CALIBRATION_UI
    | FEATURE_SECURE_UPDATE
    | FEATURE_RELEASE_INFO
    | FEATURE_UPDATE_BOOT_STATUS
    | FEATURE_DEVICE_REBOOT
    | FEATURE_DEVICE_ADDRESS_BOOK;

pub const HMAC_KEY_PURPOSE_DOWN_ALL: u8 = 5;
pub const HMAC_KEY_PURPOSE_DOWN_JTAG: u8 = 6;
pub const HMAC_KEY_PURPOSE_DOWN_DS: u8 = 7;
pub const HMAC_KEY_PURPOSE_UP: u8 = 8;

pub const UPDATE_SLOT_NONE: u8 = 0;
pub const UPDATE_SLOT_OTA0: u8 = 1;
pub const UPDATE_SLOT_OTA1: u8 = 2;
pub const UPDATE_SLOT_UNKNOWN: u8 = 0xff;

pub const UPDATE_OTA_STATE_NEW: u8 = 0;
pub const UPDATE_OTA_STATE_PENDING_VERIFY: u8 = 1;
pub const UPDATE_OTA_STATE_VALID: u8 = 2;
pub const UPDATE_OTA_STATE_INVALID: u8 = 3;
pub const UPDATE_OTA_STATE_ABORTED: u8 = 4;
pub const UPDATE_OTA_STATE_UNAVAILABLE: u8 = 0xfd;
pub const UPDATE_OTA_STATE_UNKNOWN: u8 = 0xfe;
pub const UPDATE_OTA_STATE_UNDEFINED: u8 = 0xff;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Msg<T> {
    pub v: u8,
    pub id: u32,
    pub msg: T,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Caps {
    pub proto_v: u8,
    pub compressed_pk: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Xpub {
    pub depth: u8,
    pub fp4: [u8; 4],
    pub child: u32,
    #[serde(with = "BigArray")]
    pub chain_code: [u8; 32],
    #[serde(with = "BigArray")]
    pub pubkey33: [u8; 33],
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SpendOutputMeta {
    pub gift: u64,
    pub recipient_pkh_b58: alloc::string::String,
    #[serde(default)]
    pub is_refund: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SpendMeta {
    pub outputs: alloc::vec::Vec<SpendOutputMeta>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SecurityStatus {
    pub chip_security_available: bool,
    pub mac: [u8; 6],
    pub flash_encryption: bool,
    pub flash_crypt_cnt: u8,
    pub secure_boot: bool,
    pub secure_version: u16,
    pub key_purposes: [u8; 6],
    pub hmac_key_slots: u8,
    pub hmac_user_key_slots: u8,
    pub read_protected_key_slots: u8,
    pub pad_jtag_disabled: bool,
    pub usb_jtag_disabled: bool,
    pub soft_jtag_disabled: bool,
    pub soft_jtag_disable_bits: u8,
    pub usb_serial_jtag_disabled: bool,
    pub download_mode_disabled: bool,
    pub usb_serial_jtag_download_disabled: bool,
    pub usb_otg_download_disabled: bool,
    pub secure_download_enabled: bool,
    pub direct_boot_disabled: bool,
    pub usb_rom_print_disabled: bool,
    pub power_glitch_enabled: bool,
    pub nvs_initialized: bool,
    pub nvs_schema_version: u8,
    pub nvs_slot_count: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BuildInfo {
    pub git_commit: heapless::String<40>,
    pub git_dirty: bool,
    pub build_profile: heapless::String<16>,
    pub protocol_v: u8,
    pub tx_types_rev: heapless::String<40>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub release_version: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TouchCalibration {
    pub raw_x_min: u16,
    pub raw_x_max: u16,
    pub raw_y_min: u16,
    pub raw_y_max: u16,
    pub mirror_x: bool,
    pub mirror_y: bool,
}

pub const MAX_SEED_LABEL_LEN: usize = 32;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SeedSlotLabel {
    pub slot: u8,
    pub label: heapless::String<MAX_SEED_LABEL_LEN>,
}

pub const MAX_DEVICE_ADDRESS_BOOK_ENTRIES: usize = 50;
pub const MAX_ADDRESS_BOOK_LABEL_LEN: usize = 32;
pub const MAX_ADDRESS_BOOK_PKH_LEN: usize = 64;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct DeviceAddressBookEntry {
    pub label: heapless::String<MAX_ADDRESS_BOOK_LABEL_LEN>,
    pub pkh: heapless::String<MAX_ADDRESS_BOOK_PKH_LEN>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct UpdateTrust {
    pub configured: bool,
    pub pubkey_sha256: [u8; 32],
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateStatus {
    pub active: bool,
    pub manifest_verified: bool,
    pub image_verified: bool,
    pub release_version: u32,
    pub bytes_received: u32,
    pub image_size: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateBootStatus {
    pub partition_table_ok: bool,
    pub ota_data_present: bool,
    pub ota0_present: bool,
    pub ota1_present: bool,
    pub current_slot: u8,
    pub next_slot: u8,
    pub ota_state: u8,
    pub ota0_offset: u32,
    pub ota0_size: u32,
    pub ota1_offset: u32,
    pub ota1_size: u32,
}

// nockster-core
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragKind {
    SetSeed,
    SignDraft,
    AddressBook,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Frame {
    One(Request),
    FragBegin {
        id: u16,
        total_len: u32,
        kind: FragKind,
    },
    FragPart {
        id: u16,
        offset: u32,
        chunk: alloc::vec::Vec<u8>,
        last: bool,
    },
}

// augment Request/Response just a touch
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Request {
    Hello,
    GetInfo,
    Ping,
    Wipe,
    SetSeed {
        #[serde(with = "BigArray")]
        seed64: [u8; 64],
    },
    GetFingerprint,
    GetPubkey {
        path: alloc_path::Path,
        #[serde(default)]
        compressed: bool,
    },
    GetXpub {
        path: alloc_path::Path,
    },
    SignDigest {
        path: alloc_path::Path,
        digest32: [u8; 32],
    },

    // Cheetah
    GetCheetahPub {
        slot: u8,
        path: alloc_path::Path,
    },
    SignSpendHash {
        slot: u8,
        path: alloc_path::Path,
        msg5: [u64; 5],
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        meta: Option<SpendMeta>,
    },
    SignSpendHashFor {
        slot: u8,
        path: alloc_path::Path,
        msg5: [u64; 5],
        pubkey: ([u64; 6], [u64; 6]),
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        meta: Option<SpendMeta>,
    },

    // self-test
    Health,

    // PIN and persistence
    InitializePIN {
        pin: alloc::string::String,
        #[serde(with = "BigArray")]
        seed64: [u8; 64],
    },
    AddSeed {
        #[serde(with = "BigArray")]
        seed64: [u8; 64],
    },
    DeleteSeed {
        slot: u8,
    },
    Unlock {
        pin: alloc::string::String,
    },
    Lock,
    ResetPIN {
        current_pin: alloc::string::String,
        new_pin: alloc::string::String,
    },
    GetLockStatus,
    SelectSeed {
        slot: u8,
    },
    Reset,
    GetSecurityStatus,
    GetBuildInfo,
    GetTouchCalibration,
    SetTouchCalibration {
        calibration: TouchCalibration,
    },
    ShowTouchDiagnostics {
        enabled: bool,
    },
    GetSeedLabels,
    SetSeedLabel {
        slot: u8,
        label: heapless::String<MAX_SEED_LABEL_LEN>,
    },
    ChangePinOnDevice {
        current_pin: alloc::string::String,
    },
    StartTouchCalibration,
    GetUpdateTrust,
    VerifyUpdateManifest {
        manifest: update::UpdateManifest,
        #[serde(with = "BigArray")]
        signature64: [u8; 64],
        signing_pubkey_sec1: alloc::vec::Vec<u8>,
    },
    BeginUpdate {
        manifest: update::UpdateManifest,
        #[serde(with = "BigArray")]
        signature64: [u8; 64],
        signing_pubkey_sec1: alloc::vec::Vec<u8>,
        write_flash: bool,
    },
    UpdateChunk {
        offset: u32,
        chunk: alloc::vec::Vec<u8>,
    },
    FinishUpdate,
    CancelUpdate,
    GetUpdateStatus,
    GetReleaseInfo,
    GetUpdateBootStatus,
    Reboot,
    GetAddressBook,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Response {
    Hello(Caps),
    FragBegin {
        id: u16,
        total_len: u32,
        kind: FragKind,
    },
    FragPart {
        id: u16,
        offset: u32,
        chunk: alloc::vec::Vec<u8>,
        last: bool,
    },
    Info {
        proto_v: u8,
        fw_major: u16,
        fw_minor: u16,
        features: u32,
        has_seed: bool,
        cheetah_pubs: alloc::vec::Vec<CheetahPub>,
    },
    Pong,
    Ok,
    OkSig {
        #[serde(with = "BigArray")]
        sig64: [u8; 64],
    },
    OkFingerprint {
        #[serde(with = "BigArray")]
        fp4: [u8; 4],
    },
    OkPubkey {
        #[serde(with = "BigArray")]
        uncompressed: [u8; 65],
    },
    OkPubkeyCompressed {
        #[serde(with = "BigArray")]
        compressed: [u8; 33],
    },
    OkXpub(Xpub),
    OkCheetahPub {
        x: [u64; 6],
        y: [u64; 6],
    },
    OkCheetahSig {
        chal: [u64; 8],
        sig: [u64; 8],
    },
    OkLockStatus {
        locked: bool,
        attempts_remaining: u8,
    },
    Err {
        code: u16,
    },
    OkSecurityStatus(SecurityStatus),
    OkBuildInfo(BuildInfo),
    OkTouchCalibration(TouchCalibration),
    OkSeedLabels(alloc::vec::Vec<SeedSlotLabel>),
    OkUpdateTrust(UpdateTrust),
    OkUpdateStatus(UpdateStatus),
    OkReleaseInfo(ReleaseInfo),
    OkUpdateBootStatus(UpdateBootStatus),
    OkAddressBook(alloc::vec::Vec<DeviceAddressBookEntry>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CheetahPub {
    pub slot: u8,
    pub path: alloc_path::Path,
    #[serde(with = "BigArray")]
    pub x: [u64; 6],
    #[serde(with = "BigArray")]
    pub y: [u64; 6],
}

pub const MAX_SEED_SLOTS: usize = 16;
pub const MAX_INFO_CHEETAH_PUBS: usize = MAX_SEED_SLOTS;

// Host uses Vec<u32>, firmware uses HVec<u32,16>
pub mod alloc_path {
    #[cfg(feature = "alloc-path")]
    pub type Path = alloc::vec::Vec<u32>;

    #[cfg(not(feature = "alloc-path"))]
    pub use heapless::Vec as HVec;
    #[cfg(not(feature = "alloc-path"))]
    pub type Path = HVec<u32, 16>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_security_variants_are_append_only() {
        let mut buf = [0u8; 128];

        let reset = postcard::to_slice(&Request::Reset, &mut buf).unwrap();
        assert_eq!(reset, &[21]);

        let security = postcard::to_slice(&Request::GetSecurityStatus, &mut buf).unwrap();
        assert_eq!(security, &[22]);

        let build = postcard::to_slice(&Request::GetBuildInfo, &mut buf).unwrap();
        assert_eq!(build, &[23]);

        let touch = postcard::to_slice(&Request::GetTouchCalibration, &mut buf).unwrap();
        assert_eq!(touch, &[24]);

        let calibration = TouchCalibration {
            raw_x_min: 1,
            raw_x_max: 2,
            raw_y_min: 3,
            raw_y_max: 4,
            mirror_x: true,
            mirror_y: false,
        };
        let set_touch =
            postcard::to_slice(&Request::SetTouchCalibration { calibration }, &mut buf).unwrap();
        assert_eq!(set_touch[0], 25);

        let show_touch_diagnostics =
            postcard::to_slice(&Request::ShowTouchDiagnostics { enabled: true }, &mut buf).unwrap();
        assert_eq!(show_touch_diagnostics, &[26, 1]);

        let get_seed_labels = postcard::to_slice(&Request::GetSeedLabels, &mut buf).unwrap();
        assert_eq!(get_seed_labels, &[27]);

        let mut label = heapless::String::<MAX_SEED_LABEL_LEN>::new();
        label.push_str("primary").unwrap();
        let set_seed_label =
            postcard::to_slice(&Request::SetSeedLabel { slot: 2, label }, &mut buf).unwrap();
        assert_eq!(set_seed_label[0], 28);

        let change_pin = postcard::to_slice(
            &Request::ChangePinOnDevice {
                current_pin: "0208".to_string(),
            },
            &mut buf,
        )
        .unwrap();
        assert_eq!(change_pin[0], 29);

        let start_touch_calibration =
            postcard::to_slice(&Request::StartTouchCalibration, &mut buf).unwrap();
        assert_eq!(start_touch_calibration, &[30]);

        let update_trust = postcard::to_slice(&Request::GetUpdateTrust, &mut buf).unwrap();
        assert_eq!(update_trust, &[31]);

        let manifest = update::UpdateManifest::new(
            1,
            3,
            [1; 32],
            [2; 32],
            "esp32s3-touch-lcd-1.47",
            "production",
            PROTO_V1,
            "abc123",
            "def456",
        )
        .unwrap();
        let mut update_buf = [0u8; 512];
        let verify_update = postcard::to_slice(
            &Request::VerifyUpdateManifest {
                manifest: manifest.clone(),
                signature64: [3; 64],
                signing_pubkey_sec1: alloc::vec![2; 33],
            },
            &mut update_buf,
        )
        .unwrap();
        assert_eq!(verify_update[0], 32);

        let begin_update = postcard::to_slice(
            &Request::BeginUpdate {
                manifest,
                signature64: [3; 64],
                signing_pubkey_sec1: alloc::vec![2; 33],
                write_flash: false,
            },
            &mut update_buf,
        )
        .unwrap();
        assert_eq!(begin_update[0], 33);

        let update_chunk = postcard::to_slice(
            &Request::UpdateChunk {
                offset: 0,
                chunk: alloc::vec![1, 2, 3],
            },
            &mut update_buf,
        )
        .unwrap();
        assert_eq!(update_chunk[0], 34);

        let finish_update = postcard::to_slice(&Request::FinishUpdate, &mut update_buf).unwrap();
        assert_eq!(finish_update, &[35]);

        let cancel_update = postcard::to_slice(&Request::CancelUpdate, &mut update_buf).unwrap();
        assert_eq!(cancel_update, &[36]);

        let update_status = postcard::to_slice(&Request::GetUpdateStatus, &mut update_buf).unwrap();
        assert_eq!(update_status, &[37]);

        let release_info = postcard::to_slice(&Request::GetReleaseInfo, &mut update_buf).unwrap();
        assert_eq!(release_info, &[38]);

        let update_boot_status =
            postcard::to_slice(&Request::GetUpdateBootStatus, &mut update_buf).unwrap();
        assert_eq!(update_boot_status, &[39]);

        let reboot = postcard::to_slice(&Request::Reboot, &mut update_buf).unwrap();
        assert_eq!(reboot, &[40]);

        let address_book = postcard::to_slice(&Request::GetAddressBook, &mut update_buf).unwrap();
        assert_eq!(address_book, &[41]);

        let err = postcard::to_slice(&Response::Err { code: 0x1234 }, &mut buf).unwrap();
        assert_eq!(err[0], 14);

        let status = SecurityStatus {
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
            nvs_initialized: false,
            nvs_schema_version: 0,
            nvs_slot_count: 0,
        };
        let security_response =
            postcard::to_slice(&Response::OkSecurityStatus(status), &mut buf).unwrap();
        assert_eq!(security_response[0], 15);

        let mut git_commit = heapless::String::<40>::new();
        git_commit.push_str("abc123").unwrap();
        let mut build_profile = heapless::String::<16>::new();
        build_profile.push_str("dev").unwrap();
        let mut tx_types_rev = heapless::String::<40>::new();
        tx_types_rev.push_str("def456").unwrap();
        let build_response = postcard::to_slice(
            &Response::OkBuildInfo(BuildInfo {
                git_commit,
                git_dirty: false,
                build_profile,
                protocol_v: PROTO_V1,
                tx_types_rev,
            }),
            &mut buf,
        )
        .unwrap();
        assert_eq!(build_response[0], 16);

        let touch_response =
            postcard::to_slice(&Response::OkTouchCalibration(calibration), &mut buf).unwrap();
        assert_eq!(touch_response[0], 17);

        let labels_response =
            postcard::to_slice(&Response::OkSeedLabels(alloc::vec::Vec::new()), &mut buf).unwrap();
        assert_eq!(labels_response[0], 18);

        let update_trust_response = postcard::to_slice(
            &Response::OkUpdateTrust(UpdateTrust {
                configured: true,
                pubkey_sha256: [1; 32],
            }),
            &mut buf,
        )
        .unwrap();
        assert_eq!(update_trust_response[0], 19);

        let update_status_response = postcard::to_slice(
            &Response::OkUpdateStatus(UpdateStatus {
                active: true,
                manifest_verified: true,
                image_verified: false,
                release_version: 7,
                bytes_received: 3,
                image_size: 5,
            }),
            &mut buf,
        )
        .unwrap();
        assert_eq!(update_status_response[0], 20);

        let release_info_response = postcard::to_slice(
            &Response::OkReleaseInfo(ReleaseInfo { release_version: 7 }),
            &mut buf,
        )
        .unwrap();
        assert_eq!(release_info_response[0], 21);

        let update_boot_status_response = postcard::to_slice(
            &Response::OkUpdateBootStatus(UpdateBootStatus {
                partition_table_ok: true,
                ota_data_present: true,
                ota0_present: true,
                ota1_present: true,
                current_slot: UPDATE_SLOT_OTA0,
                next_slot: UPDATE_SLOT_OTA1,
                ota_state: UPDATE_OTA_STATE_VALID,
                ota0_offset: 0x310000,
                ota0_size: 0x300000,
                ota1_offset: 0x610000,
                ota1_size: 0x300000,
            }),
            &mut buf,
        )
        .unwrap();
        assert_eq!(update_boot_status_response[0], 22);

        let address_book_response =
            postcard::to_slice(&Response::OkAddressBook(alloc::vec::Vec::new()), &mut buf).unwrap();
        assert_eq!(address_book_response[0], 23);
    }

    #[test]
    fn protocol_postcard_fixtures_remain_decodable() {
        let mut buf = [0u8; 64];

        let ping_msg_fixture = [PROTO_V1, 42, 0, 2];
        let ping_msg: Msg<Frame> = postcard::from_bytes(&ping_msg_fixture).unwrap();
        assert_eq!(ping_msg.v, PROTO_V1);
        assert_eq!(ping_msg.id, 42);
        assert!(matches!(ping_msg.msg, Frame::One(Request::Ping)));
        assert_eq!(
            postcard::to_slice(&ping_msg, &mut buf).unwrap(),
            ping_msg_fixture
        );

        let release_request_fixture = [38];
        let release_request: Request = postcard::from_bytes(&release_request_fixture).unwrap();
        assert!(matches!(release_request, Request::GetReleaseInfo));
        assert_eq!(
            postcard::to_slice(&release_request, &mut buf).unwrap(),
            release_request_fixture
        );

        let update_boot_request_fixture = [39];
        let update_boot_request: Request =
            postcard::from_bytes(&update_boot_request_fixture).unwrap();
        assert!(matches!(update_boot_request, Request::GetUpdateBootStatus));
        assert_eq!(
            postcard::to_slice(&update_boot_request, &mut buf).unwrap(),
            update_boot_request_fixture
        );

        let reboot_request_fixture = [40];
        let reboot_request: Request = postcard::from_bytes(&reboot_request_fixture).unwrap();
        assert!(matches!(reboot_request, Request::Reboot));
        assert_eq!(
            postcard::to_slice(&reboot_request, &mut buf).unwrap(),
            reboot_request_fixture
        );

        let update_status_response_fixture = [20, 1, 1, 0, 7, 3, 5];
        let update_status_response: Response =
            postcard::from_bytes(&update_status_response_fixture).unwrap();
        match &update_status_response {
            Response::OkUpdateStatus(status) => {
                assert!(status.active);
                assert!(status.manifest_verified);
                assert!(!status.image_verified);
                assert_eq!(status.release_version, 7);
                assert_eq!(status.bytes_received, 3);
                assert_eq!(status.image_size, 5);
            }
            other => panic!("unexpected response fixture: {other:?}"),
        }
        assert_eq!(
            postcard::to_slice(&update_status_response, &mut buf).unwrap(),
            update_status_response_fixture
        );

        let release_response_fixture = [21, 7];
        let release_response: Response = postcard::from_bytes(&release_response_fixture).unwrap();
        match &release_response {
            Response::OkReleaseInfo(release) => assert_eq!(release.release_version, 7),
            other => panic!("unexpected release fixture: {other:?}"),
        }
        assert_eq!(
            postcard::to_slice(&release_response, &mut buf).unwrap(),
            release_response_fixture
        );

        let update_boot_response_fixture = [
            22, 1, 1, 1, 1, 1, 2, 2, 128, 128, 196, 1, 128, 128, 192, 1, 128, 128, 132, 3, 128,
            128, 192, 1,
        ];
        let update_boot_response: Response =
            postcard::from_bytes(&update_boot_response_fixture).unwrap();
        match &update_boot_response {
            Response::OkUpdateBootStatus(status) => {
                assert!(status.partition_table_ok);
                assert!(status.ota_data_present);
                assert!(status.ota0_present);
                assert!(status.ota1_present);
                assert_eq!(status.current_slot, UPDATE_SLOT_OTA0);
                assert_eq!(status.next_slot, UPDATE_SLOT_OTA1);
                assert_eq!(status.ota_state, UPDATE_OTA_STATE_VALID);
                assert_eq!(status.ota0_offset, 0x310000);
                assert_eq!(status.ota0_size, 0x300000);
                assert_eq!(status.ota1_offset, 0x610000);
                assert_eq!(status.ota1_size, 0x300000);
            }
            other => panic!("unexpected update boot fixture: {other:?}"),
        }
        assert_eq!(
            postcard::to_slice(&update_boot_response, &mut buf).unwrap(),
            update_boot_response_fixture
        );
    }

    #[test]
    fn protocol_fragment_frames_roundtrip_edge_sizes() {
        let begin = Frame::FragBegin {
            id: u16::MAX,
            total_len: 64 * 1024,
            kind: FragKind::SignDraft,
        };
        let mut begin_buf = [0u8; 32];
        let begin_encoded = postcard::to_slice(&begin, &mut begin_buf).unwrap();
        let decoded_begin: Frame = postcard::from_bytes(begin_encoded).unwrap();
        match decoded_begin {
            Frame::FragBegin {
                id,
                total_len,
                kind,
            } => {
                assert_eq!(id, u16::MAX);
                assert_eq!(total_len, 64 * 1024);
                assert!(matches!(kind, FragKind::SignDraft));
            }
            other => panic!("unexpected decoded frame: {other:?}"),
        }

        let max_chunk = alloc::vec![0x5au8; 512];
        let part = Frame::FragPart {
            id: u16::MAX,
            offset: 64 * 1024 - 512,
            chunk: max_chunk.clone(),
            last: true,
        };
        let mut part_buf = [0u8; 640];
        let part_encoded = postcard::to_slice(&part, &mut part_buf).unwrap();
        let decoded_part: Frame = postcard::from_bytes(part_encoded).unwrap();
        match decoded_part {
            Frame::FragPart {
                id,
                offset,
                chunk,
                last,
            } => {
                assert_eq!(id, u16::MAX);
                assert_eq!(offset, 64 * 1024 - 512);
                assert_eq!(chunk, max_chunk);
                assert!(last);
            }
            other => panic!("unexpected decoded frame: {other:?}"),
        }
    }

    #[test]
    fn protocol_fragment_messages_survive_cobs_boundaries() {
        let msg = Msg {
            v: PROTO_V1,
            id: 0xfeed_beef,
            msg: Frame::FragPart {
                id: 3,
                offset: 0,
                chunk: alloc::vec::Vec::new(),
                last: true,
            },
        };

        let mut cobs_buf = [0u8; 80];
        let cobs_frame = postcard::to_slice_cobs(&msg, &mut cobs_buf).unwrap();
        let split_at = cobs_frame.len() - 1;
        assert!(cobs_frame[..split_at].iter().all(|byte| *byte != 0));
        assert_eq!(cobs_frame[split_at], 0);

        let mut cobs_frame = alloc::vec::Vec::from(cobs_frame);
        let decoded: Msg<Frame> = postcard::from_bytes_cobs(cobs_frame.as_mut_slice()).unwrap();
        assert_eq!(decoded.v, PROTO_V1);
        assert_eq!(decoded.id, 0xfeed_beef);
        match decoded.msg {
            Frame::FragPart {
                id,
                offset,
                chunk,
                last,
            } => {
                assert_eq!(id, 3);
                assert_eq!(offset, 0);
                assert!(chunk.is_empty());
                assert!(last);
            }
            other => panic!("unexpected decoded message: {other:?}"),
        }
    }

    #[test]
    fn protocol_fragment_responses_roundtrip_max_chunk() {
        let chunk = alloc::vec![0xa5u8; 512];
        let response = Response::FragPart {
            id: 9,
            offset: 1024,
            chunk: chunk.clone(),
            last: false,
        };

        let mut buf = [0u8; 640];
        let encoded = postcard::to_slice(&response, &mut buf).unwrap();
        let decoded: Response = postcard::from_bytes(encoded).unwrap();
        match decoded {
            Response::FragPart {
                id,
                offset,
                chunk: decoded_chunk,
                last,
            } => {
                assert_eq!(id, 9);
                assert_eq!(offset, 1024);
                assert_eq!(decoded_chunk, chunk);
                assert!(!last);
            }
            other => panic!("unexpected decoded response: {other:?}"),
        }
    }
}
