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

use siger_core::cheetah::{XKey, cheetah_pub_from_sk, xprv_derive_child, ser_a_pt, ser_a_pt_rep104, master_from_seed, hmac_split_512, derive_path_transcript};

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

/// Layout (176 bytes):
///   0..8    : magic "NCKEYV1\0"
///   8..40   : sk (32 bytes BE)
///   40..72  : chain code (32 bytes; may be zeroed)
///   72..176 : ser-a-pt (104 bytes: X[6], Y[6], 1u64)
pub const DEVICE_BLOB_V1_SIZE: usize = 176;

pub fn device_blob_v1(sk: [u8; 32], cc: [u8; 32], pk_xy: ([u64; 6], [u64; 6]))
-> [u8; DEVICE_BLOB_V1_SIZE] {
    let mut out = [0u8; DEVICE_BLOB_V1_SIZE];
    out[0..8].copy_from_slice(b"NCKEYV1\0");
    out[8..40].copy_from_slice(&sk);
    out[40..72].copy_from_slice(&cc);
    let ser = ser_a_pt_rep104(&pk_xy);
    out[72..176].copy_from_slice(&ser); // 104 bytes
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

pub fn xkey_from_seed(seed64: &[u8; 64]) -> XKey {
    let (sk, cc) = master_from_seed(seed64);
    let pk_xy = cheetah_pub_from_sk(sk);
    dump_key_material("raw:", sk, cc, pk_xy);
    XKey { sk: Some(sk), pk: Some(pk_xy), chain_code: cc, depth: 0, index: 0, parent_fingerprint: [0u8; 4] }
}

#[inline]
fn is_hardened(i: u32) -> bool { i >= (1 << 31) }
#[inline]
fn ser32_be(i: u32) -> [u8; 4] { i.to_be_bytes() }
#[inline]
fn ser256_be(sk: &[u8; 32]) -> [u8; 32] { *sk }

fn cheetah_order() -> UBig {
    const GROUP_ORDER_HEX: &str =
        "7af2599b3b3f22d0563fbf0f990a37b5327aa72330157722d443623eaed4accf";
    UBig::from_str_radix(GROUP_ORDER_HEX, 16).expect("valid group order")
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
    dump_key_material("raw:", sk, cc, pk_xy);
    XKey { sk: Some(sk), pk: Some(pk_xy), chain_code: cc, depth: parent.depth + 1, index: i, parent_fingerprint: [0u8; 4] }
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

fn path_to_indices(path: &str) -> Result<Vec<u32>, String> {
    let elems = parse_path(path)?;
    let mut out = Vec::with_capacity(elems.len());
    for e in elems {
        let mut i = e.index;
        if e.hardened { i |= 1 << 31; }
        out.push(i);
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

pub fn derivation_transcript_from_mnemonic(
    phrase: &str,
    passphrase: &str,
    path: &str,
) -> Result<String, String> {
    let seed64 = bip39_seed_from_mnemonic(phrase, passphrase);
    let idxs = path_to_indices(path)?;
    let (_xk, transcript) = derive_path_transcript(&seed64, &idxs);
    Ok(transcript)
}

/// Same, but if you already have the 64-byte seed.
pub fn derivation_transcript_from_seed(seed64: &[u8; 64], path: &str) -> Result<String, String> {
    let idxs = path_to_indices(path)?;
    let (_xk, transcript) = derive_path_transcript(seed64, &idxs);
    Ok(transcript)
}

/// --- Public helpers used by CLI commands -----------------------------------

/// Import from mnemonic (and path), returning structures for storage + device blob.
pub fn import_from_mnemonic(phrase: &str, passphrase: &str, path: &str) -> Result<(ImportedKey, [u8; DEVICE_BLOB_V1_SIZE]), String> {
    let seed64 = bip39_seed_from_mnemonic(phrase, passphrase);
    let xk0 = xkey_from_seed(&seed64);
    let child = derive_xprv_path(xk0, path)?;
    let sk = child.sk.ok_or("derived node has no private key")?;
    let cc = child.chain_code;
    let pk_xy = child.pk.ok_or("derived node has no public key")?;

    dump_key_material("derived:", sk, cc, pk_xy);
    println!("path: {path}");

    let pk_b58 = pubkey_to_b58(&pk_xy);
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
    if let Ok(t) = derivation_transcript_from_seed(&seed64, path) {
        println!("{t}");
    }
    Ok((key, blob))
}

/// Import from base58 32-byte scalar (raw), no chain-code/path.
pub fn import_from_b58_priv(b58: &str) -> Result<(ImportedKey, [u8; DEVICE_BLOB_V1_SIZE]), String> {
    let sk = sk_from_b58(b58)?;
    let cc = [0u8; 32];
    let pk_xy = cheetah_pub_from_sk(sk);

    dump_key_material("raw:", sk, cc, pk_xy);

    let pk_b58 = pubkey_to_b58(&pk_xy);
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

    dump_key_material("raw:", sk, cc, pk_xy);

    let pk_b58 = pubkey_to_b58(&pk_xy);
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
    let cc = child.chain_code;
    let pk_xy = child.pk.ok_or("derived node has no public key")?;

    dump_key_material("raw:", sk, cc, pk_xy);

    let pk_b58 = pubkey_to_b58(&pk_xy);
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
// pub fn pubkey_to_b58(pk: &([u64; 6], [u64; 6])) -> String {
//     let ser = ser_a_pt_rep104(pk);
//     bs58::encode(ser).into_string()
// }

pub fn pubkey_to_b58(pk: &([u64; 6], [u64; 6])) -> String {
    // 104 bytes: X[6]*u64 || Y[6]*u64 || 1u64, big-endian bytes for Base58.
    let ser = ser_a_pt(pk);
    bs58::encode(ser).into_string()
}

fn sk_to_b58(sk: [u8; 32]) -> String {
    bs58::encode(sk).into_string() // raw 32 bytes → Base58 (no version/checksum)
}

fn dump_key_material(prefix: &str, sk: [u8; 32], cc: [u8; 32], pk_xy: ([u64; 6], [u64; 6])) {
    let ser = ser_a_pt(&pk_xy);
    let b58    = bs58::encode(&ser).into_string();

    println!("{prefix} privkey (hex):  {}", hex::encode(sk));
    println!("{prefix} privkey (b58):  {}", sk_to_b58(sk));
    println!("{prefix} chaincode (hex): {}", hex::encode(cc));
    println!("{prefix} pubkey  (b58):  {b58}");

    eprintln!("pubkey-ser(104) = {}", hex::encode(&ser));
    eprintln!("Base58(104)     = {b58}");
    eprintln!("len={} sentinel_u64=0x{}", ser.len(), hex::encode(&ser));
}


