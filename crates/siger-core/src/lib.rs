#![cfg_attr(not(test), no_std)]
extern crate alloc;

pub mod cheetah;
pub mod draft_sign;
pub mod math;
pub mod noun;
// Re-export crypto types - use conditional compilation for std vs no_std differences
#[cfg(not(feature = "std"))]
pub use cheetah::{
    cheetah_pub_from_sk, master_from_seed, schnorr_sign_tx, xprv_derive_child, xpub_derive_child,
    Hash, XKey, T8,
};

#[cfg(feature = "std")]
pub use cheetah::{
    cheetah_pub_from_sk, master_from_seed, schnorr_sign_tx, xprv_derive_child,
    Hash, XKey, T8,
};

use crate::alloc_path::Path;
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
pub const ERR_FLASH: u16 = 140;
pub const ERR_CRYPTO: u16 = 141;

pub const FEATURE_CHEETAH: u32 = 1 << 0;
pub const FEATURE_FRAG: u32 = 1 << 1;
pub const FEATURE_XPUB: u32 = 1 << 2;
pub const FEATURE_SECURITY_STATUS: u32 = 1 << 3;
pub const FEATURE_ALL_KNOWN: u32 =
    FEATURE_CHEETAH | FEATURE_FRAG | FEATURE_XPUB | FEATURE_SECURITY_STATUS;

pub const HMAC_KEY_PURPOSE_DOWN_ALL: u8 = 5;
pub const HMAC_KEY_PURPOSE_DOWN_JTAG: u8 = 6;
pub const HMAC_KEY_PURPOSE_DOWN_DS: u8 = 7;
pub const HMAC_KEY_PURPOSE_UP: u8 = 8;

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

// siger-core
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragKind {
    SetSeed,
    SignDraft,
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
    }
}
