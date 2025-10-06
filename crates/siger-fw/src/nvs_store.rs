use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use alloc::vec;
use alloc::vec::Vec;
use core::result::Result;
use embedded_storage::{ReadStorage, Storage};
use esp_storage::FlashStorage;
use hmac::Hmac;
use pbkdf2::pbkdf2;
use sha2::Sha256;
use siger_core::alloc_path as pathmod;
use siger_core::{CheetahPub, MAX_SEED_SLOTS};
use zeroize::Zeroize;

const PBKDF2_ROUNDS: u32 = 100_000;
const MAX_PIN_ATTEMPTS: u8 = 10;

const NVS_BASE_ADDR: u32 = 0x9000;
const HEADER_SIZE: usize = 64;
const SLOT_SIZE: usize = 192;
const NVS_TOTAL_SIZE: usize = HEADER_SIZE + SLOT_SIZE * MAX_SEED_SLOTS;

const MAGIC: [u8; 4] = *b"SGR1";
const VERSION: u8 = 1;
const FLAG_INITIALIZED: u8 = 0x01;
const SLOT_USED: u8 = 0x01;

#[derive(Debug)]
pub enum NvsError {
    Flash,
    Crypto,
    NotInitialized,
    WrongPin,
    LockedOut,
    AlreadyInitialized,
    Full,
}

pub struct NvsStore {
    flash: FlashStorage,
}

#[derive(Clone)]
struct Header {
    flags: u8,
    attempts: u8,
    slot_count: u8,
    salt: [u8; 32],
}

impl Header {
    fn new_uninitialized() -> Self {
        Self {
            flags: 0,
            attempts: 0,
            slot_count: 0,
            salt: [0u8; 32],
        }
    }

    fn initialized(&self) -> bool {
        self.flags & FLAG_INITIALIZED != 0
    }

    fn set_initialized(&mut self) {
        self.flags |= FLAG_INITIALIZED;
    }

    fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut out = [0xFFu8; HEADER_SIZE];
        out[..MAGIC.len()].copy_from_slice(&MAGIC);
        out[4] = VERSION;
        out[5] = self.flags;
        out[6] = self.attempts;
        out[7] = self.slot_count;
        out[8..40].copy_from_slice(&self.salt);
        out
    }

    fn from_bytes(buf: &[u8; HEADER_SIZE]) -> Option<Self> {
        if &buf[..MAGIC.len()] != MAGIC.as_ref() {
            return None;
        }
        if buf[4] != VERSION {
            return None;
        }
        let mut salt = [0u8; 32];
        salt.copy_from_slice(&buf[8..40]);
        Some(Self {
            flags: buf[5],
            attempts: buf[6],
            slot_count: buf[7],
            salt,
        })
    }
}

#[derive(Clone)]
struct SlotRecord {
    nonce: [u8; 12],
    enc_seed: [u8; 80],
    pub_x: [u64; 6],
    pub_y: [u64; 6],
}

impl SlotRecord {
    fn to_bytes(&self) -> [u8; SLOT_SIZE] {
        let mut out = [0xFFu8; SLOT_SIZE];
        out[0] = SLOT_USED;
        out[4..16].copy_from_slice(&self.nonce);
        out[16..96].copy_from_slice(&self.enc_seed);
        write_u64_array(&mut out[96..144], &self.pub_x);
        write_u64_array(&mut out[144..192], &self.pub_y);
        out
    }

    fn from_bytes(buf: &[u8; SLOT_SIZE]) -> Option<Self> {
        if buf[0] != SLOT_USED {
            return None;
        }
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&buf[4..16]);
        let mut enc_seed = [0u8; 80];
        enc_seed.copy_from_slice(&buf[16..96]);
        let mut pub_x = [0u64; 6];
        read_u64_array(&buf[96..144], &mut pub_x);
        let mut pub_y = [0u64; 6];
        read_u64_array(&buf[144..192], &mut pub_y);
        Some(Self {
            nonce,
            enc_seed,
            pub_x,
            pub_y,
        })
    }
}

fn read_u64_array(src: &[u8], dst: &mut [u64]) {
    for (chunk, out) in src.chunks_exact(8).zip(dst.iter_mut()) {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(chunk);
        *out = u64::from_le_bytes(bytes);
    }
}

fn write_u64_array(dst: &mut [u8], values: &[u64]) {
    for (chunk, val) in dst.chunks_exact_mut(8).zip(values.iter()) {
        chunk.copy_from_slice(&val.to_le_bytes());
    }
}

impl NvsStore {
    pub fn new() -> Self {
        Self {
            flash: FlashStorage::new(),
        }
    }

    pub fn is_initialized(&mut self) -> bool {
        matches!(self.read_header(), Ok(Some(header)) if header.initialized())
    }

    pub fn get_attempts_remaining(&mut self) -> u8 {
        match self.read_header() {
            Ok(Some(header)) if header.initialized() => {
                MAX_PIN_ATTEMPTS.saturating_sub(header.attempts)
            }
            _ => MAX_PIN_ATTEMPTS,
        }
    }

