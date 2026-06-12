use alloc::vec::Vec;
use bip32::{ChildNumber, DerivationPath, PublicKey, XPrv};
use hmac::Hmac;
use k256::ecdsa::SigningKey;
use nockster_core::alloc_path as pathmod;
use nockster_core::{
    cheetah, CheetahPub, Request, Response, Xpub, ERR_CRYPTO, ERR_FLASH, ERR_NO_SEED, ERR_OVERFLOW,
    ERR_PIN_LOCKED_OUT, ERR_WRONG_PIN,
};
use pbkdf2::pbkdf2;
use ripemd::Ripemd160;
use sha2::{Digest, Sha256, Sha512};
use zeroize::Zeroize;

use crate::jobs::{SeedOp, SeedOpOutcome, SeedOpRequest};
use crate::session;
use core::cell::RefCell;
use critical_section::Mutex;
use nockster_fw::nvs_store::{NvsError, NvsStore};

// Cache of the per-slot root pubkeys returned by GetInfo, keyed by the session's
// seed generation. Deriving these (HMAC-SHA512 + a Cheetah scalar multiply per slot)
// is expensive on-device, and the host used to read them repeatedly. We now derive
// once per seed-set change and serve the cache otherwise. Pubkeys are public, so the
// cache holds no secret material.
#[allow(clippy::declare_interior_mutable_const)]
static INFO_PUBS_CACHE: Mutex<RefCell<Option<(u64, Vec<CheetahPub>)>>> =
    Mutex::new(RefCell::new(None));

const BIP39_WORD_COUNT: usize = 24;
const BIP39_MAX_SENTENCE_LEN: usize = 320;
const BIP39_PBKDF2_ROUNDS: u32 = 2048;

#[derive(Default)]
pub struct PendingSeedSetup {
    seed64: Option<[u8; 64]>,
}

impl PendingSeedSetup {
    pub const fn new() -> Self {
        Self { seed64: None }
    }

    pub fn store_from(&mut self, seed64: &mut [u8; 64]) {
        self.clear();
        self.seed64 = Some(*seed64);
        seed64.zeroize();
    }

    pub fn take(&mut self) -> Option<[u8; 64]> {
        let seed64 = self.seed64.as_mut()?;
        let out = *seed64;
        seed64.zeroize();
        self.seed64 = None;
        Some(out)
    }

    pub fn has_seed(&self) -> bool {
        self.seed64.is_some()
    }

    pub fn clear(&mut self) {
        if let Some(seed64) = self.seed64.as_mut() {
            seed64.zeroize();
        }
        self.seed64 = None;
    }
}

impl Drop for PendingSeedSetup {
    fn drop(&mut self) {
        self.clear();
    }
}

pub fn bip39_seed_from_words<'a>(words: impl IntoIterator<Item = &'a str>) -> Result<[u8; 64], ()> {
    let mut sentence = [0u8; BIP39_MAX_SENTENCE_LEN];

    let result = (|| {
        let mut len = 0usize;
        let mut count = 0usize;
        for word in words {
            if count >= BIP39_WORD_COUNT {
                return Err(());
            }
            if count > 0 {
                if len >= sentence.len() {
                    return Err(());
                }
                sentence[len] = b' ';
                len += 1;
            }

            let word_bytes = word.as_bytes();
            let end = len.checked_add(word_bytes.len()).ok_or(())?;
            if end > sentence.len() {
                return Err(());
            }
            sentence[len..end].copy_from_slice(word_bytes);
            len = end;
            count += 1;
        }

        if count != BIP39_WORD_COUNT {
            return Err(());
        }

        let mut out = [0u8; 64];
        if pbkdf2::<Hmac<Sha512>>(&sentence[..len], b"mnemonic", BIP39_PBKDF2_ROUNDS, &mut out)
            .is_err()
        {
            out.zeroize();
            return Err(());
        }
        Ok(out)
    })();

    sentence.zeroize();
    result
}

