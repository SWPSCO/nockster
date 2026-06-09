use std::fs;
use std::path::{Path, PathBuf};

use bs58;
use pbkdf2::pbkdf2_hmac;
use serde::{Deserialize, Serialize};
use sha2::Sha512;
use unicode_normalization::UnicodeNormalization;

use nockster_core::cheetah::{
    cheetah_pub_from_sk, master_from_seed, ser_a_pt, ser_a_pt_rep104, xprv_derive_child, XKey,
};
use tx_types::transaction_types::{Hash, SchnorrPubkey, F6LT};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "origin", rename_all = "snake_case")]
pub enum KeyOrigin {
    Mnemonic { path: String, passphrase: String },
    SeedBytes,
}

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

/// import from 64-byte seed (PBKDF'd) and path (full private derivation)
pub fn import_from_seed(
    seed64: &[u8; 64],
    path: &str,
    version: u8,
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

    let pk_b58 = pubkey_to_b58(&pk_xy, version);
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

/// pubkey (affine point) to base58 - v0 format (97-byte: 0x01 || Y || X)
pub fn pubkey_to_b58_v0(pk: &([u64; 6], [u64; 6])) -> String {
    let ser = ser_a_pt(pk); // 0x01 || Y || X
    bs58::encode(ser).into_string()
}

/// pubkey (affine point) to base58 - v1 format (PKH hash)
pub fn pubkey_to_b58_v1(pk: &([u64; 6], [u64; 6])) -> String {
    let schnorr_pk = SchnorrPubkey {
        x: F6LT { values: pk.0 },
        y: F6LT { values: pk.1 },
        inf: false,
    };
    // v1 uses the public key hash (PKH) in base58
    let pkh: Hash = schnorr_pk.to_hash();
    pkh.to_b58()
}

/// pubkey (affine point) to base58 with version selector
pub fn pubkey_to_b58(pk: &([u64; 6], [u64; 6]), version: u8) -> String {
    match version {
        0 => pubkey_to_b58_v0(pk),
        _ => pubkey_to_b58_v1(pk),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_coordinates_encode_to_nonempty_v1_pkh() {
        let pk = (
            [0x101, 0x102, 0x103, 0x104, 0x105, 0x106],
            [0x201, 0x202, 0x203, 0x204, 0x205, 0x206],
        );
        assert!(!pubkey_to_b58(&pk, 1).is_empty());
    }
}

#[cfg(test)]
mod derivation_tests {
    use super::*;

    // ---- parse_path ----

    #[test]
    fn parse_path_master_is_empty() {
        assert_eq!(parse_path("m").unwrap(), Vec::<u32>::new());
    }

    #[test]
    fn parse_path_mixes_hardened_and_soft() {
        // ' and h and H all mark hardened; bare numbers stay soft.
        let got = parse_path("m/44'/0h/0H/0/5").unwrap();
        assert_eq!(got, vec![44 | 0x8000_0000, 0x8000_0000, 0x8000_0000, 0, 5]);
    }

    #[test]
    fn parse_path_rejects_missing_m_and_bad_index() {
        assert!(parse_path("44'/0").is_err());
        assert!(parse_path("m/oops").is_err());
        assert!(parse_path("").is_err());
    }

    // ---- BIP-39 seed: official Trezor known-answer vector ----
    // mnemonic "abandon x11 about" + passphrase "TREZOR" is a standard BIP-39
    // test vector; pins our PBKDF2-HMAC-SHA512 against the spec, independent of
    // the cheetah curve.
    #[test]
    fn bip39_seed_matches_trezor_vector() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon \
                        abandon abandon abandon abandon abandon about";
        let seed = bip39_seed_from_mnemonic(mnemonic, "TREZOR");
        let expected = "c55257c360c07c72029aebc1b53c05ed0362ada38ead3e3e9efa3708e5349553\
                        1f09a6987599d18264c1e1c92f2cf141630c7a3c4ab7c81b2f001698e7463b04";
        assert_eq!(hex::encode(seed), expected);
    }

    // ---- cheetah derivation: regression goldens ----
    // These pin the current host derivation + address encoding so a refactor (or
    // an accidental divergence from firmware/chain) is caught. Values captured
    // from the implementation; update intentionally if the scheme changes.
    // Updated for the SLIP10 unhardened-CKD parity fix (tx-types 9cc0526):
    // the std path now matches the firmware/Hoon ser-p byte layout, which
    // changed unhardened children.
    const SEED7: [u8; 64] = [7u8; 64];
    const PATH: &str = "m/44'/0'/0'/0/0";
    const GOLDEN_SK: &str = "4e2f27f4f0f2ebfbc88053d6a019b057169c83a84e6383cd1431a4c54cd97120";
    const GOLDEN_V0: &str = "2zEv3TitP94fELjTPs2H9QUEXPCEH2Wojxbzwd2tzFqdwF7A52vzN9J8PnSYGio4wYfdSug6jnh7C97Ut9NAcFbvt6upqnu9gPBUNgdvehfA4EvY2QjChWGqzZdH85eT84Vi";
    const GOLDEN_V1: &str = "84YtTuQUNNGfDTu2CgXwjTudKnCfsADKVaiXPc1zGXqT2JLix3Ns87g";

    #[test]
    fn import_from_seed_is_stable() {
        let (k0, _) = import_from_seed(&SEED7, PATH, 0).unwrap();
        let (k1, _) = import_from_seed(&SEED7, PATH, 1).unwrap();
        assert_eq!(k0.sk_be32_hex, GOLDEN_SK);
        assert_eq!(k0.pk_b58, GOLDEN_V0);
        assert_eq!(k1.pk_b58, GOLDEN_V1);
    }

    #[test]
    fn derivation_is_deterministic_and_path_sensitive() {
        let a = import_from_seed(&SEED7, PATH, 1).unwrap().0;
        let b = import_from_seed(&SEED7, PATH, 1).unwrap().0;
        assert_eq!(a.pk_b58, b.pk_b58, "same inputs must derive the same key");

        let other = import_from_seed(&SEED7, "m/44'/0'/0'/0/1", 1).unwrap().0;
        assert_ne!(
            a.pk_b58, other.pk_b58,
            "different path must derive a different key"
        );
    }
}

#[cfg(test)]
mod fw_mnemonic_gen_check {
    //! Cross-check the firmware's on-device generation algorithm (gui/seed.rs:
    //! mnemonic_from_entropy / extract_11_bits) against the `bip39` crate. The fw
    //! can't be compiled in this environment (pre-existing esp-rom-sys break), so
    //! this pins the algorithm host-side. Keep in sync with gui/seed.rs.
    use bip39::{Language, Mnemonic};
    use sha2::{Digest, Sha256};

    fn extract_11_bits(bits: &[u8], start: usize) -> usize {
        let mut value = 0usize;
        for i in 0..11 {
            let bit = start + i;
            let byte = bits[bit / 8];
            let set = (byte >> (7 - (bit % 8))) & 1;
            value = (value << 1) | set as usize;
        }
        value
    }

    fn fw_mnemonic_from_entropy(entropy: &[u8; 32]) -> String {
        let checksum = Sha256::digest(entropy)[0];
        let mut bits = [0u8; 33];
        bits[..32].copy_from_slice(entropy);
        bits[32] = checksum;
        let wl = Language::English.word_list();
        let mut words: Vec<&str> = Vec::new();
        for i in 0..24 {
            words.push(wl[extract_11_bits(&bits, i * 11)]);
        }
        words.join(" ")
    }

    #[test]
    fn matches_bip39_crate_for_known_entropies() {
        for entropy in [[0u8; 32], [0x7fu8; 32], [0xffu8; 32], {
            let mut e = [0u8; 32];
            for (i, b) in e.iter_mut().enumerate() {
                *b = (i as u8).wrapping_mul(37).wrapping_add(11);
            }
            e
        }] {
            let mine = fw_mnemonic_from_entropy(&entropy);
            let theirs = Mnemonic::from_entropy(&entropy).unwrap().to_string();
            assert_eq!(mine, theirs, "mismatch for entropy {:?}", &entropy[..4]);
            // And the phrase we produce must pass full checksum validation.
            assert!(Mnemonic::parse_in(Language::English, &mine).is_ok());
        }
    }

    #[test]
    fn all_zero_entropy_is_the_canonical_phrase() {
        let mine = fw_mnemonic_from_entropy(&[0u8; 32]);
        assert!(mine.starts_with("abandon abandon"));
        assert!(mine.ends_with(" art"));
    }
}
