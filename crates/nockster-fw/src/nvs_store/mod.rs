use aes_gcm::aead::{AeadInPlace, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce, Tag};
use alloc::vec;
use alloc::vec::Vec;
use core::result::Result;
use embedded_storage::ReadStorage;
use esp_storage::FlashStorage;
use hmac::Hmac;
use nockster_core::alloc_path as pathmod;
use nockster_core::{
    CheetahPub, SeedSlotLabel, TouchCalibration, MAX_SEED_LABEL_LEN, MAX_SEED_SLOTS,
};
use pbkdf2::pbkdf2;
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

const PBKDF2_ROUNDS: u32 = 100_000;
const MAX_PIN_ATTEMPTS: u8 = 10;

const NVS_BASE_ADDR: u32 = 0x9000;
const NVS_SECTOR_SIZE: usize = FlashStorage::SECTOR_SIZE as usize;
const HEADER_SIZE: usize = 64;
const SLOT_SIZE: usize = 192;
const SEED_TOTAL_SIZE: usize = HEADER_SIZE + SLOT_SIZE * MAX_SEED_SLOTS;
const CALIBRATION_SIZE: usize = 64;
const LABELS_HEADER_SIZE: usize = 8;
const LABELS_SIZE: usize = LABELS_HEADER_SIZE + MAX_SEED_LABEL_LEN * MAX_SEED_SLOTS;
const NVS_TOTAL_SIZE: usize = SEED_TOTAL_SIZE + CALIBRATION_SIZE + LABELS_SIZE;
const NVS_SECTOR_END: u32 = NVS_BASE_ADDR + NVS_SECTOR_SIZE as u32;
const CALIBRATION_ADDR: u32 = NVS_BASE_ADDR + SEED_TOTAL_SIZE as u32;
const LABELS_ADDR: u32 = CALIBRATION_ADDR + CALIBRATION_SIZE as u32;

const MAGIC: [u8; 4] = *b"NCK1";
const LEGACY_MAGIC: [u8; 4] = [b'S', b'G', b'R', b'1'];
const CALIBRATION_MAGIC: [u8; 4] = *b"NCTC";
const LEGACY_CALIBRATION_MAGIC: [u8; 4] = [b'S', b'G', b'T', b'C'];
const LABELS_MAGIC: [u8; 4] = *b"NCLB";
const LEGACY_LABELS_MAGIC: [u8; 4] = [b'S', b'G', b'L', b'B'];
const VERSION_V1: u8 = 1;
const VERSION_V2: u8 = 2;
const VERSION: u8 = VERSION_V1;
pub const NVS_V2_PEPPER_DOMAIN: &[u8] = b"nockster-nvs-v2";
pub const NVS_V2_MASTER_DOMAIN: &[u8] = b"nockster-nvs-master-v2";
pub const NVS_V2_PEPPER_MESSAGE_LEN: usize = 15 + 32 + 6;
const CALIBRATION_VERSION: u8 = 1;
const LABELS_VERSION: u8 = 1;
const FLAG_INITIALIZED: u8 = 0x01;
const SLOT_USED: u8 = 0x01;
const CALIBRATION_MIRROR_X: u8 = 0x01;
const CALIBRATION_MIRROR_Y: u8 = 0x02;

#[derive(Debug)]
pub enum NvsError {
    Flash,
    Crypto,
    NotInitialized,
    WrongPin,
    LockedOut,
    AlreadyInitialized,
    Full,
    InvalidSlot,
    InvalidCalibration,
    InvalidLabel,
}

pub struct NvsStore {
    flash: FlashStorage,
}

pub trait NvsPepperSource {
    fn nvs_v2_pepper(&mut self, salt: &[u8; 32]) -> Result<Option<[u8; 32]>, NvsError>;
}

#[derive(Clone, Copy)]
pub enum NvsInitStage {
    ReadHeader,
    WipePartial,
    RandomSalt,
    Pepper,
    Kdf,
    KdfDone,
    EncryptSeed,
    WriteHeaderPending,
    WriteSlot,
    WriteHeaderFinal,
    WriteLabels,
    Complete,
}

pub struct NoNvsPepper;

