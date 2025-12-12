#![cfg_attr(not(test), no_std)]
extern crate alloc;

pub mod cheetah;
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
    },
    SignSpendHashFor {
        slot: u8,
        path: alloc_path::Path,
        msg5: [u64; 5],
        pubkey: ([u64; 6], [u64; 6]),
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
