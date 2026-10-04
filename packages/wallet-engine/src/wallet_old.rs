use wasm_bindgen::{
    JsValue,
    prelude::*,
};
use serde::{Deserialize, Serialize};
use argon2::{Algorithm, Argon2, Params, Version};
use getrandom::getrandom;
use log::*;
use bip39::{Language, Mnemonic};
use bytes::{Bytes, BytesMut};
use std::collections::HashMap;
use nockapp::noun::slab::NounSlab;
use noun_serde::{NounDecode, NounEncode};

// use zeroize::Zeroize;

use crate::js_error::*;

use tx_types::{
    transaction_types::{T8, SchnorrPubkey, F6LT},
    crypto::cheetah::point::cheetah_pub_from_sk,
};

#[wasm_bindgen]
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Wallet {
    seed_phrase: String,
    chain_code: String,
    private_key: String,
    master_public_key: String,
    // Use String keys so serde_wasm_bindgen can serialize it correctly
    derived_public_keys: HashMap<String, String>,
}


#[wasm_bindgen]
impl Wallet {
    /// Create a new wallet
    pub fn random_wallet() -> Result<Self, JsValue> {
        let key = Self::argon2id()?;

        info!("argon2id key: {:?}", key.as_ref());

        let memo = Mnemonic::from_entropy_in(Language::English, &key[..])
            .map_err(|e| js_error(&format!("mnemonic: {e}")))?;

        Self::wallet_from_memo(memo)
    }

    /// Generate a wallet from a seed phrase
    pub fn from_seed_phrase(seed_phrase: String) -> Result<Self, JsValue> {
        let memo = Mnemonic::parse_in(Language::English, &seed_phrase)
            .map_err(|e| js_error(&format!("mnemonic: {e}")))?;
        Self::wallet_from_memo(memo)
    }

    /// Convert the wallet to a JavaScript object
    pub fn to_js(&self) -> Result<JsValue, JsValue> {
        info!("Converting wallet to JS. Derived keys count: {}", self.derived_public_keys.len());
        for (k, v) in &self.derived_public_keys {
            info!("  Derived key {}: {}", k, v);
        }

        // Manually construct JS object because serde_wasm_bindgen doesn't handle HashMap well
        let obj = js_sys::Object::new();

        js_sys::Reflect::set(&obj, &"seedPhrase".into(), &self.seed_phrase.clone().into())?;
        js_sys::Reflect::set(&obj, &"chainCode".into(), &self.chain_code.clone().into())?;
        js_sys::Reflect::set(&obj, &"privateKey".into(), &self.private_key.clone().into())?;
        js_sys::Reflect::set(&obj, &"masterPublicKey".into(), &self.master_public_key.clone().into())?;

        // Convert derived keys HashMap to JS object
        let derived_keys_obj = js_sys::Object::new();
        for (k, v) in &self.derived_public_keys {
            js_sys::Reflect::set(&derived_keys_obj, &k.clone().into(), &v.clone().into())?;
        }
        js_sys::Reflect::set(&obj, &"derivedPublicKeys".into(), &derived_keys_obj)?;

        info!("Serialization complete");
        Ok(obj.into())
    }
    
    /// Get the seed phrase
    pub fn get_seed_phrase(&self) -> String {
        self.seed_phrase.clone()
    }
    
    /// Get the chain code
    pub fn get_chain_code(&self) -> String {
        self.chain_code.clone()
    }
    
    /// Get the private key
    pub fn get_private_key(&self) -> String {
        self.private_key.clone()
    }
    
    /// Get the master public key
    pub fn get_master_public_key(&self) -> String {
        self.master_public_key.clone()
    }
    
    /// Get the derived public keys as a JavaScript object with string keys
    /// Returns an object like: { "0": "key1", "1": "key2", "2": "key3" }
    pub fn get_derived_public_keys(&self) -> Result<JsValue, JsValue> {
        // Already using HashMap<String, String> so can serialize directly
        serde_wasm_bindgen::to_value(&self.derived_public_keys)
            .map_err(|e| js_error(&format!("Failed to serialize derived keys: {}", e)))
    }

