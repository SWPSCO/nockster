#![cfg_attr(not(feature = "std"), no_std)]

// For no_std builds, re-export from cheetah_nostd
#[cfg(not(feature = "std"))]
pub use tx_types::crypto::{
    cheetah_pub_from_sk, hmac_split_512, master_from_seed, schnorr_sign_digest, schnorr_sign_tx,
    ser_a_pt, ser_a_pt_rep104, xprv_derive_child, xpub_derive_child, CheetahPoint, F6lt, Hash,
    XKey, T8,
};

// For std builds, use the new modular API with compatibility wrappers
#[cfg(feature = "std")]
pub use tx_types::crypto::{cheetah_pub_from_sk, schnorr_sign_digest, ExtendedKey, CryptoError};

#[cfg(feature = "std")]
pub use tx_types::crypto::cheetah::{CheetahPoint, F6Element};

// Re-export Hash and T8 from transaction_types for std builds
#[cfg(feature = "std")]
pub use tx_types::transaction_types::{Hash, T8};

// Compatibility wrapper: XKey struct for std builds
#[cfg(feature = "std")]
#[derive(Clone)]
pub struct XKey {
    pub depth: u8,
    pub index: u32,
    pub chain_code: [u8; 32],
    pub sk: Option<[u8; 32]>,
    pub pk: Option<([u64; 6], [u64; 6])>,
    pub parent_fingerprint: [u8; 4],
}

#[cfg(feature = "std")]
impl XKey {
    pub fn from_master(sk: [u8; 32], chain_code: [u8; 32]) -> Self {
        let pk_arr = cheetah_pub_from_sk(sk);
        // Convert [[u64; 6]; 2] to ([u64; 6], [u64; 6])
        let pk_tuple = (pk_arr[0], pk_arr[1]);
        XKey {
            depth: 0,
            index: 0,
            chain_code,
            sk: Some(sk),
            pk: Some(pk_tuple),
            parent_fingerprint: [0u8; 4],
        }
    }

    pub fn from_extended_key(ext: &ExtendedKey) -> Self {
        let pk_coords = ext.public_key.to_coordinates();
        // Convert [[u64; 6]; 2] to ([u64; 6], [u64; 6])
        let pk_tuple = (pk_coords[0], pk_coords[1]);
        XKey {
            depth: ext.depth,
            index: ext.index,
            chain_code: ext.chain_code,
            sk: ext.private_key,
            pk: Some(pk_tuple),
            parent_fingerprint: ext.parent_fingerprint,
        }
    }
}

// master_from_seed for std builds - wraps the new API
#[cfg(feature = "std")]
pub fn master_from_seed(seed64: &[u8]) -> ([u8; 32], [u8; 32]) {
    let ext = tx_types::crypto::slip10::master_from_seed(seed64)
        .expect("master_from_seed");
    let sk = ext.private_key_bytes().expect("master has private key");
    (sk, ext.chain_code)
}

// xprv_derive_child for std builds - wraps ExtendedKey::derive_child
#[cfg(feature = "std")]
pub fn xprv_derive_child(parent: &XKey, index: u32) -> XKey {
    // Convert XKey to ExtendedKey
    let pk = parent.pk.expect("need pk");
    // Convert tuple to array format
    let pk_arr: [[u64; 6]; 2] = [pk.0, pk.1];
    let mut ext = ExtendedKey {
        private_key: parent.sk,
        public_key: CheetahPoint::from_coordinates(pk_arr),
        chain_code: parent.chain_code,
        depth: parent.depth,
        index: parent.index,
        parent_fingerprint: parent.parent_fingerprint,
        version: 0,
    };

    let child = ext.derive_child(index).expect("derivation failed");
    XKey::from_extended_key(&child)
}

// ser_a_pt for std builds - serializes a point to 97 bytes
#[cfg(feature = "std")]
pub fn ser_a_pt(pk: &([u64; 6], [u64; 6])) -> [u8; 97] {
    let mut out = [0u8; 97];
    out[0] = 0x01;
    // Y coordinates (big-endian, limb by limb)
    for (i, &limb) in pk.1.iter().rev().enumerate() {
        out[1 + i * 8..1 + (i + 1) * 8].copy_from_slice(&limb.to_be_bytes());
    }
    // X coordinates (big-endian, limb by limb)
    for (i, &limb) in pk.0.iter().rev().enumerate() {
        out[49 + i * 8..49 + (i + 1) * 8].copy_from_slice(&limb.to_be_bytes());
    }
    out
}

