use std::fs;
use std::path::{Path, PathBuf};

use bs58;
use pbkdf2::pbkdf2_hmac;
use serde::{Deserialize, Serialize};
use sha2::Sha512;
use unicode_normalization::UnicodeNormalization;

use siger_core::cheetah::{
    cheetah_pub_from_sk, master_from_seed, ser_a_pt, ser_a_pt_rep104, xprv_derive_child, XKey,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "origin", rename_all = "snake_case")]
pub enum KeyOrigin {
    Mnemonic { path: String, passphrase: String },
    PrivateKeyB58,
    PrivateKeyHex,
    SeedBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LockKey(pub tx_types::transaction_types::Lock);

/// persisted data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedKey {
    /// 32-byte BE secret scalar
    pub sk_be32_hex: String,
    /// 32-byte chain code (zeros if unknown)
    pub cc_hex: String,
    /// public key in base58
    pub pk_b58: String,
    /// derivation path if applicable (e.g., m/44'/1337'/0'/0/0)
    pub path: Option<String>,
    /// origin info (mnemonic path, base58 raw, etc.)
    pub origin: KeyOrigin,
}

/// Layout (176 bytes):
///   0..8    : magic "NCKEYV1\0"
///   8..40   : sk (32 bytes BE)
///   40..72  : chain code (32 bytes; may be zeroed)
///   72..176 : ser-a-pt (104 bytes: 8B sentinel + X[6] + Y[6])
pub const DEVICE_BLOB_V1_SIZE: usize = 176;

pub fn device_blob_v1(
    sk: [u8; 32],
    cc: [u8; 32],
    pk_xy: ([u64; 6], [u64; 6]),
) -> [u8; DEVICE_BLOB_V1_SIZE] {
    let mut out = [0u8; DEVICE_BLOB_V1_SIZE];
    out[0..8].copy_from_slice(b"NCKEYV1\0");
    out[8..40].copy_from_slice(&sk);
    out[40..72].copy_from_slice(&cc);
    let ser = ser_a_pt_rep104(&pk_xy);
    out[72..176].copy_from_slice(&ser); // 104 bytes
    out
}

/// mnemonic & optional passphrase to 64B seed
pub fn bip39_seed_from_mnemonic(mnemonic: &str, passphrase: &str) -> [u8; 64] {
    let pw = mnemonic.nfkd().collect::<String>();
    let salt = format!("mnemonic{}", passphrase.nfkd().collect::<String>());
    let mut out = [0u8; 64];
    pbkdf2_hmac::<Sha512>(pw.as_bytes(), salt.as_bytes(), 2048, &mut out);
    out
}

/// create a master XKey (xprv) from a 64-byte seed
pub fn xkey_from_seed(seed64: &[u8; 64]) -> XKey {
    let (sk, cc) = master_from_seed(seed64);
    let pk_xy = cheetah_pub_from_sk(sk);
    // Convert [[u64; 6]; 2] to ([u64; 6], [u64; 6])
    let pk_tuple = (pk_xy[0], pk_xy[1]);
    XKey {
        sk: Some(sk),
        pk: Some(pk_tuple),
        chain_code: cc,
        depth: 0,
        index: 0,
        parent_fingerprint: [0u8; 4],
    }
}

/// parse derivation path like "m/44'/1337'/0'/0/0"
pub fn parse_path(path: &str) -> Result<Vec<u32>, String> {
    let p = path.trim();
    if p.is_empty() {
        return Err("empty path".into());
    }
    if !p.starts_with('m') {
        return Err("path must start with 'm'".into());
    }
    let mut out = Vec::new();
    for comp in p.split('/').skip(1) {
        if comp.is_empty() {
            continue;
        }
        let hardened = comp.ends_with('\'') || comp.ends_with('h') || comp.ends_with('H');
        let num_str = if hardened {
            &comp[..comp.len() - 1]
        } else {
            comp
        };
        let idx: u32 = num_str.parse().map_err(|_| format!("bad index: {comp}"))?;
        let val = if hardened { idx | 0x8000_0000 } else { idx };
        out.push(val);
    }
    Ok(out)
}

fn derive_xprv_path(mut xk: XKey, path: &str) -> Result<XKey, String> {
    for i in parse_path(path)? {
        xk = xprv_derive_child(&xk, i);
    }
    Ok(xk)
}

