#![cfg_attr(not(feature = "std"), no_std)]

pub use tx_types::crypto::cheetah::{
    cheetah_pub_from_sk, hmac_split_512, master_from_seed, schnorr_sign_tx, ser_a_pt,
    ser_a_pt_rep104, xprv_derive_child, xpub_derive_child, Hash, T8, XKey,
};

#[cfg(feature = "std")]
pub use tx_types::crypto::slip10::ExtendedKey;

#[cfg(feature = "std")]
pub fn bip39_to_seed(
    mnemonic: &str,
    passphrase: &str,
) -> Result<[u8; 64], tx_types::crypto::slip10::CryptoError> {
    tx_types::crypto::slip10::bip39_to_seed(mnemonic, passphrase)
}

#[cfg(feature = "std")]
pub fn master_from_mnemonic(mnemonic: &str, passphrase: &str) -> ([u8; 32], [u8; 32]) {
    let ext = tx_types::crypto::slip10::master_from_mnemonic(mnemonic, passphrase)
        .expect("master_from_mnemonic");
    let sk = ext
        .private_key_bytes()
        .expect("derived key has private component");
    (sk, ext.chain_code())
}