    /// Generate a wallet from a bip39 seed
    fn wallet_from_memo(memo: Mnemonic) -> Result<Self, JsValue> {
        let seed_phrase = memo.to_string();

        let seed = memo.to_seed("");

        let extended_key = tx_types::crypto::master_from_seed(&seed)
            .map_err(|e| js_error(&format!("master key generation: {}", e)))?;

        let sk = extended_key.private_key
            .ok_or_else(|| js_error("private key not available"))?;
        let cc = extended_key.chain_code;

        // Generate public key coordinates using tx-types
        let coords = tx_types::crypto::cheetah::point::cheetah_pub_from_sk(sk);
        let (x, y) = (coords[0], coords[1]);

        let pk = tx_types::transaction_types::SchnorrPubkey{
            x: tx_types::transaction_types::F6LT{ values: x },
            y: tx_types::transaction_types::F6LT{ values: y },
            inf: false,
        };

        let chain_code = bs58::encode(cc).into_string();
        let private_key = bs58::encode(sk).into_string();
        let master_public_key = pk.to_b58();

        // Generate a few derived keys for testing/demo purposes
        let mut derived_public_keys = HashMap::new();
        info!("Starting to derive child keys...");
        for i in 0..3 {
            // Derive child key using BIP32-like derivation
            // For now, we'll generate mock derived keys by hashing master key + index
            // In production, this should use proper BIP32/BIP44 derivation
            info!("Deriving child key at index {}", i);
            match Self::derive_child_key(&seed, i) {
                Ok(child_pk) => {
                    info!("Successfully derived child key at index {}: {}", i, child_pk);
                    derived_public_keys.insert(i.to_string(), child_pk);
                }
                Err(e) => {
                    info!("Warning: Failed to derive key at index {}: {:?}", i, e);
                }
            }
        }
        info!("Finished deriving child keys. Total: {}", derived_public_keys.len());

        Ok(Wallet {
            seed_phrase,
            chain_code,
            private_key,
            master_public_key,
            derived_public_keys,
        })
    }


    /// Derive a child key from seed at given index
    /// TODO: This is a simplified derivation for testing. In production, this should use
    /// proper BIP32/BIP44 derivation with the tx-types crypto module
    fn derive_child_key(seed: &[u8], index: u8) -> Result<String, JsValue> {
        use argon2::Argon2;

        // Use Argon2id to derive a child key from seed + index
        // This ensures we get proper 32-byte keys suitable for elliptic curve operations
        let params = Params::new(262144, 5, 1, Some(32))
            .map_err(|e| js_error(&format!("argon2 params for child key: {e}")))?;
        let a2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        // Use index as salt (padded to 16 bytes)
        let mut salt = [0u8; 16];
        salt[0] = index;

        // Derive 32 bytes from the seed
        let mut child_key_bytes = [0u8; 32];
        a2.hash_password_into(seed, &salt, &mut child_key_bytes)
            .map_err(|e| js_error(&format!("argon2 child key derivation: {e}")))?;

        // Derive the public key from the private key
        let pk_coords = cheetah_pub_from_sk(child_key_bytes);
        let (x, y) = (pk_coords[0], pk_coords[1]);

        let pk = SchnorrPubkey {
            x: F6LT { values: x },
            y: F6LT { values: y },
            inf: false,
        };

        Ok(pk.to_b58())
    }

    /// Generate a 32-byte key using Argon2id (v=0x13)
    fn argon2id(
    ) -> Result<Bytes, JsValue> {
        // default values
        let m_cost_kib = 262144; // 256MiB
        let t_cost = 5;
        // p=1 for MV3 safety
        let p = 1;

        // Generate 16 bytes of salt
        let mut salt = BytesMut::zeroed(16);
        getrandom(&mut salt).map_err(|e| js_error(&format!("argon2 salt rng: {e}")))?;

        // Generate 32 bytes of entropy
        let mut entropy = BytesMut::zeroed(32);
        getrandom(&mut entropy).map_err(|e| js_error(&format!("argon2 entropy rng: {e}")))?;

        let params = Params::new(m_cost_kib, t_cost, p, Some(32))
            .map_err(|e| js_error(&format!("argon2 params: {e}")))?;
        let a2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

        let mut key = BytesMut::zeroed(32);
        a2.hash_password_into(&entropy, &salt, &mut key)
            .map_err(|e| js_error(&format!("argon2: {e}")))?;

        // debug
        // let key = Bytes::from_static(&[156, 229, 76, 78, 102, 207, 122, 66, 240, 212, 54, 97, 179, 95, 219, 13, 44, 42, 232, 124, 5, 80, 100, 130, 52, 103, 150, 111, 185, 119, 146, 218]);

        Ok(key.into())
    }
}