impl NvsPepperSource for NoNvsPepper {
    fn nvs_v2_pepper(&mut self, _salt: &[u8; 32]) -> Result<Option<[u8; 32]>, NvsError> {
        Ok(None)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NvsStorageStatus {
    pub initialized: bool,
    pub schema_version: u8,
    pub slot_count: u8,
}

#[derive(Clone)]
struct Header {
    version: u8,
    flags: u8,
    attempts: u8,
    slot_count: u8,
    salt: [u8; 32],
}

impl Header {
    fn new_uninitialized(version: u8) -> Self {
        Self {
            version,
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
        out[4] = self.version;
        out[5] = self.flags;
        out[6] = self.attempts;
        out[7] = self.slot_count;
        out[8..40].copy_from_slice(&self.salt);
        out
    }

    fn from_bytes(buf: &[u8; HEADER_SIZE]) -> Option<Self> {
        if !matches_magic(buf, &MAGIC, &LEGACY_MAGIC) {
            return None;
        }
        let version = buf[4];
        if version != VERSION_V1 && version != VERSION_V2 {
            return None;
        }
        let is_legacy_magic = &buf[..LEGACY_MAGIC.len()] == LEGACY_MAGIC.as_ref();
        if is_legacy_magic && version != VERSION_V1 {
            return None;
        }
        let mut salt = [0u8; 32];
        salt.copy_from_slice(&buf[8..40]);
        let header = Self {
            version,
            flags: buf[5],
            attempts: buf[6],
            slot_count: buf[7],
            salt,
        };

        // Reject obviously corrupt headers. This avoids getting stuck in an
        // "initialized but impossible" state if a flash write is interrupted.
        if header.flags & !FLAG_INITIALIZED != 0 {
            return None;
        }
        if header.attempts > MAX_PIN_ATTEMPTS {
            return None;
        }
        if header.slot_count as usize > MAX_SEED_SLOTS {
            return None;
        }
        if header.initialized() {
            if header.slot_count == 0 {
                return None;
            }
            // Salt should never be all-0xFF in a valid initialized header
            // (we set it via getrandom).
            if header.salt.iter().all(|b| *b == 0xFF) {
                return None;
            }
        }

        Some(header)
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

fn touch_calibration_valid(calibration: &TouchCalibration) -> bool {
    calibration.raw_x_min < calibration.raw_x_max && calibration.raw_y_min < calibration.raw_y_max
}

fn seed_label_valid(label: &str) -> bool {
    label.len() <= MAX_SEED_LABEL_LEN
        && label
            .bytes()
            .all(|byte| byte == b' ' || (0x21..=0x7e).contains(&byte))
}

fn matches_magic(buf: &[u8], current: &[u8; 4], legacy: &[u8; 4]) -> bool {
    let stored = &buf[..current.len()];
    stored == current.as_ref() || stored == legacy.as_ref()
}

fn relative_nvs_offset(address: u32, len: usize) -> Result<usize, NvsError> {
    if address < NVS_BASE_ADDR {
        return Err(NvsError::Flash);
    }
    let start = (address - NVS_BASE_ADDR) as usize;
    let Some(end) = start.checked_add(len) else {
        return Err(NvsError::Flash);
    };
    if end > NVS_TOTAL_SIZE {
        return Err(NvsError::Flash);
    }
    Ok(start)
}

fn slot_sector_offset(index: usize) -> Result<usize, NvsError> {
    if index >= MAX_SEED_SLOTS {
        return Err(NvsError::InvalidSlot);
    }
    relative_nvs_offset(
        NVS_BASE_ADDR + HEADER_SIZE as u32 + (index * SLOT_SIZE) as u32,
        SLOT_SIZE,
    )
}

pub fn nvs_v2_pepper_message(salt: &[u8; 32], mac: &[u8; 6]) -> [u8; NVS_V2_PEPPER_MESSAGE_LEN] {
    let mut out = [0u8; NVS_V2_PEPPER_MESSAGE_LEN];
    let mut offset = 0usize;
    out[offset..offset + NVS_V2_PEPPER_DOMAIN.len()].copy_from_slice(NVS_V2_PEPPER_DOMAIN);
    offset += NVS_V2_PEPPER_DOMAIN.len();
    out[offset..offset + salt.len()].copy_from_slice(salt);
    offset += salt.len();
    out[offset..offset + mac.len()].copy_from_slice(mac);
    out
}

fn zeroize_optional_secret(value: &mut Option<[u8; 32]>) {
    if let Some(secret) = value.as_mut() {
        secret.zeroize();
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

    pub fn storage_status(&mut self) -> NvsStorageStatus {
        match self.read_header() {
            Ok(Some(header)) => NvsStorageStatus {
                initialized: header.initialized(),
                schema_version: header.version,
                slot_count: if header.initialized() {
                    header.slot_count
                } else {
                    0
                },
            },
            _ => NvsStorageStatus {
                initialized: false,
                schema_version: 0,
                slot_count: 0,
            },
        }
    }

    pub fn initialize_pin(
        &mut self,
        pin: &str,
        seed64: &[u8; 64],
        pub_xy: ([u64; 6], [u64; 6]),
    ) -> Result<([u8; 32], u8), NvsError> {
        let mut pepper = NoNvsPepper;
        self.initialize_pin_with_pepper(pin, seed64, pub_xy, &mut pepper)
    }

    pub fn initialize_pin_with_pepper<P: NvsPepperSource>(
        &mut self,
        pin: &str,
        seed64: &[u8; 64],
        pub_xy: ([u64; 6], [u64; 6]),
        pepper_source: &mut P,
    ) -> Result<([u8; 32], u8), NvsError> {
        self.initialize_pin_with_pepper_and_progress(pin, seed64, pub_xy, pepper_source, |_| {})
    }

    pub fn initialize_pin_with_pepper_and_progress<P, F>(
        &mut self,
        pin: &str,
        seed64: &[u8; 64],
        pub_xy: ([u64; 6], [u64; 6]),
        pepper_source: &mut P,
        mut progress: F,
    ) -> Result<([u8; 32], u8), NvsError>
    where
        P: NvsPepperSource,
        F: FnMut(NvsInitStage),
    {
        progress(NvsInitStage::ReadHeader);
        if let Ok(Some(header)) = self.read_header() {
            if header.initialized() {
                return Err(NvsError::AlreadyInitialized);
            }
            progress(NvsInitStage::WipePartial);
            self.wipe_seed_storage()?;
        }

        let mut header = Header::new_uninitialized(VERSION);
        header.slot_count = 1;
        progress(NvsInitStage::RandomSalt);
        getrandom::getrandom(&mut header.salt).map_err(|_| NvsError::Crypto)?;

        progress(NvsInitStage::Pepper);
        let mut maybe_pepper = Self::pepper_for_new_header(&mut header, pepper_source)?;
        progress(NvsInitStage::Kdf);
        let key = Self::derive_master_key_for_header(pin, &header, maybe_pepper.as_ref())?;
        progress(NvsInitStage::KdfDone);
        zeroize_optional_secret(&mut maybe_pepper);
        progress(NvsInitStage::EncryptSeed);
        let slot_record = self.encrypt_seed_record(&key, seed64, pub_xy)?;

        // Atomic-ish initialization: write the header without the initialized flag,
        // then write the slot, then finally mark initialized.
        //
        // This prevents leaving the device in a "looks initialized" state if the
        // slot write fails partway through.
        progress(NvsInitStage::WriteHeaderPending);
        self.write_header(&header)?;
        progress(NvsInitStage::WriteSlot);
        self.write_slot(0, &slot_record)?;
        header.set_initialized();
        progress(NvsInitStage::WriteHeaderFinal);
        self.write_header(&header)?;
        progress(NvsInitStage::WriteLabels);
        let _ = self.write_seed_label_records(&[]);
        progress(NvsInitStage::Complete);
        Ok((key, 0))
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
        let mut pepper = NoNvsPepper;
        self.unlock_with_pepper(pin, &mut pepper)
    }

    pub fn unlock_with_pepper<P: NvsPepperSource>(
        &mut self,
        pin: &str,
        pepper_source: &mut P,
    ) -> Result<(Vec<[u8; 64]>, [u8; 32]), NvsError> {
        self.unlock_with_optional_v2_migration(pin, pepper_source, false)
            .map(|(seeds, key, _migrated)| (seeds, key))
    }

    pub fn unlock_with_optional_v2_migration<P: NvsPepperSource>(
        &mut self,
        pin: &str,
        pepper_source: &mut P,
        migrate_v1_to_v2: bool,
    ) -> Result<(Vec<[u8; 64]>, [u8; 32], bool), NvsError> {
        let mut header = self.read_header()?.ok_or(NvsError::NotInitialized)?;
        if !header.initialized() {
            return Err(NvsError::NotInitialized);
        }
        if header.attempts >= MAX_PIN_ATTEMPTS {
            return Err(NvsError::LockedOut);
        }

        let mut key = Self::derive_master_key_with_source(pin, &header, pepper_source)?;
        let mut seeds = Vec::new();
        let mut pubs = Vec::new();
        if seeds.try_reserve_exact(header.slot_count as usize).is_err()
            || pubs.try_reserve_exact(header.slot_count as usize).is_err()
        {
            key.zeroize();
            return Err(NvsError::Crypto);
        }

        for index in 0..header.slot_count as usize {
            let record = match self.read_slot(index)? {
                Some(r) => r,
                None => continue,
            };
            match self.decrypt_seed(&key, &record) {
                Ok(seed) => {
                    seeds.push(seed);
                    pubs.push(CheetahPub {
                        slot: index as u8,
                        path: pathmod::Path::new(),
                        x: record.pub_x,
                        y: record.pub_y,
                    });
                }
                Err(_) => {
                    self.increment_attempts(&mut header)?;
                    return Err(NvsError::WrongPin);
                }
            }
        }

        if migrate_v1_to_v2
            && header.version == VERSION_V1
            && seeds.len() == header.slot_count as usize
            && self.v2_pepper_available(&header, pepper_source)?
        {
            let labels = self.read_seed_labels().unwrap_or_default();
            let calibration = self.read_touch_calibration().ok().flatten();
            let new_key = self.rewrite_seed_storage(
                pin,
                seeds.as_slice(),
                pubs.as_slice(),
                labels.as_slice(),
                calibration,
                pepper_source,
            )?;
            key.zeroize();
            return Ok((seeds, new_key, true));
        }

        self.clear_attempts_if_needed(&mut header)?;
        Ok((seeds, key, false))
    }

    pub fn derive_master_key_for_pin(&mut self, pin: &str) -> Result<[u8; 32], NvsError> {
        let mut pepper = NoNvsPepper;
        self.derive_master_key_for_pin_with_pepper(pin, &mut pepper)
    }

    pub fn derive_master_key_for_pin_with_pepper<P: NvsPepperSource>(
        &mut self,
        pin: &str,
        pepper_source: &mut P,
    ) -> Result<[u8; 32], NvsError> {
        let mut header = self.read_header()?.ok_or(NvsError::NotInitialized)?;
        if !header.initialized() {
            return Err(NvsError::NotInitialized);
        }
        if header.attempts >= MAX_PIN_ATTEMPTS {
            return Err(NvsError::LockedOut);
        }

        let key = Self::derive_master_key_with_source(pin, &header, pepper_source)?;
        if self.verify_master_key(&key, &header).is_err() {
            self.increment_attempts(&mut header)?;
            return Err(NvsError::WrongPin);
        }

        self.clear_attempts_if_needed(&mut header)?;
        Ok(key)
    }

    pub fn delete_seed_with_key(
        &mut self,
        master_key: &[u8; 32],
        slot_index: usize,
    ) -> Result<(), NvsError> {
        let mut header = self.read_header()?.ok_or(NvsError::NotInitialized)?;
        if !header.initialized() {
            return Err(NvsError::NotInitialized);
        }
        if header.attempts >= MAX_PIN_ATTEMPTS {
            return Err(NvsError::LockedOut);
        }
        if slot_index >= header.slot_count as usize {
            return Err(NvsError::InvalidSlot);
        }

        if self.verify_master_key(master_key, &header).is_err() {
            self.increment_attempts(&mut header)?;
            return Err(NvsError::WrongPin);
        }

        let labels_before_delete = self.read_seed_labels().unwrap_or_default();
        let last_index = header.slot_count.saturating_sub(1) as usize;
        header.slot_count = header.slot_count.saturating_sub(1);
        if header.slot_count == 0 {
            header.flags &= !FLAG_INITIALIZED;
        }
        header.attempts = 0;
        self.delete_seed_records_transaction(
            &header,
            slot_index,
            last_index,
            labels_before_delete.as_slice(),
        )?;
        Ok(())
    }

    pub fn change_pin(&mut self, old_pin: &str, new_pin: &str) -> Result<(), NvsError> {
        let mut pepper = NoNvsPepper;
        self.change_pin_with_pepper(old_pin, new_pin, &mut pepper)
    }

    pub fn change_pin_with_pepper<P: NvsPepperSource>(
        &mut self,
        old_pin: &str,
        new_pin: &str,
        pepper_source: &mut P,
    ) -> Result<(), NvsError> {
        let (seeds, mut old_key) = self.unlock_with_pepper(old_pin, pepper_source)?;
        old_key.zeroize();
        let pubs = self.list_seed_pubs()?;
        let labels = self.read_seed_labels().unwrap_or_default();
        let calibration = self.read_touch_calibration().ok().flatten();

        if seeds.is_empty() {
            return Ok(());
        }

        let mut new_key = self.rewrite_seed_storage(
            new_pin,
            seeds.as_slice(),
            pubs.as_slice(),
            labels.as_slice(),
            calibration,
            pepper_source,
        )?;
        new_key.zeroize();
        Ok(())
    }

    fn rewrite_seed_storage<P: NvsPepperSource>(
        &mut self,
        pin: &str,
        seeds: &[[u8; 64]],
        pubs: &[CheetahPub],
        labels: &[SeedSlotLabel],
        calibration: Option<TouchCalibration>,
        pepper_source: &mut P,
    ) -> Result<[u8; 32], NvsError> {
        if seeds.is_empty() {
            return Err(NvsError::NotInitialized);
        }
        if seeds.len() != pubs.len() || seeds.len() > MAX_SEED_SLOTS {
            return Err(NvsError::Crypto);
        }

        let mut header = Header::new_uninitialized(VERSION);
        header.slot_count = seeds.len() as u8;
        header.set_initialized();
        getrandom::getrandom(&mut header.salt).map_err(|_| NvsError::Crypto)?;

        let mut maybe_pepper = Self::pepper_for_new_header(&mut header, pepper_source)?;
        let mut key = Self::derive_master_key_for_header(pin, &header, maybe_pepper.as_ref())?;
        zeroize_optional_secret(&mut maybe_pepper);

        let mut records = Vec::new();
        if records.try_reserve_exact(seeds.len()).is_err() {
            key.zeroize();
            return Err(NvsError::Crypto);
        }
        for (seed, pubinfo) in seeds.iter().zip(pubs.iter()) {
            let record = match self.encrypt_seed_record(&key, seed, (pubinfo.x, pubinfo.y)) {
                Ok(record) => record,
                Err(err) => {
                    key.zeroize();
                    return Err(err);
                }
            };
            records.push(record);
        }

        if let Err(err) = self.wipe() {
            key.zeroize();
            return Err(err);
        }
        for (index, record) in records.iter().enumerate() {
            if let Err(err) = self.write_slot(index, record) {
                key.zeroize();
                return Err(err);
            }
        }

        header.attempts = 0;
        if let Err(err) = self.write_header(&header) {
            key.zeroize();
            return Err(err);
        }
        let _ = self.write_seed_label_records(labels);
        if let Some(calibration) = calibration {
            let _ = self.write_touch_calibration(&calibration);
        }
        Ok(key)
    }

    pub fn wipe(&mut self) -> Result<(), NvsError> {
        embedded_storage::nor_flash::NorFlash::erase(
            &mut self.flash,
            NVS_BASE_ADDR,
            NVS_SECTOR_END,
        )
        .map_err(|_| NvsError::Flash)?;
        Ok(())
    }

    fn wipe_seed_storage(&mut self) -> Result<(), NvsError> {
        let zeros = vec![0xFFu8; SEED_TOTAL_SIZE];
        self.write_nvs_bytes(NVS_BASE_ADDR, &zeros)?;
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
                let path = pathmod::Path::new();
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

    pub fn read_seed_labels(&mut self) -> Result<Vec<SeedSlotLabel>, NvsError> {
        let header = match self.read_header()? {
            Some(h) if h.initialized() => h,
            _ => return Ok(Vec::new()),
        };

        let mut buf = [0u8; LABELS_SIZE];
        self.flash
            .read(LABELS_ADDR, &mut buf)
            .map_err(|_| NvsError::Flash)?;
        if buf.iter().all(|b| *b == 0xFF) {
            return Ok(Vec::new());
        }
        if !matches_magic(&buf, &LABELS_MAGIC, &LEGACY_LABELS_MAGIC) {
            return Ok(Vec::new());
        }
        if buf[4] != LABELS_VERSION {
            return Ok(Vec::new());
        }

        let mut labels = Vec::new();
        for slot in 0..header.slot_count as usize {
            let start = LABELS_HEADER_SIZE + slot * MAX_SEED_LABEL_LEN;
            let end = start + MAX_SEED_LABEL_LEN;
            let raw = &buf[start..end];
            let len = raw
                .iter()
                .position(|byte| *byte == 0 || *byte == 0xFF)
                .unwrap_or(MAX_SEED_LABEL_LEN);
            if len == 0 {
                continue;
            }
            let Ok(label_str) = core::str::from_utf8(&raw[..len]) else {
                continue;
            };
            if !seed_label_valid(label_str) {
                continue;
            }
            let mut label = heapless::String::<MAX_SEED_LABEL_LEN>::new();
            if label.push_str(label_str).is_err() {
                continue;
            }
            labels.push(SeedSlotLabel {
                slot: slot as u8,
                label,
            });
        }

        Ok(labels)
    }

    pub fn write_seed_label(&mut self, slot_index: usize, label: &str) -> Result<(), NvsError> {
        if !seed_label_valid(label) {
            return Err(NvsError::InvalidLabel);
        }
        let header = self.read_header()?.ok_or(NvsError::NotInitialized)?;
        if !header.initialized() {
            return Err(NvsError::NotInitialized);
        }
        if slot_index >= header.slot_count as usize {
            return Err(NvsError::InvalidSlot);
        }

        let mut labels = self.read_seed_labels().unwrap_or_default();
        labels.retain(|entry| entry.slot as usize != slot_index);
        if !label.is_empty() {
            let mut stored = heapless::String::<MAX_SEED_LABEL_LEN>::new();
            stored.push_str(label).map_err(|_| NvsError::InvalidLabel)?;
            labels.push(SeedSlotLabel {
                slot: slot_index as u8,
                label: stored,
            });
        }

        self.write_seed_label_records(&labels)
    }

    pub fn get_salt(&mut self) -> Result<[u8; 32], NvsError> {
        let header = self.read_header()?.ok_or(NvsError::NotInitialized)?;
        Ok(header.salt)
    }

    pub fn read_touch_calibration(&mut self) -> Result<Option<TouchCalibration>, NvsError> {
        let mut buf = [0u8; CALIBRATION_SIZE];
        self.flash
            .read(CALIBRATION_ADDR, &mut buf)
            .map_err(|_| NvsError::Flash)?;
        if buf.iter().all(|b| *b == 0xFF) {
            return Ok(None);
        }
        if !matches_magic(&buf, &CALIBRATION_MAGIC, &LEGACY_CALIBRATION_MAGIC) {
            return Ok(None);
        }
        if buf[4] != CALIBRATION_VERSION {
            return Ok(None);
        }
        let flags = buf[5];
        if flags & !(CALIBRATION_MIRROR_X | CALIBRATION_MIRROR_Y) != 0 {
            return Ok(None);
        }

        let calibration = TouchCalibration {
            raw_x_min: u16::from_le_bytes([buf[8], buf[9]]),
            raw_x_max: u16::from_le_bytes([buf[10], buf[11]]),
            raw_y_min: u16::from_le_bytes([buf[12], buf[13]]),
            raw_y_max: u16::from_le_bytes([buf[14], buf[15]]),
            mirror_x: flags & CALIBRATION_MIRROR_X != 0,
            mirror_y: flags & CALIBRATION_MIRROR_Y != 0,
        };
        if !touch_calibration_valid(&calibration) {
            return Ok(None);
        }

        Ok(Some(calibration))
    }

    pub fn write_touch_calibration(
        &mut self,
        calibration: &TouchCalibration,
    ) -> Result<(), NvsError> {
        if !touch_calibration_valid(calibration) {
            return Err(NvsError::InvalidCalibration);
        }

        let mut buf = [0xFFu8; CALIBRATION_SIZE];
        buf[..CALIBRATION_MAGIC.len()].copy_from_slice(&CALIBRATION_MAGIC);
        buf[4] = CALIBRATION_VERSION;
        let mut flags = 0u8;
        if calibration.mirror_x {
            flags |= CALIBRATION_MIRROR_X;
        }
        if calibration.mirror_y {
            flags |= CALIBRATION_MIRROR_Y;
        }
        buf[5] = flags;
        buf[8..10].copy_from_slice(&calibration.raw_x_min.to_le_bytes());
        buf[10..12].copy_from_slice(&calibration.raw_x_max.to_le_bytes());
        buf[12..14].copy_from_slice(&calibration.raw_y_min.to_le_bytes());
        buf[14..16].copy_from_slice(&calibration.raw_y_max.to_le_bytes());

        self.write_nvs_bytes(CALIBRATION_ADDR, &buf)
    }

    fn derive_master_key_v1(pin: &str, salt: &[u8; 32]) -> Result<[u8; 32], NvsError> {
        let mut key = [0u8; 32];
        pbkdf2::<Hmac<Sha256>>(pin.as_bytes(), salt, PBKDF2_ROUNDS, &mut key)
            .map_err(|_| NvsError::Crypto)?;
        Ok(key)
    }

    fn derive_master_key_v2(
        pin: &str,
        salt: &[u8; 32],
        pepper: &[u8; 32],
    ) -> Result<[u8; 32], NvsError> {
        let mut pin_key = Self::derive_master_key_v1(pin, salt)?;
        let mut h = Sha256::new();
        h.update(NVS_V2_MASTER_DOMAIN);
        h.update(pin_key.as_slice());
        h.update(pepper);
        let digest = h.finalize();
        pin_key.zeroize();

        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        Ok(out)
    }

    fn derive_master_key_for_header(
        pin: &str,
        header: &Header,
        pepper: Option<&[u8; 32]>,
    ) -> Result<[u8; 32], NvsError> {
        match header.version {
            VERSION_V1 => Self::derive_master_key_v1(pin, &header.salt),
            VERSION_V2 => {
                let pepper = pepper.ok_or(NvsError::Crypto)?;
                Self::derive_master_key_v2(pin, &header.salt, pepper)
            }
            _ => Err(NvsError::Crypto),
        }
    }

    fn derive_master_key_with_source<P: NvsPepperSource>(
        pin: &str,
        header: &Header,
        pepper_source: &mut P,
    ) -> Result<[u8; 32], NvsError> {
        let mut maybe_pepper = if header.version == VERSION_V2 {
            pepper_source.nvs_v2_pepper(&header.salt)?
        } else {
            None
        };
        let key = Self::derive_master_key_for_header(pin, header, maybe_pepper.as_ref())?;
        zeroize_optional_secret(&mut maybe_pepper);
        Ok(key)
    }

    fn pepper_for_new_header<P: NvsPepperSource>(
        header: &mut Header,
        pepper_source: &mut P,
    ) -> Result<Option<[u8; 32]>, NvsError> {
        let pepper = pepper_source.nvs_v2_pepper(&header.salt)?;
        header.version = if pepper.is_some() {
            VERSION_V2
        } else {
            VERSION_V1
        };
        Ok(pepper)
    }

    fn v2_pepper_available<P: NvsPepperSource>(
        &mut self,
        header: &Header,
        pepper_source: &mut P,
    ) -> Result<bool, NvsError> {
        let mut maybe_pepper = pepper_source.nvs_v2_pepper(&header.salt)?;
        let available = maybe_pepper.is_some();
        zeroize_optional_secret(&mut maybe_pepper);
        Ok(available)
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

        let mut enc_seed = [0u8; 80];
        enc_seed[..64].copy_from_slice(seed);
        let tag = cipher
            .encrypt_in_place_detached(nonce, b"", &mut enc_seed[..64])
            .map_err(|_| NvsError::Crypto)?;
        enc_seed[64..].copy_from_slice(tag.as_slice());

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
        let mut seed = [0u8; 64];
        seed.copy_from_slice(&record.enc_seed[..64]);
        let tag = Tag::from_slice(&record.enc_seed[64..]);
        cipher
            .decrypt_in_place_detached(nonce, b"", &mut seed, tag)
            .map_err(|_| {
                seed.zeroize();
                NvsError::Crypto
            })?;
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
        self.write_nvs_bytes(NVS_BASE_ADDR, &buf)
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
        self.write_nvs_bytes(offset, &buf)
    }

    fn write_seed_label_records(&mut self, labels: &[SeedSlotLabel]) -> Result<(), NvsError> {
        let buf = Self::seed_label_records_bytes(labels)?;
        self.write_nvs_bytes(LABELS_ADDR, &buf)
    }

    fn seed_label_records_bytes(labels: &[SeedSlotLabel]) -> Result<[u8; LABELS_SIZE], NvsError> {
        let mut buf = [0xFFu8; LABELS_SIZE];
        buf[..LABELS_MAGIC.len()].copy_from_slice(&LABELS_MAGIC);
        buf[4] = LABELS_VERSION;

        for entry in labels {
            let slot = entry.slot as usize;
            if slot >= MAX_SEED_SLOTS {
                continue;
            }
            let label = entry.label.as_str();
            if label.is_empty() {
                continue;
            }
            if !seed_label_valid(label) {
                return Err(NvsError::InvalidLabel);
            }
            let start = LABELS_HEADER_SIZE + slot * MAX_SEED_LABEL_LEN;
            let label_bytes = label.as_bytes();
            buf[start..start + label_bytes.len()].copy_from_slice(label_bytes);
            if label_bytes.len() < MAX_SEED_LABEL_LEN {
                buf[start + label_bytes.len()] = 0;
            }
        }

        Ok(buf)
    }

    fn shifted_seed_label_records_bytes(
        labels: &[SeedSlotLabel],
        deleted_slot: usize,
        old_last_slot: usize,
    ) -> Result<[u8; LABELS_SIZE], NvsError> {
        let mut shifted = Vec::new();
        for entry in labels {
            let slot = entry.slot as usize;
            if slot == deleted_slot {
                continue;
            }
            if slot > old_last_slot {
                continue;
            }
            let new_slot = if slot > deleted_slot { slot - 1 } else { slot };
            if new_slot >= MAX_SEED_SLOTS {
                continue;
            }
            let mut label = heapless::String::<MAX_SEED_LABEL_LEN>::new();
            if label.push_str(entry.label.as_str()).is_ok() {
                shifted.push(SeedSlotLabel {
                    slot: new_slot as u8,
                    label,
                });
            }
        }
        Self::seed_label_records_bytes(&shifted)
    }

    fn delete_seed_records_transaction(
        &mut self,
        header: &Header,
        deleted_slot: usize,
        old_last_slot: usize,
        labels_before_delete: &[SeedSlotLabel],
    ) -> Result<(), NvsError> {
        if deleted_slot > old_last_slot || old_last_slot >= MAX_SEED_SLOTS {
            return Err(NvsError::InvalidSlot);
        }

        let mut sector = self.read_nvs_sector()?;
        for index in deleted_slot..old_last_slot {
            let dst = slot_sector_offset(index)?;
            let src = slot_sector_offset(index + 1)?;
            sector.copy_within(src..src + SLOT_SIZE, dst);
        }

        let last = slot_sector_offset(old_last_slot)?;
        sector[last..last + SLOT_SIZE].fill(0xFF);

        let header_bytes = header.to_bytes();
        sector[..HEADER_SIZE].copy_from_slice(&header_bytes);

        let labels = Self::shifted_seed_label_records_bytes(
            labels_before_delete,
            deleted_slot,
            old_last_slot,
        )?;
        let labels_start = relative_nvs_offset(LABELS_ADDR, LABELS_SIZE)?;
        sector[labels_start..labels_start + LABELS_SIZE].copy_from_slice(&labels);

        self.write_nvs_sector(&sector)
    }

    fn increment_attempts(&mut self, header: &mut Header) -> Result<(), NvsError> {
        if header.attempts < MAX_PIN_ATTEMPTS {
            header.attempts = header.attempts.saturating_add(1);
            self.write_header(header)?;
        }
        Ok(())
    }

    fn clear_attempts_if_needed(&mut self, header: &mut Header) -> Result<(), NvsError> {
        if header.attempts == 0 {
            return Ok(());
        }
        header.attempts = 0;
        self.write_header(header)
    }

    fn write_nvs_bytes(&mut self, address: u32, bytes: &[u8]) -> Result<(), NvsError> {
        let start = relative_nvs_offset(address, bytes.len())?;
        let mut sector = self.read_nvs_sector()?;
        let end = start + bytes.len();
        sector[start..end].copy_from_slice(bytes);
        self.write_nvs_sector(&sector)
    }

    fn read_nvs_sector(&mut self) -> Result<Vec<u8>, NvsError> {
        if NVS_TOTAL_SIZE > NVS_SECTOR_SIZE {
            return Err(NvsError::Flash);
        }
        let mut sector = vec![0xFFu8; NVS_SECTOR_SIZE];
        self.flash
            .read(NVS_BASE_ADDR, &mut sector)
            .map_err(|_| NvsError::Flash)?;
        Ok(sector)
    }

    fn write_nvs_sector(&mut self, sector: &[u8]) -> Result<(), NvsError> {
        if sector.len() != NVS_SECTOR_SIZE {
            return Err(NvsError::Flash);
        }
        embedded_storage::nor_flash::NorFlash::erase(
            &mut self.flash,
            NVS_BASE_ADDR,
            NVS_SECTOR_END,
        )
        .map_err(|_| NvsError::Flash)?;
        embedded_storage::nor_flash::NorFlash::write(&mut self.flash, NVS_BASE_ADDR, &sector)
            .map_err(|_| NvsError::Flash)
    }
}