// ser_a_pt_rep104 for std builds - serializes a point to 104 bytes with sentinel
#[cfg(feature = "std")]
pub fn ser_a_pt_rep104(pk: &([u64; 6], [u64; 6])) -> [u8; 104] {
    let mut out = [0u8; 104];
    // 8-byte sentinel (0x01 followed by zeros)
    out[0] = 0x01;
    // Y coordinates (big-endian, limb by limb)
    for (i, &limb) in pk.1.iter().rev().enumerate() {
        out[8 + i * 8..8 + (i + 1) * 8].copy_from_slice(&limb.to_be_bytes());
    }
    // X coordinates (big-endian, limb by limb)
    for (i, &limb) in pk.0.iter().rev().enumerate() {
        out[56 + i * 8..56 + (i + 1) * 8].copy_from_slice(&limb.to_be_bytes());
    }
    out
}

// schnorr_sign_tx for std builds - wraps schnorr_sign_digest with type conversions
#[cfg(feature = "std")]
pub fn schnorr_sign_tx(sk_be: [u8; 32], pk: ([u64; 6], [u64; 6]), m5: [u64; 5]) -> (T8, T8) {
    use tx_types::transaction_types::{SchnorrPubkey, F6LT};

    // Convert sk_be [u8; 32] to T8 format (8x u32 in u64)
    let sk_t8 = be32_to_t8(&sk_be);

    // Convert pk tuple to SchnorrPubkey
    let pubkey = SchnorrPubkey {
        x: F6LT { values: pk.0 },
        y: F6LT { values: pk.1 },
        inf: false,
    };

    // Convert m5 [u64; 5] to Hash
    let message = Hash { values: m5 };

    schnorr_sign_digest(sk_t8, pubkey, message)
}

// Helper: convert 32-byte big-endian to T8 format
#[cfg(feature = "std")]
fn be32_to_t8(be: &[u8; 32]) -> T8 {
    // Reverse to little-endian
    let mut le = [0u8; 32];
    for i in 0..32 {
        le[i] = be[31 - i];
    }

    // Pack into 8x u32 values stored in u64
    let mut values = [0u64; 8];
    for i in 0..8 {
        values[i] = u32::from_le_bytes([le[i * 4], le[i * 4 + 1], le[i * 4 + 2], le[i * 4 + 3]]) as u64;
    }
    T8 { values }
}

#[cfg(feature = "std")]
pub fn bip39_to_seed(
    mnemonic: &str,
    passphrase: &str,
) -> Result<[u8; 64], tx_types::crypto::CryptoError> {
    tx_types::crypto::slip10::bip39_to_seed(mnemonic, passphrase)
}

#[cfg(feature = "std")]
pub fn master_from_mnemonic(mnemonic: &str, passphrase: &str) -> ([u8; 32], [u8; 32]) {
    let ext = tx_types::crypto::slip10::master_from_mnemonic(mnemonic, passphrase)
        .expect("master_from_mnemonic");
    let sk = ext
        .private_key_bytes()
        .expect("derived key has private component");
    (sk, ext.chain_code)
}

// Re-export utils module for tests
#[cfg(feature = "std")]
pub mod utils {
    pub use tx_types::crypto::utils::*;
}

// test_tip5_hash_words for testing TIP5 hashing
#[cfg(feature = "std")]
pub fn test_tip5_hash_words(words: &[u64]) -> [u64; 5] {
    use tx_types::hashing::tip5::Tip5Hasher;
    use nockapp::noun::slab::NounSlab;
    use nockvm::noun::{Atom, Cell};

    let mut slab: NounSlab = NounSlab::new();
    let mut list = Atom::new(&mut slab, 0u64).as_noun();
    for &w in words.iter().rev() {
        let atom = Atom::new(&mut slab, w).as_noun();
        list = Cell::new(&mut slab, atom, list).as_noun();
    }

    let hash = Tip5Hasher::hash_varlen(list).expect("hash");
    hash.values
}