    pub fn initialize_pin(
        &mut self,
        pin: &str,
        seed64: &[u8; 64],
        pub_xy: ([u64; 6], [u64; 6]),
    ) -> Result<u8, NvsError> {
        if let Ok(Some(header)) = self.read_header() {
            if header.initialized() {
                return Err(NvsError::AlreadyInitialized);
            }
        }

        let mut header = Header::new_uninitialized();
        header.slot_count = 1;
        header.set_initialized();
        getrandom::getrandom(&mut header.salt).map_err(|_| NvsError::Crypto)?;

        let key = Self::derive_master_key(pin, &header.salt)?;
        let slot_record = self.encrypt_seed_record(&key, seed64, pub_xy)?;
        drop(key);

        self.write_header(&header)?;
        self.write_slot(0, &slot_record)?;
        Ok(0)
    }

    pub fn add_seed_with_key(
        &mut self,
        master_key: &[u8; 32],
        seed64: &[u8; 64],
        pub_xy: ([u64; 6], [u64; 6]),
    ) -> Result<u8, NvsError> {
        let mut header = self.read_header()?.ok_or(NvsError::NotInitialized)?;
        if !header.initialized() {
            return Err(NvsError::NotInitialized);
        }
        if header.attempts >= MAX_PIN_ATTEMPTS {
            return Err(NvsError::LockedOut);
        }
        if header.slot_count as usize >= MAX_SEED_SLOTS {
            return Err(NvsError::Full);
        }

        let slot_index = header.slot_count as usize;
        if self.verify_master_key(master_key, &header).is_err() {
            self.increment_attempts(&mut header)?;
            return Err(NvsError::WrongPin);
        }

        let slot_record = self.encrypt_seed_record(master_key, seed64, pub_xy)?;

        self.write_slot(slot_index, &slot_record)?;
        header.slot_count += 1;
        header.attempts = 0;
        self.write_header(&header)?;
        Ok(slot_index as u8)
    }

    pub fn unlock(&mut self, pin: &str) -> Result<(Vec<[u8; 64]>, [u8; 32]), NvsError> {
        let mut header = self.read_header()?.ok_or(NvsError::NotInitialized)?;
        if !header.initialized() {
            return Err(NvsError::NotInitialized);
        }
        if header.attempts >= MAX_PIN_ATTEMPTS {
            return Err(NvsError::LockedOut);
        }

        let key = Self::derive_master_key(pin, &header.salt)?;
        let mut seeds = Vec::new();
        seeds.reserve_exact(header.slot_count as usize);

        for index in 0..header.slot_count as usize {
            let record = match self.read_slot(index)? {
                Some(r) => r,
                None => continue,
            };
            match self.decrypt_seed(&key, &record) {
                Ok(seed) => seeds.push(seed),
                Err(_) => {
                    self.increment_attempts(&mut header)?;
                    return Err(NvsError::WrongPin);
                }
            }
        }

        header.attempts = 0;
        self.write_header(&header)?;
        Ok((seeds, key))
    }

    pub fn derive_master_key_for_pin(&mut self, pin: &str) -> Result<[u8; 32], NvsError> {
        let mut header = self.read_header()?.ok_or(NvsError::NotInitialized)?;
        if !header.initialized() {
            return Err(NvsError::NotInitialized);
        }
        if header.attempts >= MAX_PIN_ATTEMPTS {
            return Err(NvsError::LockedOut);
        }

        let key = Self::derive_master_key(pin, &header.salt)?;
        if self.verify_master_key(&key, &header).is_err() {
            self.increment_attempts(&mut header)?;
            return Err(NvsError::WrongPin);
        }

        header.attempts = 0;
        self.write_header(&header)?;
        Ok(key)
    }

    pub fn change_pin(&mut self, old_pin: &str, new_pin: &str) -> Result<(), NvsError> {
        let (seeds, _) = self.unlock(old_pin)?;
        let pubs = self.list_seed_pubs()?;

        // wipe existing storage
        self.wipe()?;

        // initialize with new pin and previously stored metadata
        if seeds.is_empty() {
            return Ok(());
        }

        let mut header = Header::new_uninitialized();
        header.slot_count = pubs.len() as u8;
        header.set_initialized();
        getrandom::getrandom(&mut header.salt).map_err(|_| NvsError::Crypto)?;

        let key = Self::derive_master_key(new_pin, &header.salt)?;

        for (index, (seed, pubinfo)) in seeds.iter().zip(pubs.iter()).enumerate() {
            let record = self.encrypt_seed_record(&key, seed, (pubinfo.x, pubinfo.y))?;
            self.write_slot(index, &record)?;
        }

        drop(key);
        header.attempts = 0;
        self.write_header(&header)?;
        Ok(())
    }

    pub fn wipe(&mut self) -> Result<(), NvsError> {
        let zeros = vec![0xFFu8; NVS_TOTAL_SIZE];
        self.flash
            .write(NVS_BASE_ADDR, &zeros)
            .map_err(|_| NvsError::Flash)?;
        Ok(())
    }

    pub fn factory_reset(&mut self) -> Result<(), NvsError> {
        self.wipe()
    }