// Non-WASM methods. I think these should all end up in the WASM impl, once we figure out the
// tx-types WASM compatibility and Wallet HashMap issue.
impl Wallet {
    /// Create a wallet from imported data (used by import_wallet_file)
    pub fn from_imported_data(
        seed_phrase: String,
        chain_code: String,
        private_key: Option<String>,
        master_public_key: String,
        derived_public_keys: HashMap<String, String>,
    ) -> Result<Self, JsValue> {
        Ok(Wallet {
            seed_phrase,
            chain_code,
            private_key: private_key.unwrap_or_else(|| String::new()),
            master_public_key,
            derived_public_keys,
        })
    }

    /// Get the derived public keys as a HashMap (for internal use)
    pub fn get_derived_public_keys_map(&self) -> &HashMap<String, String> {
        &self.derived_public_keys
    }
    
    /// Create a wallet with exactly 8-byte values for testing with simplified wallet.rs
    /// This is a workaround for the incomplete wallet noun-serde implementation
    pub fn random_wallet_8byte() -> Result<Self, JsValue> {
        // Generate 8 bytes of random data for each field, but limit to smaller values
        // to avoid exceeding DIRECT_MAX in the noun system
        let mut seed_phrase_bytes = [0u8; 8];
        let mut chain_code_bytes = [0u8; 8];
        let mut private_key_bytes = [0u8; 8];
        let mut master_public_key_bytes = [0u8; 8];
        
        getrandom(&mut seed_phrase_bytes).map_err(|e| js_error(&format!("rng error: {e}")))?;
        getrandom(&mut chain_code_bytes).map_err(|e| js_error(&format!("rng error: {e}")))?;
        getrandom(&mut private_key_bytes).map_err(|e| js_error(&format!("rng error: {e}")))?;
        getrandom(&mut master_public_key_bytes).map_err(|e| js_error(&format!("rng error: {e}")))?;
        
        // Limit the values to avoid noun system limits by clearing the top bits
        // This ensures the u64 values are smaller while still being 8 bytes when encoded
        seed_phrase_bytes[0] &= 0x0F;  // Clear top 4 bits
        chain_code_bytes[0] &= 0x0F;
        private_key_bytes[0] &= 0x0F;
        master_public_key_bytes[0] &= 0x0F;
        
        // Encode as base58 strings
        let seed_phrase = bs58::encode(seed_phrase_bytes).into_string();
        let chain_code = bs58::encode(chain_code_bytes).into_string();
        let private_key = bs58::encode(private_key_bytes).into_string();
        let master_public_key = bs58::encode(master_public_key_bytes).into_string();
        
        // Generate a few derived public keys (also 8 bytes each)
        let mut derived_public_keys = HashMap::new();
        for i in 0..3 {
            let mut derived_key_bytes = [0u8; 8];
            getrandom(&mut derived_key_bytes).map_err(|e| js_error(&format!("rng error: {e}")))?;
            derived_key_bytes[0] &= 0x0F;  // Limit value to avoid noun system limits
            let derived_key = bs58::encode(derived_key_bytes).into_string();
            derived_public_keys.insert(i.to_string(), derived_key);
        }
        
        Ok(Wallet {
            seed_phrase,
            chain_code,
            private_key,
            master_public_key,
            derived_public_keys,
        })
    }
    
    /// Get the private key in T8 format for transaction signing
    pub fn get_private_key_as_t8(&self) -> Result<T8, String> {
        if self.private_key.is_empty() {
            return Err("Wallet has no private key".to_string());
        }
        
        // Decode from base58
        let key_bytes = bs58::decode(&self.private_key)
            .into_vec()
            .map_err(|e| format!("Failed to decode private key: {}", e))?;
        
        // For real cryptographic keys, we expect 32 bytes
        if key_bytes.len() != 32 {
            return Err(format!("Private key must be 32 bytes for cryptographic operations, got {} bytes", key_bytes.len()));
        }
        
        // Convert 32-byte private key to T8 format (8 x u32 values stored as u64)
        // T8 represents 8 limbs of 32 bits each, stored in little-endian order
        let mut values = [0u64; 8];
        for i in 0..8 {
            let start_idx = i * 4;
            let end_idx = start_idx + 4;
            if end_idx <= key_bytes.len() {
                // Convert 4 bytes to u32, then to u64
                let limb_bytes = &key_bytes[start_idx..end_idx];
                let limb = u32::from_le_bytes([limb_bytes[0], limb_bytes[1], limb_bytes[2], limb_bytes[3]]);
                values[i] = limb as u64;
            }
        }
        
        Ok(T8 { values })
    }
    
