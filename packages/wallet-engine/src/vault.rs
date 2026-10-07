use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt};

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose, Engine as _};
use chacha20poly1305::{
    aead::{AeadInPlace, KeyInit},
    ChaCha20Poly1305, Nonce, Tag,
};
use getrandom::getrandom;
use wasm_bindgen::JsValue;
use zeroize::Zeroize;

use crate::{js_error::js_error, vmk_slot::VmkSlots, wallet::Wallet};
use js_sys::Date;

#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum KdfType {
    Argon2id,
    Argon2d,
}

impl fmt::Debug for KdfType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let variant = match self {
            KdfType::Argon2id => "argon2id",
            KdfType::Argon2d => "argon2d",
        };
        f.write_str(variant)
    }
}

#[derive(Debug)]
pub struct KDF {
    pub kdf_type: KdfType,
    pub salt: [u8; 32],
    pub m: u64,
    pub t: u64,
    pub p: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct WrappedVmk {
    pub nonce: [u8; 12], // random per-wrap
    pub ct: [u8; 32],    // ciphertext of 32-byte VMK
    pub tag: [u8; 16],   // detached auth tag
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VaultData {
    pub padding: [u8; 32],
    // a map of the nickname and wallet
    pub wallets: HashMap<String, Wallet>,
}

impl VaultData {
    pub fn new() -> Result<Self, JsValue> {
        let mut padding = [0u8; 32];
        getrandom(&mut padding)
            .map_err(|err| js_error(&format!("failed to generate vault padding: {err}")))?;
        Ok(Self {
            padding,
            wallets: HashMap::new(),
        })
    }

    fn summaries(&self) -> Vec<WalletSummary> {
        self.wallets
            .iter()
            .map(|(nickname, wallet)| WalletSummary {
                nickname: nickname.clone(),
                public_key: wallet.public_key.clone(),
                extended_public_key: wallet.extended_public_key.clone(),
            })
            .collect()
    }
}

impl KDF {
    fn from_extern(extern_kdf: &KDFExtern) -> Result<Self, JsValue> {
        Ok(Self {
            kdf_type: extern_kdf.kdf_type,
            salt: decode_array(&extern_kdf.salt, "kdf.salt")?,
            m: extern_kdf.m,
            t: extern_kdf.t,
            p: extern_kdf.p,
        })
    }
}

#[derive(Debug)]
pub struct Vault {
    pub kdf: KDF,
    pub wrapped_vmk: WrappedVmk,
    pub log: Vec<VaultLogEntry>,
    pub data: VaultData,
}

#[derive(Clone, Debug)]
pub struct VaultLogEntry {
    pub timestamp: u64,
    pub operation: String,
}

#[derive(Debug)]
pub struct VaultArtifacts {
    pub vault: Vault,
    pub vmk_slot: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KDFExtern {
    pub kdf_type: KdfType,
    // b64 encoded
    pub salt: String,
    pub m: u64,
    pub t: u64,
    pub p: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrappedVmkExtern {
    // b64 encoded
    pub nonce: String,
    pub ct: String,
    pub tag: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultExtern {
    pub kdf: KDFExtern,
    pub wrapped_vmk: WrappedVmkExtern,
    pub log: Vec<VaultLogEntryExtern>,
    // b64 encoded and encrypted
    pub data: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultLogEntryExtern {
    pub timestamp: u64,
    pub operation: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletSummary {
    pub nickname: String,
    pub public_key: String,
    pub extended_public_key: Option<String>,
}

impl VaultExtern {
    pub fn from_vault(mut v: Vault, operation: &str, vmk_slot: u32) -> Result<Self, JsValue> {
        v.log_operation(operation);
        let data = encrypt_vault_data(&v.data, vmk_slot)?;
        Ok(Self {
            kdf: KDFExtern {
                kdf_type: v.kdf.kdf_type,
                salt: general_purpose::STANDARD.encode(v.kdf.salt),
                m: v.kdf.m,
                t: v.kdf.t,
                p: v.kdf.p,
            },
            wrapped_vmk: WrappedVmkExtern {
                nonce: general_purpose::STANDARD.encode(v.wrapped_vmk.nonce),
                ct: general_purpose::STANDARD.encode(v.wrapped_vmk.ct),
                tag: general_purpose::STANDARD.encode(v.wrapped_vmk.tag),
            },
            log: v
                .log
                .into_iter()
                .map(|entry| VaultLogEntryExtern {
                    timestamp: entry.timestamp,
                    operation: entry.operation,
                })
                .collect(),
            data,
        })
    }

    pub fn append_log(&mut self, operation: &str) {
        push_log_entry(&mut self.log, operation);
    }

    pub fn import_wallet(
        &mut self,
        vmk_slot: u32,
        nickname: &str,
        key: &str,
    ) -> Result<WalletSummary, JsValue> {
        self.with_data_mut(vmk_slot, |data| {
            if data.wallets.contains_key(nickname) {
                return Err(js_error("wallet already exists"));
            }
            let wallet = Wallet::from_key(key)?;
            if let Some((existing_nickname, _)) = data
                .wallets
                .iter()
                .find(|(_nick, existing)| existing.private_key == wallet.private_key)
            {
                return Err(js_error(&format!(
                    "wallet {} already uses that private key",
                    existing_nickname
                )));
            }
            let public_key = wallet.public_key.clone();
            let extended_public_key = wallet.extended_public_key.clone();
            data.wallets.insert(nickname.to_string(), wallet);
            Ok(WalletSummary {
                nickname: nickname.to_string(),
                public_key,
                extended_public_key,
            })
        })
    }

    pub fn rename_wallet(
        &mut self,
        vmk_slot: u32,
        old_nickname: &str,
        new_nickname: &str,
    ) -> Result<WalletSummary, JsValue> {
        self.with_data_mut(vmk_slot, |data| {
            if old_nickname == new_nickname {
                return data
                    .wallets
                    .get(old_nickname)
                    .map(|wallet| WalletSummary {
                        nickname: old_nickname.to_string(),
                        public_key: wallet.public_key.clone(),
                        extended_public_key: wallet.extended_public_key.clone(),
                    })
                    .ok_or_else(|| js_error("wallet not found"));
            }

            if data.wallets.contains_key(new_nickname) {
                return Err(js_error("nickname already exists"));
            }

            let wallet = data
                .wallets
                .remove(old_nickname)
                .ok_or_else(|| js_error("wallet not found"))?;
            let public_key = wallet.public_key.clone();
            let extended_public_key = wallet.extended_public_key.clone();
            data.wallets.insert(new_nickname.to_string(), wallet);
            Ok(WalletSummary {
                nickname: new_nickname.to_string(),
                public_key,
                extended_public_key,
            })
        })
    }

    pub fn delete_wallet(&mut self, vmk_slot: u32, nickname: &str) -> Result<bool, JsValue> {
        self.with_data_mut(vmk_slot, |data| {
            data.wallets
                .remove(nickname)
                .map(|_| true)
                .ok_or_else(|| js_error("wallet not found"))
        })
    }

    pub fn wallet_summary(&self, vmk_slot: u32, nickname: &str) -> Result<WalletSummary, JsValue> {
        self.with_data(vmk_slot, |data| {
            data.wallets
                .get(nickname)
                .map(|wallet| WalletSummary {
                    nickname: nickname.to_string(),
                    public_key: wallet.public_key.clone(),
                    extended_public_key: wallet.extended_public_key.clone(),
                })
                .ok_or_else(|| js_error("wallet not found"))
        })
    }

    pub fn wallet_summaries(&self, vmk_slot: u32) -> Result<Vec<WalletSummary>, JsValue> {
        self.with_data(vmk_slot, |data| Ok(data.summaries()))
    }

    pub fn export_wallet(&self, vmk_slot: u32, nickname: &str) -> Result<Wallet, JsValue> {
        self.with_data(vmk_slot, |data| {
            data.wallets
                .get(nickname)
                .cloned()
                .ok_or_else(|| js_error("wallet not found"))
        })
    }

    fn with_data_mut<T, F>(&mut self, vmk_slot: u32, op: F) -> Result<T, JsValue>
    where
        F: FnOnce(&mut VaultData) -> Result<T, JsValue>,
    {
        let mut data = VmkSlots::with_vmk(vmk_slot, |vmk| decrypt_vault_data(&self.data, vmk))?;
        let result = op(&mut data)?;
        self.data = encrypt_vault_data(&data, vmk_slot)?;
        Ok(result)
    }

    fn with_data<T, F>(&self, vmk_slot: u32, op: F) -> Result<T, JsValue>
    where
        F: FnOnce(&VaultData) -> Result<T, JsValue>,
    {
        let data = VmkSlots::with_vmk(vmk_slot, |vmk| decrypt_vault_data(&self.data, vmk))?;
        op(&data)
    }
}

impl Vault {
    // construct a new vault
    pub fn new(local_password: &str, kdf_type: KdfType) -> Result<VaultArtifacts, JsValue> {
        // we construct the kdf params
        let kdf = Self::build_kdf(kdf_type)?;

        // we derive key encryption key (kek)
        let mut kek = Self::derive_kek(local_password, &kdf)?;

        // we create the vault master key (vmk)
        let mut vmk = Self::generate_vmk()?;

        let vmk_slot = match VmkSlots::allocate(&vmk) {
            Ok(slot) => slot,
            Err(err) => {
                vmk.zeroize();
                kek.zeroize();
                return Err(err);
            }
        };
        let wrapped_vmk = match Self::wrap_vmk(&kek, &vmk, &kdf.salt) {
            Ok(result) => result,
            Err(err) => {
                let _ = VmkSlots::free(vmk_slot);
                vmk.zeroize();
                kek.zeroize();
                return Err(err);
            }
        };

        vmk.zeroize();
        kek.zeroize();

        let log = Vec::new();

        let data = VaultData::new()?;

        Ok(VaultArtifacts {
            vault: Self {
                kdf,
                wrapped_vmk,
                log,
                data,
            },
            vmk_slot,
        })
    }

    pub fn log_operation(&mut self, operation: &str) {
        self.log.push(VaultLogEntry::new(operation));
    }

    fn build_kdf(kdf_type: KdfType) -> Result<KDF, JsValue> {
        let salt = Self::generate_salt()?;
        let (m, t, p) = match kdf_type {
            KdfType::Argon2id => (128 * 1024, 3, 1),
            // KdfType::Argon2d => (64 * 1024, 3, 1), this is not used for now
            _ => return Err(js_error("unsupported KDF type")),
        };

        Ok(KDF {
            kdf_type,
            salt,
            m,
            t,
            p,
        })
    }

    fn derive_kek(local_password: &str, kdf: &KDF) -> Result<[u8; 32], JsValue> {
        let algorithm = match kdf.kdf_type {
            KdfType::Argon2id => Algorithm::Argon2id,
            _ => return Err(js_error("unsupported KDF type")),
        };

        let mem_cost =
            u32::try_from(kdf.m).map_err(|_| js_error("argon2 mem cost is too large"))?;
        let time_cost =
            u32::try_from(kdf.t).map_err(|_| js_error("argon2 time cost is too large"))?;
        let lanes =
            u32::try_from(kdf.p).map_err(|_| js_error("argon2 lanes count is too large"))?;

        let params = Params::new(mem_cost, time_cost, lanes, Some(32))
            .map_err(|err| js_error(&format!("argon2 params: {err}")))?;
        let argon2 = Argon2::new(algorithm, Version::V0x13, params);

        let mut kek = [0u8; 32];
        argon2
            .hash_password_into(local_password.as_bytes(), &kdf.salt, &mut kek)
            .map_err(|err| js_error(&format!("failed to derive KEK: {err}")))?;
        Ok(kek)
    }

    fn generate_salt() -> Result<[u8; 32], JsValue> {
        let mut bytes = [0u8; 32];
        getrandom(&mut bytes)
            .map_err(|err| js_error(&format!("failed to generate vault salt: {err}")))?;
        Ok(bytes)
    }

    fn generate_vmk() -> Result<[u8; 32], JsValue> {
        let mut bytes = [0u8; 32];
        getrandom(&mut bytes)
            .map_err(|err| js_error(&format!("failed to generate vault master key: {err}")))?;
        Ok(bytes)
    }

    pub fn wrap_vmk(kek: &[u8; 32], vmk: &[u8; 32], aad: &[u8]) -> Result<WrappedVmk, JsValue> {
        // 1) cipher from KEK
        let cipher =
            ChaCha20Poly1305::new_from_slice(kek).map_err(|_| js_error("invalid KEK length"))?;

        // 2) 12-byte random nonce
        let mut nonce_arr = [0u8; 12];
        getrandom(&mut nonce_arr)
            .map_err(|err| js_error(&format!("failed to generate VMK nonce: {err}")))?;
        let nonce = Nonce::from(nonce_arr);

        // 3) encrypt VMK in place, get detached tag
        let mut buf = *vmk; // local copy; we’ll zeroize after
        let tag = cipher
            .encrypt_in_place_detached(&nonce, aad, &mut buf)
            .map_err(|_| js_error("failed to wrap VMK"))?;

        // 4) build output with fixed sizes
        let mut out = WrappedVmk {
            nonce: nonce_arr,
            ct: buf,
            tag: [0u8; 16],
        };
        out.tag.copy_from_slice(tag.as_ref());

        Ok(out)
    }

    pub fn unwrap_vmk(
        kek: &[u8; 32],
        wrapped: &WrappedVmk,
        aad: &[u8],
    ) -> Result<[u8; 32], JsValue> {
        let cipher =
            ChaCha20Poly1305::new_from_slice(kek).map_err(|_| js_error("invalid KEK length"))?;
        let mut buf = wrapped.ct;
        let nonce = Nonce::from(wrapped.nonce);
        let tag = Tag::from(wrapped.tag);
        cipher
            .decrypt_in_place_detached(&nonce, aad, &mut buf, &tag)
            .map_err(|_| js_error("failed to unwrap VMK"))?;
        Ok(buf)
    }

    pub fn verify_password(
        local_password: &str,
        vault: &VaultExtern,
        vmk_slot: u32,
    ) -> Result<bool, JsValue> {
        let kdf = KDF::from_extern(&vault.kdf)?;
        let mut kek = Self::derive_kek(local_password, &kdf)?;
        let wrapped = decode_wrapped_vmk(&vault.wrapped_vmk)?;
        let mut vmk_candidate = Self::unwrap_vmk(&kek, &wrapped, &kdf.salt)?;
        kek.zeroize();
        let matches = VmkSlots::with_vmk(vmk_slot, |vmk| Ok(vmk == &vmk_candidate))?;
        vmk_candidate.zeroize();
        Ok(matches)
    }

    pub fn unlock_serialized(
        local_password: &str,
        vault: VaultExtern,
    ) -> Result<(VaultExtern, u32, Vec<WalletSummary>), JsValue> {
        let kdf = KDF::from_extern(&vault.kdf)?;
        let mut kek = Self::derive_kek(local_password, &kdf)?;
        let wrapped = decode_wrapped_vmk(&vault.wrapped_vmk)?;
        let mut vmk = Self::unwrap_vmk(&kek, &wrapped, &kdf.salt)?;
        kek.zeroize();
        let data = decrypt_vault_data(&vault.data, &vmk)?;

        let allocation = VmkSlots::allocate(&vmk);
        vmk.zeroize();
        let vmk_slot = allocation?;

        let mut vault = vault;
        vault.append_log("UNLOCK");
        Ok((vault, vmk_slot, data.summaries()))
    }

    /// Unlock vault using a raw VMK (for session restoration).
    /// The VMK must match the one used to encrypt the vault data.
    /// Returns error if VMK is invalid (cannot decrypt vault data).
    pub fn unlock_with_vmk(
        vmk: &[u8; 32],
        vault: VaultExtern,
    ) -> Result<(VaultExtern, u32, Vec<WalletSummary>), JsValue> {
        // Verify the VMK can decrypt the vault data
        let data = decrypt_vault_data(&vault.data, vmk)?;

        // Allocate a slot for this VMK
        let vmk_slot = VmkSlots::allocate(vmk)?;

        let mut vault = vault;
        vault.append_log("UNLOCK_SESSION");
        Ok((vault, vmk_slot, data.summaries()))
    }
}

impl VaultLogEntry {
    fn new(operation: &str) -> Self {
        Self {
            timestamp: unix_timestamp(),
            operation: operation.to_string(),
        }
    }
}

fn push_log_entry(log: &mut Vec<VaultLogEntryExtern>, operation: &str) {
    log.push(VaultLogEntryExtern {
        timestamp: unix_timestamp(),
        operation: operation.to_string(),
    });
}

fn unix_timestamp() -> u64 {
    (Date::now() / 1000.0) as u64
}

// Encrypts the internal vault data with the VMK stored at `vmk_slot` and
// returns the ciphertext (nonce||ct||tag) as a Base64 string. This allows the
// frontend to persist opaque data without learning its contents.
fn encrypt_vault_data(data: &VaultData, vmk_slot: u32) -> Result<String, JsValue> {
    let mut payload = serde_json::to_vec(data)
        .map_err(|err| js_error(&format!("failed to serialize vault data: {err}")))?;

    VmkSlots::with_vmk(vmk_slot, move |vmk| {
        let cipher =
            ChaCha20Poly1305::new_from_slice(vmk).map_err(|_| js_error("invalid VMK length"))?;

        let mut nonce = [0u8; 12];
        getrandom(&mut nonce)
            .map_err(|err| js_error(&format!("failed to generate data nonce: {err}")))?;

        let tag = cipher
            .encrypt_in_place_detached(&Nonce::from(nonce), b"vault-data", &mut payload)
            .map_err(|_| js_error("failed to encrypt vault data"))?;

        let tag_bytes: &[u8] = tag.as_ref();
        let mut output = Vec::with_capacity(nonce.len() + payload.len() + tag_bytes.len());
        output.extend_from_slice(&nonce);
        output.extend_from_slice(&payload);
        output.extend_from_slice(tag_bytes);
        payload.zeroize();

        Ok(general_purpose::STANDARD.encode(output))
    })
}

fn decrypt_vault_data(data_b64: &str, vmk: &[u8; 32]) -> Result<VaultData, JsValue> {
    let mut blob = general_purpose::STANDARD
        .decode(data_b64)
        .map_err(|err| js_error(&format!("failed to decode vault data: {err}")))?;
    if blob.len() < 12 + 16 {
        return Err(js_error("vault data blob too short"));
    }
    let (nonce_bytes, remainder) = blob.split_at_mut(12);
    let (ciphertext, tag_bytes) = remainder.split_at_mut(remainder.len().saturating_sub(16));
    let nonce_array: [u8; 12] = nonce_bytes
        .try_into()
        .map_err(|_| js_error("vault data nonce has invalid length"))?;
    let tag_array: [u8; 16] = tag_bytes
        .try_into()
        .map_err(|_| js_error("vault data tag has invalid length"))?;
    let nonce = Nonce::from(nonce_array);
    let tag = Tag::from(tag_array);
    let cipher =
        ChaCha20Poly1305::new_from_slice(vmk).map_err(|_| js_error("invalid VMK length"))?;
    cipher
        .decrypt_in_place_detached(&nonce, b"vault-data", ciphertext, &tag)
        .map_err(|_| js_error("failed to decrypt vault data"))?;
    let data = serde_json::from_slice::<VaultData>(ciphertext)
        .map_err(|err| js_error(&format!("failed to deserialize vault data: {err}")))?;
    ciphertext.zeroize();
    blob.zeroize();
    Ok(data)
}

fn decode_array<const N: usize>(value: &str, label: &str) -> Result<[u8; N], JsValue> {
    let bytes = general_purpose::STANDARD
        .decode(value)
        .map_err(|err| js_error(&format!("failed to decode {label}: {err}")))?;
    bytes
        .try_into()
        .map_err(|_| js_error(&format!("{label} has invalid length")))
}

fn decode_wrapped_vmk(extern_wrapped: &WrappedVmkExtern) -> Result<WrappedVmk, JsValue> {
    Ok(WrappedVmk {
        nonce: decode_array(&extern_wrapped.nonce, "wrappedVmk.nonce")?,
        ct: decode_array(&extern_wrapped.ct, "wrappedVmk.ct")?,
        tag: decode_array(&extern_wrapped.tag, "wrappedVmk.tag")?,
    })
}
