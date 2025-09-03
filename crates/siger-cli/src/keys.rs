//! Key import/derivation for CLI (no signing here).
//! - BIP39 mnemonic (+ passphrase) → 64B seed → SLIP-10 master → path derivation
//! - Base58 raw private key (32B big-endian scalar)
//! - Exports a compact device blob for the ESP32 key-store.

use std::path::{Path, PathBuf};
use std::fs;

use bs58;
use pbkdf2::pbkdf2_hmac;
use sha2::Sha512;
use unicode_normalization::UnicodeNormalization;
use hmac::Mac;
use num_traits::Zero;
use ibig::UBig;
use serde::{Serialize, Deserialize};

use siger_core::cheetah::cheetah_pub_from_sk;

/// 97-byte serialization of a-pt (6×u64 X, 6×u64 Y, 1 byte inf=0)
const SER_LIMBS_BIG_ENDIAN: bool = false;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "origin", rename_all = "snake_case")]
pub enum KeyOrigin {
    Mnemonic {
        path: String,
        passphrase: String,
    },
    PrivateKeyB58,
    PrivateKeyHex,
    SeedBytes,
}

/// What we persist for the device + for human/debug views
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedKey {
    /// 32-byte BE secret scalar
    pub sk_be32_hex: String,
    /// 32-byte chain code (zeros if unknown)
    pub cc_hex: String,
    /// Public key in base58 (cheetah a-pt encoding parity)
    pub pk_b58: String,
    /// Derivation path if applicable (e.g., m/44'/1337'/0'/0/0)
    pub path: Option<String>,
    /// Origin info (mnemonic path, base58 raw, etc.)
    pub origin: KeyOrigin,
}

/// Blob format for ESP32. Fixed-size to keep flashing/simple transport easy.
///
/// Layout (169 bytes):
///   0..8   : magic "NCKEYV1\0"
///   8..40  : sk (32 bytes BE)
///   40..72 : chain code (32 bytes; may be zeroed)
///   72..169: ser-a-pt (97 bytes; inf=0)
pub const DEVICE_BLOB_V1_SIZE: usize = 169;

pub fn device_blob_v1(sk: [u8; 32], cc: [u8; 32], pk_xy: ([u64; 6], [u64; 6])) -> [u8; DEVICE_BLOB_V1_SIZE] {
  let mut out = [0u8; DEVICE_BLOB_V1_SIZE];
  out[0..8].copy_from_slice(b"NCKEYV1\0");
  out[8..40].copy_from_slice(&sk);
  out[40..72].copy_from_slice(&cc);

  // ser-a-pt (x limbs then y limbs, little-endian per limb by default, inf=0)
  let mut off = 72usize;
  for limb in pk_xy.0.iter().chain(pk_xy.1.iter()) {  // Use .0 and .1 instead of [0] and [1]
      let b = if SER_LIMBS_BIG_ENDIAN { limb.to_be_bytes() } else { limb.to_le_bytes() };
      out[off..off + 8].copy_from_slice(&b);
      off += 8;
  }
  out[72 + 12 * 8] = 0; // inf = false
  out
}

/// mnemo+pass → 64B BIP39 seed
pub fn bip39_seed_from_mnemonic(mnemonic: &str, passphrase: &str) -> [u8; 64] {
    let pw = mnemonic.nfkd().collect::<String>();
    let salt = format!("mnemonic{}", passphrase.nfkd().collect::<String>());
    let mut out = [0u8; 64];
    pbkdf2_hmac::<Sha512>(pw.as_bytes(), salt.as_bytes(), 2048, &mut out);
    out
}

/// --- SLIP-10 (Hoon parity) -------------------------------------------------

type HmacSha512 = hmac::Hmac<Sha512>;
const NOCKCHAIN_SLIP10_KEY: &[u8] = b"Nockchain seed";