    /// Get the public key as SchnorrPubkey format for transaction signing
    /// This derives the actual elliptic curve point from the private key using Cheetah curve operations
    /// Get wallet keys as T8 and SchnorrPubkey
    pub fn get_keys(&self) -> Result<(T8, SchnorrPubkey), String> {
        let privkey = T8::from_b58(&self.private_key)
            .map_err(|e| format!("Failed to parse private key: {}", e))?;
        let pubkey = self.get_pubkey_as_schnorr_pubkey()?;
        Ok((privkey, pubkey))
    }

    pub fn get_pubkey_as_schnorr_pubkey(&self) -> Result<SchnorrPubkey, String> {
        if self.private_key.is_empty() {
            return Err("Wallet has no private key".to_string());
        }
        
        // Decode private key from base58
        let key_bytes = bs58::decode(&self.private_key)
            .into_vec()
            .map_err(|e| format!("Failed to decode private key: {}", e))?;
        
        if key_bytes.len() != 32 {
            return Err(format!("Private key must be 32 bytes, got {} bytes", key_bytes.len()));
        }
        
        // Convert to 32-byte array for curve operations
        let mut private_key_32 = [0u8; 32];
        private_key_32.copy_from_slice(&key_bytes);
        
        // Use the Cheetah curve implementation to derive the public key point
        // This uses the same curve operations that tx-types uses internally
        let pk_coords = cheetah_pub_from_sk(private_key_32);
        
        // The function returns [[u64; 6]; 2] where [0] is X and [1] is Y coordinates
        let x_values = pk_coords[0];
        let y_values = pk_coords[1];
        
        Ok(SchnorrPubkey {
            x: F6LT { values: x_values },
            y: F6LT { values: y_values },
            inf: false,
        })
    }
    
