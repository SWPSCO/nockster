#![cfg_attr(not(test), no_std)]
extern crate alloc;

pub mod math;
pub mod cheetah;
pub mod noun;
pub use cheetah::{Hash, T8, XKey, cheetah_pub_from_sk, schnorr_sign_txid};

use alloc::vec::Vec;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

pub enum Curve { Secp256k1 }

pub struct DerivedKey {
    pub pubkey: Vec<u8>,
    pub chain_code: [u8; 32],
}

pub trait Signer {
    fn master_fingerprint(&self) -> [u8; 4];
    fn derive_pubkey(&self, path: &[u32]) -> DerivedKey;
    fn sign_digest32(&self, path: &[u32], digest32: [u8;32]) -> [u8; 64];
}

pub const PROTO_V1: u8 = 1;

// Error codes
pub const ERR_BAD_COBS_OR_POSTCARD: u16 = 100;
pub const ERR_OVERFLOW: u16            = 102;
pub const ERR_ENCODE_TOO_BIG: u16      = 103;
pub const ERR_UNSUPPORTED_VERSION: u16 = 110;
pub const ERR_NO_SEED: u16             = 120;

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
    pub fp4: [u8;4],
    pub child: u32,
    #[serde(with = "BigArray")]
    pub chain_code: [u8;32],
    #[serde(with = "BigArray")]
    pub pubkey33: [u8;33],
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Request {
    Hello,
    Ping,
    Health,
    SetSeed { #[serde(with = "BigArray")] seed64: [u8; 64] },
    Wipe,
    GetFingerprint,
    GetPubkey { path: alloc_path::Path, #[serde(default)] compressed: bool },
    SignDigest { path: alloc_path::Path, digest32: [u8; 32] },
    GetCheetahPub { path: alloc_path::Path },
    SignTxId { path: alloc_path::Path, txid5: [u64; 5] },
    GetXpub { path: alloc_path::Path },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Response {
    Hello(Caps),
    Pong,
    Ok,
    HealthOk,
    OkFingerprint { fp4: [u8; 4] },
    OkPubkey { #[serde(with = "BigArray")] uncompressed: [u8; 65] },
    OkPubkeyCompressed { #[serde(with = "BigArray")] compressed: [u8; 33] },
    OkSig { #[serde(with = "BigArray")] sig64: [u8; 64] },
    OkXpub(Xpub),
    OkCheetahPub { x: [u64; 6], y: [u64; 6] },
    OkCheetahSig { chal: [u64; 8], sig: [u64; 8] },
    Err  { code: u16 },
}

// Host uses Vec<u32>, firmware uses HVec<u32,16>
pub mod alloc_path {
  #[cfg(feature = "alloc-path")]
  pub type Path = alloc::vec::Vec<u32>;

  #[cfg(not(feature = "alloc-path"))]
  pub use heapless::Vec as HVec;
  #[cfg(not(feature = "alloc-path"))]
  pub type Path = HVec<u32, 16>;
}