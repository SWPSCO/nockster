use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use alloc::vec::Vec;
use core::result::Result;
use embedded_storage::{ReadStorage, Storage};
use esp_storage::FlashStorage;
use hmac::Hmac;
use pbkdf2::pbkdf2;
use sha2::Sha256;
use zeroize::Zeroize;

const PBKDF2_ROUNDS: u32 = 100_000;
const MAX_PIN_ATTEMPTS: u8 = 10;

// NVS storage layout in flash (128 bytes total):
// offset 0: initialized flag (1 byte)
// offset 1: attempt counter (1 byte)
// offset 2-33: salt (32 bytes)
// offset 34-45: nonce (12 bytes)
// offset 46-125: encrypted seed (80 bytes max, 64 bytes seed + 16 bytes GCM tag)
const NVS_BASE_ADDR: u32 = 0x9000; // Custom data partition
const NVS_SIZE: usize = 128;

#[derive(Debug)]
pub enum NvsError {
    Flash,
    Crypto,
    NotInitialized,
    WrongPin,
    LockedOut,
    AlreadyInitialized,
}

pub struct NvsStore {
    flash: FlashStorage,
}

impl NvsStore {
    pub fn new() -> Self {
        Self {
            flash: FlashStorage::new(),
        }
    }

    pub fn is_initialized(&mut self) -> bool {
        let mut buf = [0u8; 1];
        if self.flash.read(NVS_BASE_ADDR, &mut buf).is_ok() {
            buf[0] == 1
        } else {
            false
        }
    }

    pub fn get_attempts_remaining(&mut self) -> u8 {
        let mut buf = [0u8; 1];
        if self.flash.read(NVS_BASE_ADDR + 1, &mut buf).is_ok() {
            MAX_PIN_ATTEMPTS.saturating_sub(buf[0])
        } else {
            MAX_PIN_ATTEMPTS
        }
    }

    fn increment_attempts(&mut self) -> Result<(), NvsError> {
        let mut buf = [0u8; 1];
        let current = if self.flash.read(NVS_BASE_ADDR + 1, &mut buf).is_ok() {
            buf[0]
        } else {
            0
        };

        buf[0] = current + 1;
        self.flash
            .write(NVS_BASE_ADDR + 1, &buf)
            .map_err(|_| NvsError::Flash)?;
        Ok(())
    }

    fn reset_attempts(&mut self) -> Result<(), NvsError> {
        let buf = [0u8; 1];
        self.flash
            .write(NVS_BASE_ADDR + 1, &buf)
            .map_err(|_| NvsError::Flash)?;
        Ok(())
    }

    pub fn initialize_pin(&mut self, pin: &str, seed64: &[u8; 64]) -> Result<(), NvsError> {
        if self.is_initialized() {
            return Err(NvsError::AlreadyInitialized);
        }

        // generate random salt and nonce
        let mut salt = [0u8; 32];
        let mut nonce_bytes = [0u8; 12];

        // Use getrandom for salt and nonce
        getrandom::getrandom(&mut salt).map_err(|_| NvsError::Crypto)?;
        getrandom::getrandom(&mut nonce_bytes).map_err(|_| NvsError::Crypto)?;

        // derive key from pin
        let mut key = [0u8; 32];
        pbkdf2::<Hmac<Sha256>>(pin.as_bytes(), &salt, PBKDF2_ROUNDS, &mut key)
            .map_err(|_| NvsError::Crypto)?;

        // encrypt seed
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| NvsError::Crypto)?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let encrypted = cipher
            .encrypt(nonce, &seed64[..])
            .map_err(|_| NvsError::Crypto)?;

        // write to flash
        let init_flag = [1u8];
        self.flash
            .write(NVS_BASE_ADDR, &init_flag)
            .map_err(|_| NvsError::Flash)?;

        let attempts = [0u8];
        self.flash
            .write(NVS_BASE_ADDR + 1, &attempts)
            .map_err(|_| NvsError::Flash)?;

        self.flash
            .write(NVS_BASE_ADDR + 2, &salt)
            .map_err(|_| NvsError::Flash)?;
        self.flash
            .write(NVS_BASE_ADDR + 34, &nonce_bytes)
            .map_err(|_| NvsError::Flash)?;
        self.flash
            .write(NVS_BASE_ADDR + 46, &encrypted)
            .map_err(|_| NvsError::Flash)?;

        key.zeroize();
        Ok(())
    }

    pub fn unlock(&mut self, pin: &str) -> Result<[u8; 64], NvsError> {
        if !self.is_initialized() {
            return Err(NvsError::NotInitialized);
        }

        if self.get_attempts_remaining() == 0 {
            return Err(NvsError::LockedOut);
        }

        // read salt/nonce/enc seed from flash
        let mut salt = [0u8; 32];
        let mut nonce_bytes = [0u8; 12];
        let mut encrypted = Vec::new();
        encrypted.resize(80, 0); // 64 bytes + 16 bytes gcm tag

        self.flash
            .read(NVS_BASE_ADDR + 2, &mut salt)
            .map_err(|_| NvsError::Flash)?;
        self.flash
            .read(NVS_BASE_ADDR + 34, &mut nonce_bytes)
            .map_err(|_| NvsError::Flash)?;
        self.flash
            .read(NVS_BASE_ADDR + 46, &mut encrypted)
            .map_err(|_| NvsError::Flash)?;

        // Derive key from pin
        let mut key = [0u8; 32];
        pbkdf2::<Hmac<Sha256>>(pin.as_bytes(), &salt, PBKDF2_ROUNDS, &mut key)
            .map_err(|_| NvsError::Crypto)?;

        // Decrypt seed
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| NvsError::Crypto)?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let decrypted = cipher.decrypt(nonce, encrypted.as_ref());

        key.zeroize();

        match decrypted {
            Ok(seed_vec) => {
                if seed_vec.len() != 64 {
                    return Err(NvsError::Crypto);
                }

                // reset attempts on success
                self.reset_attempts()?;

                let mut seed = [0u8; 64];
                seed.copy_from_slice(&seed_vec);
                Ok(seed)
            }
            Err(_) => {
                // increment failed attempts
                self.increment_attempts()?;
                Err(NvsError::WrongPin)
            }
        }
    }

    pub fn change_pin(&mut self, old_pin: &str, new_pin: &str) -> Result<(), NvsError> {
        // first unlock with old pin to get seed
        let seed = self.unlock(old_pin)?;

        // re-encrypt with new pin (reusing initialize logic)
        // clear initialized flag temporarily
        let init_flag = [0u8];
        self.flash
            .write(NVS_BASE_ADDR, &init_flag)
            .map_err(|_| NvsError::Flash)?;

        // re-initialize with new pin
        self.initialize_pin(new_pin, &seed)?;

        Ok(())
    }

    pub fn wipe(&mut self) -> Result<(), NvsError> {
        // zero out all stored data
        let zeros = [0u8; 128];
        self.flash
            .write(NVS_BASE_ADDR, &zeros)
            .map_err(|_| NvsError::Flash)?;
        Ok(())
    }
}