/// Hoon parity: rehash the whole 64B if left==0 or >=n
pub fn master_from_seed(seed: &[u8]) -> ([u8; 32], [u8; 32]) {
    let n = cheetah_order();

    let mut mac = HmacSha512::new_from_slice(NOCKCHAIN_SLIP10_KEY).unwrap();
    mac.update(seed);
    let mut i = mac.finalize().into_bytes().to_vec();

    loop {
        let mut left = [0u8; 32];
        let mut right = [0u8; 32];
        left.copy_from_slice(&i[..32]);
        right.copy_from_slice(&i[32..]);

        let sk = UBig::from_be_bytes(&left);
        if !sk.is_zero() && sk < n {
            return (left, right);
        }

        let mut mac = HmacSha512::new_from_slice(NOCKCHAIN_SLIP10_KEY).unwrap();
        mac.update(&i);
        i = mac.finalize().into_bytes().to_vec();
    }
}

#[derive(Clone)]
pub struct XKey {
    pub sk: Option<[u8; 32]>,
    pub pk_xy: ([u64; 6], [u64; 6]),
    pub cc: [u8; 32],
    pub depth: u8,
    pub index: u32,
    pub parent_fp: [u8; 4],
}

pub fn xkey_from_seed(seed64: &[u8; 64]) -> XKey {
    let (sk, cc) = master_from_seed(seed64);
    let pk_xy = cheetah_pub_from_sk(sk);
    XKey { sk: Some(sk), pk_xy, cc, depth: 0, index: 0, parent_fp: [0u8; 4] }
}

#[inline]
fn is_hardened(i: u32) -> bool { i >= (1 << 31) }
#[inline]
fn ser32_be(i: u32) -> [u8; 4] { i.to_be_bytes() }
#[inline]
fn ser256_be(sk: &[u8; 32]) -> [u8; 32] { *sk }

// 97 bytes: 6×u64 X, 6×u64 Y, 1 byte inf
fn ser_a_pt(pk_xy: &([u64; 6], [u64; 6])) -> [u8; 97] {
  let mut out = [0u8; 97];
  let mut off = 0usize;
  for limb in pk_xy.0.iter().chain(pk_xy.1.iter()) {  // Use .0 and .1
      let b = if SER_LIMBS_BIG_ENDIAN { limb.to_be_bytes() } else { limb.to_le_bytes() };
      out[off..off + 8].copy_from_slice(&b);
      off += 8;
  }
  out[96] = 0;
  out
}

#[inline]
fn hmac_split_512(key: &[u8; 32], data: &[u8]) -> ([u8; 32], [u8; 32]) {
    let mut mac = HmacSha512::new_from_slice(key).unwrap();
    mac.update(data);
    let i = mac.finalize().into_bytes();
    let mut left = [0u8; 32];
    let mut right = [0u8; 32];
    left.copy_from_slice(&i[..32]);
    right.copy_from_slice(&i[32..]);
    (left, right)
}

fn cheetah_order() -> UBig {
    const GROUP_ORDER_HEX: &str =
        "7af2599b3b3f22d0563fbf0f990a37b5327aa72330157722d443623eaed4accf";
    UBig::from_str_radix(GROUP_ORDER_HEX, 16).expect("valid group order")
}

pub fn xprv_derive_child(parent: &XKey, i: u32) -> XKey {
    let n = cheetah_order();

    let (prv, cc) = (parent.sk.expect("need private key"), parent.cc);
    let prv_int = UBig::from_be_bytes(&prv);

    let pub_ser = ser_a_pt(&parent.pk_xy);

    let mut data = Vec::with_capacity(1 + 32 + 4 + 97);
    if is_hardened(i) {
        data.push(0u8);
        data.extend_from_slice(&ser256_be(&prv));
        data.extend_from_slice(&ser32_be(i));
    } else {
        data.extend_from_slice(&pub_ser);
        data.extend_from_slice(&ser32_be(i));
    }

    let (left, right) = hmac_split_512(&cc, &data);
    let mut left_int = UBig::from_be_bytes(&left);
    let mut child_sk_int = (&left_int + &prv_int) % &n;

    if !(left_int < n) || child_sk_int.is_zero() {
        let mut red = Vec::with_capacity(1 + 32 + 4);
        red.push(0x01);
        red.extend_from_slice(&right);
        red.extend_from_slice(&ser32_be(i));
        let (left2, right2) = hmac_split_512(&cc, &red);
        left_int = UBig::from_be_bytes(&left2);
        child_sk_int = (&left_int + &prv_int) % &n;
        assert!(!child_sk_int.is_zero() && left_int < n, "invalid after retry");
        return xkey_from_child_int(child_sk_int, right2, parent, i);
    }

    xkey_from_child_int(child_sk_int, right, parent, i)
}