    pub fn list_seed_pubs(&mut self) -> Result<Vec<CheetahPub>, NvsError> {
        let header = match self.read_header()? {
            Some(h) if h.initialized() => h,
            _ => return Ok(Vec::new()),
        };

        let mut pubs = Vec::new();
        pubs.reserve_exact(header.slot_count as usize);

        for index in 0..header.slot_count as usize {
            if let Some(record) = self.read_slot(index)? {
                let mut path = pathmod::Path::new();
                let slot = index as u8;
                // Keep path empty (root) for now.
                pubs.push(CheetahPub {
                    slot,
                    path,
                    x: record.pub_x,
                    y: record.pub_y,
                });
            }
        }

        Ok(pubs)
    }

    pub fn get_salt(&mut self) -> Result<[u8; 32], NvsError> {
        let header = self.read_header()?.ok_or(NvsError::NotInitialized)?;
        Ok(header.salt)
    }

    fn derive_master_key(pin: &str, salt: &[u8; 32]) -> Result<[u8; 32], NvsError> {
        let mut key = [0u8; 32];
        pbkdf2::<Hmac<Sha256>>(pin.as_bytes(), salt, PBKDF2_ROUNDS, &mut key)
            .map_err(|_| NvsError::Crypto)?;
        Ok(key)
    }

    fn verify_master_key(&mut self, key: &[u8; 32], header: &Header) -> Result<(), NvsError> {
        if header.slot_count == 0 {
            return Ok(());
        }
        if let Some(record) = self.read_slot(0)? {
            self.decrypt_seed(key, &record).map(|_| ())
        } else {
            Ok(())
        }
    }

    fn encrypt_seed_record(
        &mut self,
        key: &[u8; 32],
        seed: &[u8; 64],
        pub_xy: ([u64; 6], [u64; 6]),
    ) -> Result<SlotRecord, NvsError> {
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| NvsError::Crypto)?;
        let mut nonce_bytes = [0u8; 12];
        getrandom::getrandom(&mut nonce_bytes).map_err(|_| NvsError::Crypto)?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let encrypted = cipher
            .encrypt(nonce, seed.as_ref())
            .map_err(|_| NvsError::Crypto)?;
        if encrypted.len() != 80 {
            return Err(NvsError::Crypto);
        }
        let mut enc_seed = [0u8; 80];
        enc_seed.copy_from_slice(&encrypted);

        Ok(SlotRecord {
            nonce: nonce_bytes,
            enc_seed,
            pub_x: pub_xy.0,
            pub_y: pub_xy.1,
        })
    }

    fn decrypt_seed(&self, key: &[u8; 32], record: &SlotRecord) -> Result<[u8; 64], NvsError> {
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| NvsError::Crypto)?;
        let nonce = Nonce::from_slice(&record.nonce);
        let decrypted = cipher
            .decrypt(nonce, record.enc_seed.as_ref())
            .map_err(|_| NvsError::Crypto)?;
        if decrypted.len() != 64 {
            return Err(NvsError::Crypto);
        }
        let mut seed = [0u8; 64];
        seed.copy_from_slice(&decrypted);
        Ok(seed)
    }

    fn read_header(&mut self) -> Result<Option<Header>, NvsError> {
        let mut buf = [0u8; HEADER_SIZE];
        self.flash
            .read(NVS_BASE_ADDR, &mut buf)
            .map_err(|_| NvsError::Flash)?;
        if buf.iter().all(|b| *b == 0xFF) {
            return Ok(None);
        }
        Ok(Header::from_bytes(&buf))
    }

    fn write_header(&mut self, header: &Header) -> Result<(), NvsError> {
        let buf = header.to_bytes();
        self.flash
            .write(NVS_BASE_ADDR, &buf)
            .map_err(|_| NvsError::Flash)
    }

    fn read_slot(&mut self, index: usize) -> Result<Option<SlotRecord>, NvsError> {
        if index >= MAX_SEED_SLOTS {
            return Ok(None);
        }
        let mut buf = [0u8; SLOT_SIZE];
        let offset = NVS_BASE_ADDR + HEADER_SIZE as u32 + (index * SLOT_SIZE) as u32;
        self.flash
            .read(offset, &mut buf)
            .map_err(|_| NvsError::Flash)?;
        if buf.iter().all(|b| *b == 0xFF) {
            return Ok(None);
        }
        Ok(SlotRecord::from_bytes(&buf))
    }

    fn write_slot(&mut self, index: usize, record: &SlotRecord) -> Result<(), NvsError> {
        if index >= MAX_SEED_SLOTS {
            return Err(NvsError::Full);
        }
        let buf = record.to_bytes();
        let offset = NVS_BASE_ADDR + HEADER_SIZE as u32 + (index * SLOT_SIZE) as u32;
        self.flash.write(offset, &buf).map_err(|_| NvsError::Flash)
    }

    fn increment_attempts(&mut self, header: &mut Header) -> Result<(), NvsError> {
        if header.attempts < MAX_PIN_ATTEMPTS {
            header.attempts = header.attempts.saturating_add(1);
            self.write_header(header)?;
        }
        Ok(())
    }
}