pub fn get_xpub(_path: &pathmod::Path) -> Result<Xpub, ()> {
    // secp256k1/bip32 xpub derivation needs a BIP39 seed; slots now store a
    // Cheetah coil with no seed, so this legacy path is unsupported.
    Err(())
}

#[allow(dead_code)]
fn get_xpub_legacy(path: &pathmod::Path) -> Result<Xpub, ()> {
    let mut seed = get_active_seed_copy()?;
    let dp = path_to_derivation(path)?;
    let child = match XPrv::derive_from_path(&seed, &dp) {
        Ok(child) => child,
        Err(_) => {
            seed.zeroize();
            return Err(());
        }
    };
    seed.zeroize();
    let xpub = child.public_key();
    let attrs = child.attrs();
    let depth = attrs.depth;
    let child_u32 = u32::from(attrs.child_number);
    let fp4 = attrs.parent_fingerprint;
    let chain_code = attrs.chain_code;
    let mut pubkey33 = [0u8; 33];
    pubkey33.copy_from_slice(&xpub.public_key().to_bytes());
    Ok(Xpub {
        depth,
        fp4,
        child: child_u32,
        chain_code,
        pubkey33,
    })
}

pub fn master_key_copy() -> Option<[u8; 32]> {
    session::master_key_copy()
}

pub fn store_master_key(key: &[u8; 32]) {
    session::store_master_key(key);
}

pub fn clear_master_key() {
    session::clear_master_key();
}

pub fn set_seed(seed64: &[u8; 64]) {
    session::set_seed(seed64);
}

pub fn update_seed_store_from_slice(seeds: &[[u8; 64]]) {
    session::update_seed_store_from_slice(seeds);
}

pub fn append_seed_slot(seed64: &[u8; 64]) {
    session::append_seed_slot(seed64);
}

pub fn remove_seed_slot(index: usize) {
    session::remove_seed_slot(index);
}

pub fn wipe_seed() {
    session::wipe();
}

pub fn handle_session_request(req: &Request) -> Option<Response> {
    match req {
        Request::SetSeed { seed64 } => {
            // Dev/diagnostic path: store the BIP39 seed as a coil too.
            let mut coil = coil_from_seed(seed64);
            set_seed(&coil);
            coil.zeroize();
            Some(Response::Ok)
        }
        Request::Wipe | Request::Lock => {
            wipe_seed();
            Some(Response::Ok)
        }
        Request::SelectSeed { slot } => Some(match set_active_slot(*slot as usize) {
            Ok(()) => Response::Ok,
            Err(_) => Response::Err { code: ERR_NO_SEED },
        }),
        _ => None,
    }
}