fn xkey_from_child_int(child_sk: UBig, cc: [u8; 32], parent: &XKey, i: u32) -> XKey {
    let mut be = child_sk.to_be_bytes();
    if be.len() < 32 {
        let mut pad = vec![0u8; 32 - be.len()];
        pad.extend_from_slice(&be);
        be = pad;
    }
    let mut sk = [0u8; 32];
    sk.copy_from_slice(&be[be.len() - 32..]);
    let pk_xy = cheetah_pub_from_sk(sk);
    XKey { sk: Some(sk), pk_xy, cc, depth: parent.depth + 1, index: i, parent_fp: [0u8; 4] }
}

/// --- Derivation path parsing ----------------------------------------------

#[derive(Debug, Clone, Copy)]
struct PathElem { index: u32, hardened: bool }

fn parse_path(s: &str) -> Result<Vec<PathElem>, String> {
    let s = s.trim();
    if s.is_empty() { return Err("empty derivation path".into()); }
    if s == "m" { return Ok(vec![]); }
    let rest = s.strip_prefix("m/").ok_or_else(|| "path must start with 'm'".to_string())?;
    let mut out = Vec::new();
    for part in rest.split('/') {
        let hardened = part.ends_with('\'');
        let num_str = if hardened { &part[..part.len()-1] } else { part };
        let idx: u32 = num_str.parse().map_err(|_| format!("bad index: {part}"))?;
        out.push(PathElem { index: idx, hardened });
    }
    Ok(out)
}

fn derive_xprv_path(mut xk: XKey, path: &str) -> Result<XKey, String> {
    for e in parse_path(path)? {
        let i = if e.hardened { e.index | (1<<31) } else { e.index };
        xk = xprv_derive_child(&xk, i);
    }
    Ok(xk)
}

/// --- Public helpers used by CLI commands -----------------------------------

/// Import from mnemonic (and path), returning structures for storage + device blob.
pub fn import_from_mnemonic(phrase: &str, passphrase: &str, path: &str) -> Result<(ImportedKey, [u8; DEVICE_BLOB_V1_SIZE]), String> {
    let seed64 = bip39_seed_from_mnemonic(phrase, passphrase);
    let xk0 = xkey_from_seed(&seed64);
    let child = derive_xprv_path(xk0, path)?;
    let sk = child.sk.ok_or("derived node has no private key")?;
    let cc = child.cc;
    let pk_xy = child.pk_xy;

    let pk_b58 = pubkey_to_b58(pk_xy);
    let key = ImportedKey {
        sk_be32_hex: hex::encode(sk),
        cc_hex: hex::encode(cc),
        pk_b58,
        path: Some(path.to_string()),
        origin: KeyOrigin::Mnemonic {
            path: path.to_string(),
            passphrase: passphrase.to_string(),
        },
    };
    let blob = device_blob_v1(sk, cc, pk_xy);
    Ok((key, blob))
}

/// Import from base58 32-byte scalar (raw), no chain-code/path.
pub fn import_from_b58_priv(b58: &str) -> Result<(ImportedKey, [u8; DEVICE_BLOB_V1_SIZE]), String> {
    let sk = sk_from_b58(b58)?;
    let cc = [0u8; 32];
    let pk_xy = cheetah_pub_from_sk(sk);

    let pk_b58 = pubkey_to_b58(pk_xy);
    let key = ImportedKey {
        sk_be32_hex: hex::encode(sk),
        cc_hex: hex::encode(cc),
        pk_b58,
        path: None,
        origin: KeyOrigin::PrivateKeyB58,
    };
    let blob = device_blob_v1(sk, cc, pk_xy);
    Ok((key, blob))
}

