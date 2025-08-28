// src/cheetah/mod.rs
#![no_std]
extern crate alloc;

use alloc::vec::Vec;
use hmac::{Hmac, Mac};
use sha2::{Sha256, Sha512};
use ibig::UBig;

// ---------- public types you use in protocol ----------
#[derive(Clone, Copy)]
pub struct Hash { pub values: [u64; 5] }             // TIP5 input shape
#[derive(Clone, Copy)]
pub struct T8   { pub values: [u64; 8] }             // 8 limb (LE-by-limb)
#[derive(Clone, Copy)]
pub struct F6LT { pub values: [u64; 6] }             // 6 limb (LE-by-limb)

// Minimal extended key for your curve
pub struct XKey {
    pub sk: Option<[u8;32]>,         // None for public-only nodes
    pub pk_xy: ([u64;6],[u64;6]),    // affine
    pub cc: [u8;32],                 // chain code
    pub depth: u8,
    pub index: u32,                  // raw, MSB = hardened flag allowed
    pub parent_fp: [u8;4],
}

impl XKey {
    pub fn from_master(sk: [u8;32], cc: [u8;32]) -> Self {
        let pk_xy = cheetah_pub_from_sk(sk);
        Self { sk: Some(sk), pk_xy, cc, depth: 0, index: 0, parent_fp: [0;4] }
    }
}

// ----------------- helpers you already have -----------------
// NOTE: Replace the bodies with your WORKING code from signer.rs.
// These signatures match how you call them in the FW.

#[inline] fn is_hardened(i: u32) -> bool { i >= (1 << 31) }

// SLIP-10 master (seed -> (sk, cc)) for your curve / label
pub fn master_from_seed(seed64: &[u8;64]) -> ([u8;32], [u8;32]) {
    // <--- paste your code here (HMAC-SHA512 over the seed label you use) --->
    unimplemented!()
}

// Private child derivation (hardened + non-hardened)
pub fn xprv_derive_child(parent: &XKey, i: u32) -> XKey {
    // <--- paste your code here (your SLIP-10 child derivation rules) --->
    unimplemented!()
}

// Public child derivation (non-hardened only), if you need it:
pub fn xpub_derive_child(parent: &XKey, i: u32) -> XKey {
    // <--- paste your code here --->
    unimplemented!()
}

// Scalar mult to get affine point (x,y) as 6-limb LE each
pub fn cheetah_pub_from_sk(sk_be32: [u8;32]) -> ([u64;6],[u64;6]) {
    // <--- paste your code here --->
    unimplemented!()
}

// Schnorr (TIP5) on (R || P || txid) returning (chal, sig) as T8 limbs
pub fn schnorr_sign_txid(sk_be32: [u8;32], pk_xy: ([u64;6],[u64;6]), txid: Hash) -> (T8, T8) {
    // <--- paste your RFC6979-HMAC-SHA256 deterministic k, TIP5 challenge, etc. --->
    unimplemented!()
}