/// import from base58 32-byte scalar (raw), no chain-code/path.
pub fn import_from_b58_priv(b58: &str) -> Result<(ImportedKey, [u8; DEVICE_BLOB_V1_SIZE]), String> {
    let sk = sk_from_b58(b58)?;
    let cc = [0u8; 32];
    let pk_xy = cheetah_pub_from_sk(sk);
    // Convert [[u64; 6]; 2] to ([u64; 6], [u64; 6])
    let pk_tuple = (pk_xy[0], pk_xy[1]);

    let pk_b58 = pubkey_to_b58(&pk_tuple);
    let key = ImportedKey {
        sk_be32_hex: hex::encode(sk),
        cc_hex: hex::encode(cc),
        pk_b58,
        path: None,
        origin: KeyOrigin::PrivateKeyB58,
    };
    let blob = device_blob_v1(sk, cc, pk_tuple);
    Ok((key, blob))
}

/// import from raw hex private key (32B)
pub fn import_from_hex_priv(
    hex_sk: &str,
) -> Result<(ImportedKey, [u8; DEVICE_BLOB_V1_SIZE]), String> {
    let mut sk = [0u8; 32];
    let v = hex::decode(hex_sk).map_err(|e| format!("hex decode: {e}"))?;
    if v.len() != 32 {
        return Err(format!("expected 32 bytes, got {}", v.len()));
    }
    sk.copy_from_slice(&v);
    let cc = [0u8; 32];
    let pk_xy = cheetah_pub_from_sk(sk);
    // Convert [[u64; 6]; 2] to ([u64; 6], [u64; 6])
    let pk_tuple = (pk_xy[0], pk_xy[1]);

    let pk_b58 = pubkey_to_b58(&pk_tuple);
    let key = ImportedKey {
        sk_be32_hex: hex::encode(sk),
        cc_hex: hex::encode(cc),
        pk_b58,
        path: None,
        origin: KeyOrigin::PrivateKeyHex,
    };
    let blob = device_blob_v1(sk, cc, pk_tuple);
    Ok((key, blob))
}

/// import from 64-byte seed (PBKDF’d) and path (full private derivation)
pub fn import_from_seed(
    seed64: &[u8; 64],
    path: &str,
) -> Result<(ImportedKey, [u8; DEVICE_BLOB_V1_SIZE]), String> {
    let xk0 = xkey_from_seed(seed64);
    let child = derive_xprv_path(xk0, path)?;
    let sk = child
        .sk
        .ok_or_else(|| "derived node has no private key".to_string())?;
    let cc = child.chain_code;
    let pk_xy = child
        .pk
        .ok_or_else(|| "derived node has no public key".to_string())?;

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

/// persist json + bin to nvs
pub fn write_key_files(
    out_base: &Path,
    key: &ImportedKey,
    blob: &[u8; DEVICE_BLOB_V1_SIZE],
) -> Result<(PathBuf, PathBuf), String> {
    let json_path = out_base.with_extension("json");
    let bin_path = out_base.with_extension("bin");
    let json = serde_json::to_vec_pretty(key).map_err(|e| e.to_string())?;
    fs::write(&json_path, json).map_err(|e| format!("write {:?}: {e}", json_path))?;
    fs::write(&bin_path, blob).map_err(|e| format!("write {:?}: {e}", bin_path))?;
    Ok((json_path, bin_path))
}

/// base58 private key to 32-byte big-endian scalar
fn sk_from_b58(s: &str) -> Result<[u8; 32], String> {
    let v = bs58::decode(s)
        .into_vec()
        .map_err(|e| format!("base58 decode: {e}"))?;
    if v.len() < 32 {
        return Err(format!(
            "base58 key too short: {} bytes (need >=32)",
            v.len()
        ));
    }
    let mut sk = [0u8; 32];
    sk.copy_from_slice(&v[v.len() - 32..]);
    Ok(sk)
}

/// pubkey (affine point) to base58
pub fn pubkey_to_b58(pk: &([u64; 6], [u64; 6])) -> String {
    let ser = ser_a_pt(pk); // 0x01 || Y || X
    bs58::encode(ser).into_string()
}