/// Import from raw hex private key (32B)
pub fn import_from_hex_priv(hex_sk: &str) -> Result<(ImportedKey, [u8; DEVICE_BLOB_V1_SIZE]), String> {
    let mut sk = [0u8; 32];
    let v = hex::decode(hex_sk).map_err(|e| format!("hex decode: {e}"))?;
    if v.len() != 32 { return Err(format!("expected 32 bytes, got {}", v.len())); }
    sk.copy_from_slice(&v);
    let cc = [0u8; 32];
    let pk_xy = cheetah_pub_from_sk(sk);

    let pk_b58 = pubkey_to_b58(pk_xy);
    let key = ImportedKey {
        sk_be32_hex: hex::encode(sk),
        cc_hex: hex::encode(cc),
        pk_b58,
        path: None,
        origin: KeyOrigin::PrivateKeyHex,
    };
    let blob = device_blob_v1(sk, cc, pk_xy);
    Ok((key, blob))
}

/// Import from 64-byte seed (already PBKDF’d) and path (full private derivation).
pub fn import_from_seed(seed64: &[u8; 64], path: &str) -> Result<(ImportedKey, [u8; DEVICE_BLOB_V1_SIZE]), String> {
    let xk0 = xkey_from_seed(seed64);
    let child = derive_xprv_path(xk0, path)?;
    let sk = child.sk.ok_or("derived node has no private key")?;
    let cc = child.cc;
    let pk_xy = child.pk_xy;

    let pk_b58 = pubkey_to_b58(pk_xy);
    let key = ImportedKey {
        sk_be32_hex: hex::encode(sk),
        cc_hex: hex::encode(cc),
        pk_b58,
        path: Some(path.to_string()),
        origin: KeyOrigin::SeedBytes,
    };
    let blob = device_blob_v1(sk, cc, pk_xy);
    Ok((key, blob))
}

/// Persist json + bin to disk
pub fn write_key_files(out_base: &Path, key: &ImportedKey, blob: &[u8; DEVICE_BLOB_V1_SIZE]) -> Result<(PathBuf, PathBuf), String> {
    let json_path = out_base.with_extension("json");
    let bin_path  = out_base.with_extension("bin");
    let json = serde_json::to_vec_pretty(key).map_err(|e| e.to_string())?;
    fs::write(&json_path, json).map_err(|e| format!("write {:?}: {e}", json_path))?;
    fs::write(&bin_path,  blob).map_err(|e| format!("write {:?}: {e}", bin_path))?;
    Ok((json_path, bin_path))
}

/// base58 private key → 32-byte big-endian scalar
fn sk_from_b58(s: &str) -> Result<[u8; 32], String> {
    let v = bs58::decode(s).into_vec().map_err(|e| format!("base58 decode: {e}"))?;
    if v.len() < 32 {
        return Err(format!("base58 key too short: {} bytes (need >=32)", v.len()));
    }
    let mut sk = [0u8; 32];
    sk.copy_from_slice(&v[v.len()-32..]); // take last 32 as big-endian scalar
    Ok(sk)
}

/// Cheetah a-pt to base58
fn pubkey_to_b58(pk_xy: ([u64; 6], [u64; 6])) -> String {
  use num_bigint::BigUint;
  use num_traits::{Zero, One};

  fn pack_le(words: &[u64]) -> BigUint {
      let mut n = BigUint::zero();
      for (i, &w) in words.iter().enumerate() {
          n += BigUint::from(w) << (64 * i);
      }
      n
  }

  let x = pack_le(&pk_xy.0);  // Use .0
  let y = pack_le(&pk_xy.1);  // Use .1
  let mut n: BigUint = (y << (64 * 6)) | x;
  n = (n << 1) | BigUint::one();

  bs58::encode(n.to_bytes_be()).into_string()
}