    /// Core function to import wallet from jam data (Rust-only, no WASM bindings)
    pub fn import_from_jam(jam_data: &[u8]) -> Result<Self, String> {
        info!("Importing wallet from jam file, {} bytes", jam_data.len());
        
        // Convert the byte array to Bytes
        let bytes = Bytes::copy_from_slice(jam_data);
        
        // Create a new NounSlab to work with
        let mut slab: NounSlab = NounSlab::new();
        
        // Cue the jammed data into a Noun
        let wallet_noun = slab.cue_into(bytes)
            .map_err(|e| format!("Failed to deserialize jam data: {}", e))?;
        
        info!("Successfully cued wallet data into noun format");
        
        // Parse the noun as a list of (trek, meta) pairs - this is what do-export-keys exports
        type TrekMetaPair = (noun_serde::wallet::Trek, noun_serde::wallet::Meta);
        let keys_list: Vec<TrekMetaPair> = Vec::from_noun(&wallet_noun)
            .map_err(|e| format!("Failed to parse keys list: {}", e))?;
        
        info!("Successfully parsed {} key entries", keys_list.len());
        
        // Find the master key and seed phrase from the imported keys
        let mut master_public_key: Option<String> = None;
        let mut master_private_key: Option<String> = None;
        let mut chain_code: Option<String> = None;
        let mut seed_phrase: Option<String> = None;
        let mut derived_public_keys: HashMap<String, String> = HashMap::new();

        for (trek, meta) in &keys_list {
            let path_parts = &trek.0;
            info!("Processing path: {:?}", path_parts);

            // MASTER COILS: /keys/<master>/pub|prv/m 
            if path_parts.len() >= 4 && path_parts[0] == "keys" && path_parts[3] == "m" {
                if let noun_serde::wallet::Meta::Coil(coil) = meta {
                    let key_bytes = match &coil.key {
                        noun_serde::wallet::Key::Pub(k) => k.to_be_bytes().to_vec(),
                        noun_serde::wallet::Key::Prv(k) => k.to_be_bytes().to_vec(),
                    };
                    let cc_bytes = coil.knot.to_be_bytes().to_vec();
                    let key_b58 = bs58::encode(&key_bytes).into_string();
                    let cc_b58  = bs58::encode(&cc_bytes).into_string();

                    match &coil.key {
                        noun_serde::wallet::Key::Pub(_) => {
                            master_public_key = Some(key_b58);
                            chain_code = Some(cc_b58);
                        }
                        noun_serde::wallet::Key::Prv(_) => {
                            master_private_key = Some(key_b58);
                            if chain_code.is_none() {
                                chain_code = Some(cc_b58);
                            }
                        }
                    }
                }
            }

            // SEED LABEL: /keys/<master>/seed/<label>
            if path_parts.len() >= 3 && path_parts[0] == "keys" && path_parts[2] == "seed" {
                if let noun_serde::wallet::Meta::Label(phrase) = meta {
                    seed_phrase = Some(phrase.clone());
                }
            }

            // DERIVED PUB KEYS: /keys/<master>/pub/<index>/<coil>
            // Only accept numeric indices as derived/skip m
            if path_parts.len() >= 4 && path_parts[0] == "keys" && path_parts[2] == "pub" {
                let idx = &path_parts[3];
                if idx.chars().all(|c| c.is_ascii_digit()) {
                    if let noun_serde::wallet::Meta::Coil(coil) = meta {
                        if let noun_serde::wallet::Key::Pub(_) = &coil.key {
                            let key_bytes = match &coil.key {
                                noun_serde::wallet::Key::Pub(k) => k.to_be_bytes().to_vec(),
                                noun_serde::wallet::Key::Prv(k) => k.to_be_bytes().to_vec(),
                            };
                            let key_b58 = bs58::encode(&key_bytes).into_string();
                            info!("Found derived public key at index {}: {}", idx, key_b58);
                            derived_public_keys.insert(idx.clone(), key_b58);
                        }
                    }
                } else {
                    // debug
                    info!("Skipping non-numeric pub index {:?} at path {:?}", idx, path_parts);
                }
            }
        }

        // fallback if master wasn’t picked up
        if master_public_key.is_none() || chain_code.is_none() {
            // Try to locate a pub/m coil directly
            if let Some((_, noun_serde::wallet::Meta::Coil(coil))) =
                keys_list.iter().find(|(t, m)| {
                    t.0.len() >= 4
                        && t.0[0] == "keys"
                        && t.0[2] == "pub"
                        && t.0[3] == "m"
                        && matches!(m, noun_serde::wallet::Meta::Coil(_))
                })
            {
                let key_bytes = match &coil.key {
                    noun_serde::wallet::Key::Pub(k) => k.to_be_bytes().to_vec(),
                    noun_serde::wallet::Key::Prv(k) => k.to_be_bytes().to_vec(),
                };
                let cc_bytes = coil.knot.to_be_bytes().to_vec();
                master_public_key = Some(bs58::encode(&key_bytes).into_string());
                chain_code       = Some(bs58::encode(&cc_bytes).into_string());
                info!("Recovered master pub + chain code via fallback.");
            }
        }

        let master_pub = master_public_key.ok_or_else(|| "No master public key found in imported wallet".to_string())?;
        let cc = chain_code.ok_or_else(|| "No chain code found in imported wallet".to_string())?;
        let phrase = seed_phrase.unwrap_or_else(|| "imported-wallet".to_string());

        info!("Found master public key: {}", master_pub);
        info!("Found chain code: {}", cc);
        info!("Found seed phrase: {}", phrase);
        info!("Found {} derived public keys", derived_public_keys.len());
   
        // Create a new Wallet struct with the imported data
        let wallet = Self::from_imported_data(
            phrase,
            cc,
            master_private_key,
            master_pub,
            derived_public_keys,
        ).map_err(|e| format!("Failed to create wallet: {:?}", e))?;
        
        Ok(wallet)
    }
    
