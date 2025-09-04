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
use serde::{Serialize, Deserialize};

use siger_core::cheetah::{XKey, cheetah_pub_from_sk, xprv_derive_child, ser_a_pt, ser_a_pt_rep104, master_from_seed, derive_path_transcript};

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

pub fn xkey_from_seed(seed64: &[u8; 64]) -> XKey {
    let (sk, cc) = master_from_seed(seed64);
    let pk_xy = cheetah_pub_from_sk(sk);
    probe_master_variants(seed64, KNOWN_GOOD_MASTER_SER97_HEX);
    check_known_good_from_pk(&pk_xy, KNOWN_GOOD_MASTER_SER97_HEX);
    dump_key_material("raw:", sk, cc, pk_xy);
    XKey { sk: Some(sk), pk: Some(pk_xy), chain_code: cc, depth: 0, index: 0, parent_fingerprint: [0u8; 4] }
}

/// --- Derivation path parsing ----------------------------------------------

pub fn parse_path(path: &str) -> Result<alloc::vec::Vec<u32>, String> {
  let p = path.trim();
  if p.is_empty() { return Err("empty path".into()); }
  if !p.starts_with('m') { return Err("path must start with 'm'".into()); }
  let mut out = Vec::new();
  for comp in p.split('/').skip(1) {
      if comp.is_empty() { continue; }
      let hardened = comp.ends_with('\'') || comp.ends_with('h') || comp.ends_with('H');
      let num_str = if hardened { &comp[..comp.len() - 1] } else { comp };
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

pub fn derivation_transcript_from_mnemonic(
    phrase: &str,
    passphrase: &str,
    path: &str,
) -> Result<String, String> {
    let seed64 = bip39_seed_from_mnemonic(phrase, passphrase);
    let idxs = parse_path(path)?;
    let (_xk, transcript) = derive_path_transcript(&seed64, &idxs);
    Ok(transcript)
}

/// Same, but if you already have the 64-byte seed.
pub fn derivation_transcript_from_seed(seed64: &[u8; 64], path: &str) -> Result<String, String> {
    let idxs = parse_path(path)?;
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

pub fn pubkey_to_b58(pk: &([u64; 6], [u64; 6])) -> String {
  // 97 bytes: 0x01 + 12 u64 limbs (big-endian)
  let ser = ser_a_pt(pk);
  bs58::encode(ser).into_string()
}

fn dump_key_material(prefix: &str, sk: [u8; 32], cc: [u8; 32], pk_xy: ([u64; 6],[u64; 6])) {
  let ser97  = ser_a_pt(&pk_xy);
  let b58_97 = bs58::encode(&ser97).into_string();

  let ser104  = ser_a_pt_rep104(&pk_xy);
  let b58_104 = bs58::encode(&ser104).into_string();

  println!("{prefix} pubkey  (b58)       : {b58_97}");
  eprintln!("pubkey-ser(97)  = {}", hex::encode(&ser97));
  eprintln!("Base58(97)      = {b58_97}");

  eprintln!("pubkey-ser(104) = {}", hex::encode(&ser104));
  eprintln!("Base58(104)     = {b58_104}");

  println!("{prefix} privkey (hex):  {}", hex::encode(sk));
  println!("{prefix} privkey (b58):  {}", bs58::encode(sk).into_string());
  println!("{prefix} chaincode (hex): {}", hex::encode(cc));
}


// ---------- KNOWN-GOOD MASTER CHECK + DIAGNOSTICS ----------------------------

const KNOWN_GOOD_MASTER_SER97_HEX: &str =
    "01fbd221c97b1e6eeabb439d13f38b6861896c79a6a1881dec43c0f6573bb188\
     b76144934507d6b8e4bb9e68f772f6533b852b45f2db4683a9d5490c12ddc5ad\
     d1752aabf3cc3dca40b216d2241c9fdb6e8973ee61568f06794cebd558d1939624";

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len()*2);
    for &b in bytes { s.push_str(&format!("{:02x}", b)); }
    s
}

#[derive(Clone, Copy)]
struct SerFlags { swap_xy: bool, rev_x: bool, rev_y: bool, le64: bool, rep104: bool }

fn ser_with_flags(pk: &([u64;6],[u64;6]), f: SerFlags) -> Vec<u8> {
    let (mut x, mut y) = *pk;
    if f.swap_xy { core::mem::swap(&mut x, &mut y); }
    if f.rev_x   { x.reverse(); }
    if f.rev_y   { y.reverse(); }

    let mut out = Vec::with_capacity(if f.rep104 { 104 } else { 97 });
    if f.rep104 { out.extend_from_slice(&1u64.to_be_bytes()); } else { out.push(0x01); }

    for w in x.into_iter().chain(y.into_iter()) {
        let be = w.to_be_bytes();
        let le = w.to_le_bytes();
        out.extend_from_slice(if f.le64 { &le } else { &be });
    }
    out
}

fn try_all_serializations(pk: &([u64;6],[u64;6]), known_hex: &str) -> bool {
    let modes = [
        SerFlags{swap_xy:false,rev_x:false,rev_y:false,le64:false,rep104:false},
        SerFlags{swap_xy:false,rev_x:true ,rev_y:false,le64:false,rep104:false},
        SerFlags{swap_xy:false,rev_x:false,rev_y:true ,le64:false,rep104:false},
        SerFlags{swap_xy:false,rev_x:true ,rev_y:true ,le64:false,rep104:false},
        SerFlags{swap_xy:true ,rev_x:false,rev_y:false,le64:false,rep104:false},
        SerFlags{swap_xy:true ,rev_x:true ,rev_y:false,le64:false,rep104:false},
        SerFlags{swap_xy:true ,rev_x:false,rev_y:true ,le64:false,rep104:false},
        SerFlags{swap_xy:true ,rev_x:true ,rev_y:true ,le64:false,rep104:false},

        // unlikely but helpful for debugging accidental LE writes
        SerFlags{swap_xy:false,rev_x:false,rev_y:false,le64:true ,rep104:false},
        SerFlags{swap_xy:false,rev_x:true ,rev_y:false,le64:true ,rep104:false},
        SerFlags{swap_xy:false,rev_x:false,rev_y:true ,le64:true ,rep104:false},
        SerFlags{swap_xy:false,rev_x:true ,rev_y:true ,le64:true ,rep104:false},
        SerFlags{swap_xy:true ,rev_x:false,rev_y:false,le64:true ,rep104:false},
        SerFlags{swap_xy:true ,rev_x:true ,rev_y:false,le64:true ,rep104:false},
        SerFlags{swap_xy:true ,rev_x:false,rev_y:true ,le64:true ,rep104:false},
        SerFlags{swap_xy:true ,rev_x:true ,rev_y:true ,le64:true ,rep104:false},

        // 104B formats, for visibility (shouldn’t match the 97B reference)
        SerFlags{swap_xy:false,rev_x:false,rev_y:false,le64:false,rep104:true},
        SerFlags{swap_xy:false,rev_x:true ,rev_y:false,le64:false,rep104:true},
        SerFlags{swap_xy:false,rev_x:false,rev_y:true ,le64:false,rep104:true},
        SerFlags{swap_xy:false,rev_x:true ,rev_y:true ,le64:false,rep104:true},
        SerFlags{swap_xy:true ,rev_x:false,rev_y:false,le64:false,rep104:true},
        SerFlags{swap_xy:true ,rev_x:true ,rev_y:false,le64:false,rep104:true},
        SerFlags{swap_xy:true ,rev_x:false,rev_y:true ,le64:false,rep104:true},
        SerFlags{swap_xy:true ,rev_x:true ,rev_y:true ,le64:false,rep104:true},
    ];

    let target = known_hex.replace(char::is_whitespace, "").to_ascii_lowercase();
    for (i, m) in modes.iter().enumerate() {
        let hex = to_hex(&ser_with_flags(pk, *m));
        if hex == target {
            eprintln!(
                "🎯 known-good matched with variant #{i}: \
                 swap_xy={} rev_x={} rev_y={} le64={} rep104={}",
                m.swap_xy, m.rev_x, m.rev_y, m.le64, m.rep104
            );
            return true;
        }
    }
    false
}

fn check_known_good_from_pk(pk: &([u64;6],[u64;6]), known_hex: &str) {
    let ser = ser_a_pt(pk);
    let got = to_hex(&ser);
    eprintln!("MASTER ser_a_pt = {}", got);

    let expect = known_hex.replace(char::is_whitespace, "");
    if got.eq_ignore_ascii_case(&expect) {
        eprintln!("✅ matches known good");
    } else {
        eprintln!("❌ mismatch ({} vs {})", got.len(), expect.len());
        // Help find common mistakes
        if !try_all_serializations(pk, known_hex) {
            eprintln!("(No serializer variant matched — if the core flip in \
                       cheetah_pub_from_sk is applied, the scalar ladder or base point \
                       is likely wrong.)");
        }
    }
}

// -------- PROBE MASTER KDF VARIANTS (label × rehash) ------------------------
use hmac::Hmac;
use hmac::Mac;
type HmacSha512 = Hmac<Sha512>;

fn hmac512(key: &[u8], data: &[u8]) -> [u8; 64] {
    let mut mac = HmacSha512::new_from_slice(key).unwrap();
    mac.update(data);
    let out = mac.finalize().into_bytes();
    let mut b = [0u8; 64];
    b.copy_from_slice(&out);
    b
}

fn master_once(label: &[u8], seed64: &[u8; 64]) -> ([u8; 32], [u8; 32]) {
    let i = hmac512(label, seed64);
    let mut il = [0u8; 32];
    let mut ir = [0u8; 32];
    il.copy_from_slice(&i[..32]);
    ir.copy_from_slice(&i[32..]);
    (il, ir)
}

fn master_with_rehash(label: &[u8], seed64: &[u8; 64]) -> ([u8; 32], [u8; 32]) {
    // loop until 0 < IL < n ; rehash whole 64B each time
    let mut i = hmac512(label, seed64);
    loop {
        let mut il = [0u8; 32];
        let mut ir = [0u8; 32];
        il.copy_from_slice(&i[..32]);
        ir.copy_from_slice(&i[32..]);

        let ok = {
            // 0 < IL < n
            let zero = il.iter().all(|&b| b == 0);
            let ge_n = {
                const N: [u8;32] = [0x7a,0xf2,0x59,0x9b,0x3b,0x3f,0x22,0xd0,0x56,0x3f,0xbf,0x0f,0x99,0x0a,0x37,0xb5,
                                    0x32,0x7a,0xa7,0x23,0x30,0x15,0x77,0x22,0xd4,0x43,0x62,0x3e,0xae,0xd4,0xac,0xcf];
                let mut ge = false;
                for k in 0..32 {
                    if il[k] != N[k] { ge = il[k] > N[k]; break; }
                }
                ge || il == N
            };
            !zero && !ge_n
        };

        if ok { return (il, ir); }
        i = hmac512(label, &i);
    }
}

fn ser97_hex_of_priv(sk_be: &[u8; 32]) -> String {
    let pk = cheetah_pub_from_sk(*sk_be);
    let ser = ser_a_pt(&pk);
    to_hex(&ser)
}

fn probe_master_variants(seed64: &[u8;64], known_hex: &str) {
    let known = known_hex.replace(char::is_whitespace, "").to_ascii_lowercase();

    const LABELS: &[(&str, &[u8])] = &[
        ("Nockchain seed", b"Nockchain seed"),
        ("Nockchain seed", b"dees niahckcoN"),
    ];

    println!("-- Probing master KDF variants --");
    for (lname, lbytes) in LABELS {
        // Variant A: single pass (no rehash even if IL >= n)
        let (il0, ir0) = master_once(lbytes, seed64);
        let got0 = ser97_hex_of_priv(&il0);
        let tag0 = format!("label='{}', mode=single-pass", lname);
        println!("{}: pk = {}", tag0, got0);
        if got0.eq_ignore_ascii_case(&known) {
            println!("🎯 MATCH: {}", tag0);
            return;
        }

        // Variant B: rehash-until-valid (your current behavior for Nockchain)
        let (il1, ir1) = master_with_rehash(lbytes, seed64);
        let got1 = ser97_hex_of_priv(&il1);
        let tag1 = format!("label='{}', mode=rehash-until-valid", lname);
        println!("{}: pk = {}", tag1, got1);
        if got1.eq_ignore_ascii_case(&known) {
            println!("🎯 MATCH: {}", tag1);
            return;
        }

        // (Optional) print the chain codes so you can compare too
        println!("{}: IR(single) = {}", lname, hex::encode(ir0));
        println!("{}: IR(rehash) = {}", lname, hex::encode(ir1));
    }
    println!("⛔ No master-KDF variant matched the known-good.");
}