pub fn compute_seed_op_outcome(mut request: SeedOpRequest) -> SeedOpOutcome {
    let msg_id = request.msg_id;
    match &mut request.op {
        SeedOp::Add { seed64, master_key } => {
            let mut nvs = NvsStore::new();
            // Convert the BIP39 seed to the stored coil; the session and NVS
            // hold coils, not seeds.
            let mut coil = coil_from_seed(seed64);
            seed64.zeroize();
            let pub_xy = root_pub_from_coil(&coil);
            let outcome = match nvs.add_seed_with_key(master_key, &coil, pub_xy) {
                Ok(slot) => SeedOpOutcome::Added {
                    msg_id,
                    slot,
                    seed64: coil,
                },
                Err(err) => {
                    // On failure the coil is not handed to the session; wipe it.
                    coil.zeroize();
                    match err {
                        NvsError::WrongPin => SeedOpOutcome::WrongPin { msg_id },
                        NvsError::LockedOut => SeedOpOutcome::LockedOut { msg_id },
                        NvsError::Full => SeedOpOutcome::Full { msg_id },
                        NvsError::Flash => SeedOpOutcome::Flash { msg_id },
                        NvsError::Crypto => SeedOpOutcome::Crypto { msg_id },
                        NvsError::NotInitialized => SeedOpOutcome::NotInitialized { msg_id },
                        _ => SeedOpOutcome::Failed { msg_id },
                    }
                }
            };
            master_key.zeroize();
            outcome
        }
        SeedOp::AddCoil { coil64, master_key } => {
            let mut nvs = NvsStore::new();
            // The host already validated the encoding (zprv checksum etc.);
            // the 64 bytes are stored exactly as a seed-derived coil would be.
            let mut coil = *coil64;
            coil64.zeroize();
            let pub_xy = root_pub_from_coil(&coil);
            let outcome = match nvs.add_seed_with_key(master_key, &coil, pub_xy) {
                Ok(slot) => SeedOpOutcome::Added {
                    msg_id,
                    slot,
                    seed64: coil,
                },
                Err(err) => {
                    coil.zeroize();
                    match err {
                        NvsError::WrongPin => SeedOpOutcome::WrongPin { msg_id },
                        NvsError::LockedOut => SeedOpOutcome::LockedOut { msg_id },
                        NvsError::Full => SeedOpOutcome::Full { msg_id },
                        NvsError::Flash => SeedOpOutcome::Flash { msg_id },
                        NvsError::Crypto => SeedOpOutcome::Crypto { msg_id },
                        NvsError::NotInitialized => SeedOpOutcome::NotInitialized { msg_id },
                        _ => SeedOpOutcome::Failed { msg_id },
                    }
                }
            };
            master_key.zeroize();
            outcome
        }
        SeedOp::Delete { slot, master_key } => {
            let mut nvs = NvsStore::new();
            let outcome = match nvs.delete_seed_with_key(master_key, *slot as usize) {
                Ok(()) => SeedOpOutcome::Deleted {
                    msg_id,
                    slot: *slot,
                },
                Err(NvsError::InvalidSlot) => SeedOpOutcome::InvalidSlot { msg_id },
                Err(NvsError::WrongPin) => SeedOpOutcome::WrongPin { msg_id },
                Err(NvsError::LockedOut) => SeedOpOutcome::LockedOut { msg_id },
                Err(NvsError::NotInitialized) => SeedOpOutcome::NotInitialized { msg_id },
                Err(NvsError::Flash) => SeedOpOutcome::Flash { msg_id },
                Err(NvsError::Crypto) => SeedOpOutcome::Crypto { msg_id },
                Err(_) => SeedOpOutcome::Failed { msg_id },
            };
            master_key.zeroize();
            outcome
        }
        SeedOp::Reset => {
            let mut nvs = NvsStore::new();
            match nvs.factory_reset() {
                Ok(()) => SeedOpOutcome::Reset { msg_id },
                Err(NvsError::Flash) => SeedOpOutcome::Flash { msg_id },
                Err(_) => SeedOpOutcome::Failed { msg_id },
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedOpUiEffect {
    None,
    Added,
    Deleted,
    Reset,
}

pub struct SeedOpApplied {
    pub msg_id: u32,
    pub response: Response,
    pub ui_effect: SeedOpUiEffect,
    pub added_slot: Option<u8>,
    pub debug: &'static [u8],
}

pub fn apply_seed_op_outcome(outcome: SeedOpOutcome) -> SeedOpApplied {
    match outcome {
        SeedOpOutcome::Added {
            msg_id,
            slot,
            mut seed64,
        } => {
            append_seed_slot(&seed64);
            seed64.zeroize();
            SeedOpApplied {
                msg_id,
                response: Response::Ok,
                ui_effect: SeedOpUiEffect::Added,
                added_slot: Some(slot),
                debug: b"seed added\r\n",
            }
        }
        SeedOpOutcome::Deleted { msg_id, slot } => {
            remove_seed_slot(slot as usize);
            SeedOpApplied {
                msg_id,
                response: Response::Ok,
                ui_effect: SeedOpUiEffect::Deleted,
                added_slot: None,
                debug: b"seed deleted\r\n",
            }
        }
        SeedOpOutcome::Reset { msg_id } => SeedOpApplied {
            msg_id,
            response: Response::Ok,
            ui_effect: SeedOpUiEffect::Reset,
            added_slot: None,
            debug: b"factory reset\r\n",
        },
        SeedOpOutcome::WrongPin { msg_id } => SeedOpApplied {
            msg_id,
            response: Response::Err {
                code: ERR_WRONG_PIN,
            },
            ui_effect: SeedOpUiEffect::None,
            added_slot: None,
            debug: b"seed op wrong pin\r\n",
        },
        SeedOpOutcome::LockedOut { msg_id } => SeedOpApplied {
            msg_id,
            response: Response::Err {
                code: ERR_PIN_LOCKED_OUT,
            },
            ui_effect: SeedOpUiEffect::None,
            added_slot: None,
            debug: b"seed op locked out\r\n",
        },
        SeedOpOutcome::Full { msg_id } => SeedOpApplied {
            msg_id,
            response: Response::Err { code: ERR_OVERFLOW },
            ui_effect: SeedOpUiEffect::None,
            added_slot: None,
            debug: b"seed op full\r\n",
        },
        SeedOpOutcome::InvalidSlot { msg_id } | SeedOpOutcome::NotInitialized { msg_id } => {
            SeedOpApplied {
                msg_id,
                response: Response::Err { code: ERR_NO_SEED },
                ui_effect: SeedOpUiEffect::None,
                added_slot: None,
                debug: b"seed op no seed\r\n",
            }
        }
        SeedOpOutcome::Flash { msg_id } => SeedOpApplied {
            msg_id,
            response: Response::Err { code: ERR_FLASH },
            ui_effect: SeedOpUiEffect::None,
            added_slot: None,
            debug: b"seed op flash error\r\n",
        },
        SeedOpOutcome::Crypto { msg_id } => SeedOpApplied {
            msg_id,
            response: Response::Err { code: ERR_CRYPTO },
            ui_effect: SeedOpUiEffect::None,
            added_slot: None,
            debug: b"seed op crypto error\r\n",
        },
        SeedOpOutcome::Failed { msg_id } => SeedOpApplied {
            msg_id,
            response: Response::Err { code: ERR_NO_SEED },
            ui_effect: SeedOpUiEffect::None,
            added_slot: None,
            debug: b"seed op failed\r\n",
        },
    }
}

pub fn collect_info_pubs_from_ram() -> Vec<CheetahPub> {
    let generation = session::seed_generation();

    // Fast path: serve the cached pubkeys while the seed set is unchanged.
    if let Some(cached) =
        critical_section::with(|cs| match INFO_PUBS_CACHE.borrow_ref(cs).as_ref() {
            Some((gen, pubs)) if *gen == generation => Some(pubs.clone()),
            _ => None,
        })
    {
        return cached;
    }

    // Slow path: derive once for this generation, then cache.
    let mut out = Vec::new();
    for (idx, seed) in session::seed_slots_copy().into_iter().enumerate() {
        let pub_xy = root_pub_from_coil(&seed);
        let path = pathmod::Path::new();
        out.push(CheetahPub {
            slot: idx as u8,
            path,
            x: pub_xy.0,
            y: pub_xy.1,
        });
    }

    let cache_entry = out.clone();
    critical_section::with(|cs| {
        *INFO_PUBS_CACHE.borrow_ref_mut(cs) = Some((generation, cache_entry));
    });
    out
}

fn path_to_derivation(path: &pathmod::Path) -> Result<DerivationPath, ()> {
    let mut dp = DerivationPath::default();
    for &p in path.iter() {
        let hardened = (p & 0x8000_0000) != 0;
        let idx = p & 0x7FFF_FFFF;
        let child = ChildNumber::new(idx, hardened).map_err(|_| ())?;
        dp.push(child);
    }
    Ok(dp)
}

fn derive_signing_key_for_slot(_path: &pathmod::Path, _slot: usize) -> Result<SigningKey, ()> {
    // secp256k1 signing needs a BIP39 seed; coil slots have none.
    Err(())
}

#[allow(dead_code)]
fn derive_signing_key_for_slot_legacy(path: &pathmod::Path, slot: usize) -> Result<SigningKey, ()> {
    let mut seed = get_seed_for_slot(slot)?;
    let mut key = match XPrv::new(&seed) {
        Ok(key) => key,
        Err(_) => {
            seed.zeroize();
            return Err(());
        }
    };
    seed.zeroize();
    for index in path.iter() {
        let child_num = ChildNumber::from(*index);
        key = key.derive_child(child_num).map_err(|_| ())?;
    }
    let mut sk_bytes = key.private_key().to_bytes();
    let signing_key = match SigningKey::from_bytes((&sk_bytes).into()) {
        Ok(signing_key) => signing_key,
        Err(_) => {
            sk_bytes.zeroize();
            return Err(());
        }
    };
    sk_bytes.zeroize();
    Ok(signing_key)
}

pub fn master_fingerprint_for_active() -> Result<[u8; 4], ()> {
    // secp256k1 master fingerprint needs a BIP39 seed; coil slots have none.
    Err(())
}

#[allow(dead_code)]
fn master_fingerprint_for_active_legacy() -> Result<[u8; 4], ()> {
    let mut seed = get_active_seed_copy()?;
    let xprv = match XPrv::new(&seed) {
        Ok(xprv) => xprv,
        Err(_) => {
            seed.zeroize();
            return Err(());
        }
    };
    seed.zeroize();
    let xpub = xprv.public_key();
    let comp = xpub.public_key().to_bytes();
    let sha = Sha256::digest(&comp);
    let ripe = Ripemd160::digest(sha);
    let mut fp4 = [0u8; 4];
    fp4.copy_from_slice(&ripe[..4]);
    Ok(fp4)
}

/// Slots store a Cheetah master **coil** (`sk || cc`), the nockchain-wallet's
/// native key format, rather than a BIP39 seed. A BIP39 mnemonic is converted
/// to a coil once at ingest via [`coil_from_seed`]; a raw coil (e.g. from a
/// wallet `keys.export`) is stored as-is. Child derivation then skips the
/// SLIP10 master step entirely.
pub fn coil_from_seed(seed64: &[u8; 64]) -> [u8; 64] {
    let (mut sk, mut cc) = cheetah::master_from_seed(seed64);
    let mut coil = [0u8; 64];
    coil[..32].copy_from_slice(&sk);
    coil[32..].copy_from_slice(&cc);
    sk.zeroize();
    cc.zeroize();
    coil
}

pub fn derive_child_sk_for_slot(path: &pathmod::Path, slot: usize) -> Result<[u8; 32], ()> {
    let mut coil = get_seed_for_slot(slot)?;
    let mut sk = [0u8; 32];
    let mut cc = [0u8; 32];
    sk.copy_from_slice(&coil[..32]);
    cc.copy_from_slice(&coil[32..]);
    coil.zeroize();
    let mut xk = cheetah::XKey::from_master(sk, cc);
    sk.zeroize();
    cc.zeroize();
    for &i in path.iter() {
        xk = cheetah::xprv_derive_child(&xk, i);
    }
    xk.sk.ok_or(())
}

/// Root public key for a stored coil (`pub` of the master `sk`).
pub fn root_pub_from_coil(coil: &[u8; 64]) -> ([u64; 6], [u64; 6]) {
    let mut sk = [0u8; 32];
    sk.copy_from_slice(&coil[..32]);
    let pubkey = cheetah::cheetah_pub_from_sk(sk);
    sk.zeroize();
    pubkey
}

fn get_seed_for_slot(slot: usize) -> Result<[u8; 64], ()> {
    session::get_seed_for_slot(slot)
}

fn get_active_seed_copy() -> Result<[u8; 64], ()> {
    session::get_active_seed_copy()
}

pub fn set_active_slot(slot: usize) -> Result<(), ()> {
    session::set_active_slot(slot)
}

pub fn active_slot_index() -> Result<usize, ()> {
    session::active_slot_index()
}

pub fn derive_signing_key_active(path: &pathmod::Path) -> Result<SigningKey, ()> {
    let slot = active_slot_index()?;
    derive_signing_key_for_slot(path, slot)
}