    /// Core function to export wallet to jam data (Rust-only, no WASM bindings)
    //TODO we are working here with u64 keys, chain code, etc, because that is how the 
    //current wallet noun-serde implementation works. this will need to be updated when
    //it is fixed.
    pub fn export_to_jam(&self) -> Result<Vec<u8>, String> {
        info!("Exporting wallet to jam file");
        
        // Create a new NounSlab to work with
        let mut slab: NounSlab = NounSlab::new();
        
        // Build the list of (trek, meta) pairs that match the Hoon wallet format
        let mut trek_meta_pairs: Vec<(noun_serde::wallet::Trek, noun_serde::wallet::Meta)> = Vec::new();
        
        // Parse the master public key and chain code from base58
        let master_pub_bytes = bs58::decode(&self.get_master_public_key())
            .into_vec()
            .map_err(|e| format!("Failed to decode master public key: {}", e))?;
        let chain_code_bytes = bs58::decode(&self.get_chain_code())
            .into_vec()
            .map_err(|e| format!("Failed to decode chain code: {}", e))?;
        
        // Convert bytes back to u64 (assuming 8 bytes)
        if master_pub_bytes.len() != 8 || chain_code_bytes.len() != 8 {
            return Err("Master public key and chain code must be 8 bytes each".to_string());
        }
        
        let master_pub_u64 = u64::from_be_bytes(master_pub_bytes.try_into().unwrap());
        let chain_code_u64 = u64::from_be_bytes(chain_code_bytes.try_into().unwrap());
        
        // Add master public key: /keys/[master-key]/pub/m/[coil]
        let master_pub_trek = noun_serde::wallet::Trek(vec![
            "keys".to_string(),
            format!("{}", master_pub_u64), // Use the key itself as the master identifier
            "pub".to_string(),
            "m".to_string(),
        ]);
        let master_pub_coil = noun_serde::wallet::Coil {
            key: noun_serde::wallet::Key::Pub(master_pub_u64),
            knot: chain_code_u64,
        };
        trek_meta_pairs.push((master_pub_trek, noun_serde::wallet::Meta::Coil(master_pub_coil)));
        
        // Add master private key if available: /keys/[master-key]/prv/m/[coil]
        if !self.get_private_key().is_empty() {
            let master_prv_bytes = bs58::decode(&self.get_private_key())
                .into_vec()
                .map_err(|e| format!("Failed to decode master private key: {}", e))?;
            
            if master_prv_bytes.len() != 8 {
                return Err("Master private key must be 8 bytes".to_string());
            }
            
            let master_prv_u64 = u64::from_be_bytes(master_prv_bytes.try_into().unwrap());
            
            let master_prv_trek = noun_serde::wallet::Trek(vec![
                "keys".to_string(),
                format!("{}", master_pub_u64), // Same master identifier
                "prv".to_string(),
                "m".to_string(),
            ]);
            let master_prv_coil = noun_serde::wallet::Coil {
                key: noun_serde::wallet::Key::Prv(master_prv_u64),
                knot: chain_code_u64,
            };
            trek_meta_pairs.push((master_prv_trek, noun_serde::wallet::Meta::Coil(master_prv_coil)));
        }
        
        // Add seed phrase: /keys/[master-key]/seed/[seed-phrase]
        let seed_phrase = self.get_seed_phrase();
        if !seed_phrase.is_empty() && seed_phrase != "imported-wallet" {
            let seed_trek = noun_serde::wallet::Trek(vec![
                "keys".to_string(),
                format!("{}", master_pub_u64), // Same master identifier
                "seed".to_string(),
            ]);
            trek_meta_pairs.push((seed_trek, noun_serde::wallet::Meta::Label(seed_phrase)));
        }
        
        // Add derived public keys: /keys/[master]/pub/[index]/[coil]
        for (index, derived_key_b58) in self.get_derived_public_keys_map() {
            let derived_key_bytes = bs58::decode(derived_key_b58)
                .into_vec()
                .map_err(|e| format!("Failed to decode derived key at index {}: {}", index, e))?;
            
            if derived_key_bytes.len() != 8 {
                return Err(format!("Derived key at index {} must be 8 bytes", index));
            }
            
            let derived_key_u64 = u64::from_be_bytes(derived_key_bytes.try_into().unwrap());
            
            let derived_trek = noun_serde::wallet::Trek(vec![
                "keys".to_string(),
                format!("{}", master_pub_u64), // Same master identifier
                "pub".to_string(),
                index.to_string(),
            ]);
            let derived_coil = noun_serde::wallet::Coil {
                key: noun_serde::wallet::Key::Pub(derived_key_u64),
                knot: chain_code_u64, // Use same chain code for derived keys
            };
            trek_meta_pairs.push((derived_trek, noun_serde::wallet::Meta::Coil(derived_coil)));
        }
        
        info!("Created {} trek/meta pairs for export", trek_meta_pairs.len());
        
        // put the trek/meta pairs on a slab
        let trek_meta_noun = trek_meta_pairs.to_noun(&mut slab);
        slab.set_root(trek_meta_noun);
        let jammed_bytes = slab.jam();
        
        info!("Successfully exported wallet to {} bytes", jammed_bytes.len());
        
        Ok(jammed_bytes.to_vec())
    }
}