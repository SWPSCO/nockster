use bip39::{Language, Mnemonic};
use getrandom::getrandom;
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use zeroize::{Zeroize, Zeroizing};

use crate::js_error::js_error;
use tx_types::{
    crypto::{cheetah::point::cheetah_pub_from_sk, utils::be32_atom_to_t8_le},
    transaction_types::{Hash, SchnorrPubkey, F6LT, T8},
};

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Wallet {
    pub public_key: String,
    pub extended_public_key: Option<String>,
    pub private_key: String,
    pub extended_private_key: Option<String>,
    pub chain_code: Option<[u8; 32]>,
    pub depth: Option<u8>,
    pub index: Option<u32>,
    pub parent_fingerprint: Option<[u8; 4]>,
    pub version: Option<u8>,
    pub seedphrase: Option<Vec<String>>,
}

impl std::fmt::Debug for Wallet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wallet")
            .field("public_key", &self.public_key)
            .finish_non_exhaustive()
    }
}

impl Drop for Wallet {
    fn drop(&mut self) {
        self.private_key.zeroize();
        self.extended_private_key.zeroize();
        self.chain_code.zeroize();
        self.seedphrase.zeroize();
    }
}

impl Wallet {
    pub fn from_key(key: &str) -> Result<Self, JsValue> {
        let value = key.trim().to_string();
        if value.starts_with("zprv") {
            Wallet::from_extended_key(value)
        } else {
            Wallet::from_seedphrase(value)
        }
    }

    /// Construct wallet from extended private key
    pub fn from_extended_key(key: String) -> Result<Self, JsValue> {
        if !key.starts_with("zprv") {
            return Err(js_error("unsupported key!"));
        }

        let decoded = tx_types::crypto::slip10::ExtendedKey::from_extended_key_string(&key)
            .map_err(|e| js_error(&format!("invalid extended private key: {e}")))?;

        let public_key = decoded.public_key.to_schnorr_pubkey().to_hash().to_b58();

        let extended_public_key = decoded
            .to_zpub_string()
            .map_err(|e| js_error(&format!("invalid extended public key: {e}")))?;

        let private_key = decoded
            .private_key
            .map(|pk| bs58::encode(pk).into_string())
            .ok_or_else(|| js_error("no private key"))?;

        Ok(Self {
            public_key,
            //: "to_hash() doesn't work".to_string(),
            extended_public_key: Some(extended_public_key),
            private_key,
            extended_private_key: Some(key),
            chain_code: Some(decoded.chain_code),
            depth: Some(decoded.depth),
            index: Some(decoded.index),
            parent_fingerprint: Some(decoded.parent_fingerprint),
            version: Some(decoded.version),
            seedphrase: None, // no way to derive from zprv
        })
    }

    /// Construct wallet from seedphrase
    pub fn from_seedphrase(key: String) -> Result<Self, JsValue> {
        let trimmed = key.trim();
        let mnemonic = Mnemonic::parse_in_normalized(Language::English, trimmed)
            .map_err(|e| js_error(&format!("invalid seed phrase: {e}")))?;
        let words: Vec<String> = mnemonic.words().map(|word| word.to_string()).collect();
        if words.len() != 24 {
            return Err(js_error(&format!(
                "expected 24 mnemonic words, received {}",
                words.len()
            )));
        }

        let seed = mnemonic.to_seed("");
        let extended = tx_types::crypto::slip10::ExtendedKey::from_seed(&seed, 1).map_err(|e| {
            js_error(&format!(
                "failed to derive extended key from seed phrase: {e}"
            ))
        })?;

        let extended_private_key = extended
            .to_zprv_string()
            .map_err(|e| js_error(&format!("failed to encode extended private key: {e}")))?;

        let extended_public_key = extended
            .to_zpub_string()
            .map_err(|e| js_error(&format!("failed to encode extended public key: {e}")))?;

        let private_key = extended
            .private_key_bytes()
            .map(|pk| bs58::encode(pk).into_string())
            .ok_or_else(|| js_error("extended key is missing a private key"))?;

        let public_key = extended.public_key.to_schnorr_pubkey().to_hash().to_b58();

        Ok(Self {
            public_key,
            extended_public_key: Some(extended_public_key),
            private_key,
            extended_private_key: Some(extended_private_key),
            chain_code: Some(extended.chain_code),
            depth: Some(extended.depth),
            index: Some(extended.index),
            parent_fingerprint: Some(extended.parent_fingerprint),
            version: Some(extended.version),
            seedphrase: Some(words),
        })
    }

    /// Generate a 24-word mnemonic from 256 bits of secure randomness.
    pub fn generate_key() -> Result<Vec<String>, JsValue> {
        let mut entropy = Zeroizing::new([0u8; 32]);
        getrandom(&mut entropy[..]).map_err(|e| js_error(&format!("entropy rng: {e}")))?;

        let memo = Mnemonic::from_entropy_in(Language::English, &entropy[..])
            .map_err(|e| js_error(&format!("mnemonic: {e}")))?;

        let words: Vec<String> = memo.words().map(|word| word.to_string()).collect();
        if words.len() != 24 {
            return Err(js_error(&format!(
                "expected 24 mnemonic words, received {}",
                words.len()
            )));
        }

        Ok(words)
    }

    pub fn private_key_t8(&self) -> Result<T8, String> {
        let key_bytes = decode_private_key_bytes(&self.private_key)?;
        Ok(be32_atom_to_t8_le(&key_bytes))
    }

    pub fn schnorr_pubkey(&self) -> Result<SchnorrPubkey, String> {
        let key_bytes = decode_private_key_bytes(&self.private_key)?;
        let coords = cheetah_pub_from_sk(key_bytes);
        Ok(SchnorrPubkey {
            x: F6LT { values: coords[0] },
            y: F6LT { values: coords[1] },
            inf: false,
        })
    }

    pub fn public_key_hash(&self) -> Result<Hash, String> {
        Hash::from_b58(&self.public_key).map_err(|e| format!("invalid wallet address: {e}"))
    }
}

fn decode_private_key_bytes(value: &str) -> Result<[u8; 32], String> {
    if value.is_empty() {
        return Err("wallet is missing a private key".to_string());
    }
    let bytes = bs58::decode(value)
        .into_vec()
        .map_err(|e| format!("failed to decode private key: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!(
            "private key must be 32 bytes for signing, got {}",
            bytes.len()
        ));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_signing_wallet_round_trips_without_hd_fields() {
        let mut scalar = [0u8; 32];
        scalar[31] = 1;
        let wallet: Wallet = serde_json::from_value(serde_json::json!({
            "publicKey": "stored address",
            "privateKey": bs58::encode(scalar).into_string()
        }))
        .unwrap();
        let address = wallet.schnorr_pubkey().unwrap().to_hash().to_b58();
        let restored: Wallet =
            serde_json::from_str(&serde_json::to_string(&wallet).unwrap()).unwrap();
        assert_eq!(
            restored.schnorr_pubkey().unwrap().to_hash().to_b58(),
            address
        );
        assert_eq!(
            restored.private_key_t8().unwrap(),
            wallet.private_key_t8().unwrap()
        );
        assert!(restored.seedphrase.is_none());
        assert!(restored.extended_private_key.is_none());
    }
}
