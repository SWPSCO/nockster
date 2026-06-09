use aes_gcm::aead::{AeadInPlace, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce, Tag};
use alloc::vec;
use alloc::vec::Vec;
use core::result::Result;
use core::sync::atomic::{AtomicBool, Ordering};
use embedded_storage::ReadStorage;
use esp_hal::delay::Delay;
use esp_storage::FlashStorage;
use hmac::Hmac;
use nockster_core::alloc_path as pathmod;
use nockster_core::cheetah;
use nockster_core::{
    CheetahPub, DeviceAddressBookEntry, SeedSlotLabel, TouchCalibration,
    MAX_ADDRESS_BOOK_LABEL_LEN, MAX_ADDRESS_BOOK_PKH_LEN, MAX_DEVICE_ADDRESS_BOOK_ENTRIES,
    MAX_SEED_LABEL_LEN, MAX_SEED_SLOTS,
};
use pbkdf2::pbkdf2;
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

const PBKDF2_ROUNDS: u32 = 100_000;
const MAX_PIN_ATTEMPTS: u8 = 10;

const NVS_BASE_ADDR: u32 = 0x9000;
const NVS_SECTOR_SIZE: usize = FlashStorage::SECTOR_SIZE as usize;
const NVS_WRITE_CHUNK_SIZE: usize = 256;
const HEADER_SIZE: usize = 64;
const SLOT_SIZE: usize = 192;
const SEED_TOTAL_SIZE: usize = HEADER_SIZE + SLOT_SIZE * MAX_SEED_SLOTS;
const CALIBRATION_SIZE: usize = 64;
const LABELS_HEADER_SIZE: usize = 8;
const LABELS_SIZE: usize = LABELS_HEADER_SIZE + MAX_SEED_LABEL_LEN * MAX_SEED_SLOTS;
const GUI_PREFS_SIZE: usize = 64;
const ADDRESS_BOOK_HEADER_SIZE: usize = 8;
const ADDRESS_BOOK_RECORD_SIZE: usize = MAX_ADDRESS_BOOK_LABEL_LEN + MAX_ADDRESS_BOOK_PKH_LEN;
const ADDRESS_BOOK_SIZE: usize =
    ADDRESS_BOOK_HEADER_SIZE + ADDRESS_BOOK_RECORD_SIZE * MAX_DEVICE_ADDRESS_BOOK_ENTRIES;
const SEED_CORE_TOTAL_SIZE: usize =
    SEED_TOTAL_SIZE + CALIBRATION_SIZE + LABELS_SIZE + GUI_PREFS_SIZE;
const SEED_CORE_STORAGE_SIZE: usize = align_up(SEED_CORE_TOTAL_SIZE, NVS_SECTOR_SIZE);
const SEED_CORE_STORAGE_END: u32 = NVS_BASE_ADDR + SEED_CORE_STORAGE_SIZE as u32;
const ADDRESS_BOOK_STORAGE_SIZE: usize = align_up(ADDRESS_BOOK_SIZE, NVS_SECTOR_SIZE);
const CALIBRATION_ADDR: u32 = NVS_BASE_ADDR + SEED_TOTAL_SIZE as u32;
const LABELS_ADDR: u32 = CALIBRATION_ADDR + CALIBRATION_SIZE as u32;
const GUI_PREFS_ADDR: u32 = LABELS_ADDR + LABELS_SIZE as u32;
const ADDRESS_BOOK_ADDR: u32 = SEED_CORE_STORAGE_END;
const ADDRESS_BOOK_STORAGE_END: u32 = ADDRESS_BOOK_ADDR + ADDRESS_BOOK_STORAGE_SIZE as u32;
// PIN attempt marks live in their own sector so recording an attempt never
// erases or rewrites the seed region: one erased->programmed 32-byte block
// per attempt (atomic under power loss, and writable under flash encryption),
// erased as a whole on successful unlock. The legacy header.attempts field is
// still honored read-only via max() but no longer written on failures.
const ATTEMPT_MARKS_ADDR: u32 = ADDRESS_BOOK_STORAGE_END;
const ATTEMPT_MARKS_STORAGE_SIZE: usize = NVS_SECTOR_SIZE;
const ATTEMPT_MARKS_END: u32 = ATTEMPT_MARKS_ADDR + ATTEMPT_MARKS_STORAGE_SIZE as u32;
const ATTEMPT_MARK_SIZE: usize = 32;
const NVS_STORAGE_END: u32 = ATTEMPT_MARKS_END;
// partitions.csv: nvs, 0x9000, 28K
const _: () = assert!(NVS_STORAGE_END <= NVS_BASE_ADDR + 28 * 1024);
const FLASH_PAUSE_TIMEOUT_MS: u16 = 2_000;

const MAGIC: [u8; 4] = *b"NCK1";
const LEGACY_MAGIC: [u8; 4] = [b'S', b'G', b'R', b'1'];
const CALIBRATION_MAGIC: [u8; 4] = *b"NCTC";
const LEGACY_CALIBRATION_MAGIC: [u8; 4] = [b'S', b'G', b'T', b'C'];
const LABELS_MAGIC: [u8; 4] = *b"NCLB";
const LEGACY_LABELS_MAGIC: [u8; 4] = [b'S', b'G', b'L', b'B'];
const GUI_PREFS_MAGIC: [u8; 4] = *b"NCUI";
const ADDRESS_BOOK_MAGIC: [u8; 4] = *b"NCAB";
const VERSION_V1: u8 = 1;
const VERSION_V2: u8 = 2;
const VERSION: u8 = VERSION_V2;
pub const NVS_V2_PEPPER_DOMAIN: &[u8] = b"nockster-nvs-v2";
pub const NVS_V2_MASTER_DOMAIN: &[u8] = b"nockster-nvs-master-v2";
pub const NVS_V2_PEPPER_MESSAGE_LEN: usize = 15 + 32 + 6;
const NVS_V2_SOFTWARE_PEPPER: [u8; 32] = *b"nockster-dev-nvs-v2-pepper-00000";
const CALIBRATION_VERSION: u8 = 1;
const LABELS_VERSION: u8 = 1;
const GUI_PREFS_VERSION: u8 = 1;
const ADDRESS_BOOK_VERSION: u8 = 1;
const FLAG_INITIALIZED: u8 = 0x01;
const SLOT_USED: u8 = 0x01;
const CALIBRATION_MIRROR_X: u8 = 0x01;
const CALIBRATION_MIRROR_Y: u8 = 0x02;

const fn align_up(value: usize, alignment: usize) -> usize {
    let rem = value % alignment;
    if rem == 0 {
        value
    } else {
        value + alignment - rem
    }
}

static FLASH_PAUSE_REQUESTED: AtomicBool = AtomicBool::new(false);
static FLASH_PAUSE_WORKER_PARKED: AtomicBool = AtomicBool::new(false);
static FLASH_PAUSE_WORKER_ONLINE: AtomicBool = AtomicBool::new(false);

#[esp_hal::ram]
pub fn worker_flash_pause_requested() -> bool {
    FLASH_PAUSE_REQUESTED.load(Ordering::SeqCst)
}

#[esp_hal::ram]
pub fn worker_flash_pause_set_parked(parked: bool) {
    FLASH_PAUSE_WORKER_PARKED.store(parked, Ordering::SeqCst);
}

pub fn worker_flash_pause_set_online(online: bool) {
    FLASH_PAUSE_WORKER_ONLINE.store(online, Ordering::SeqCst);
    if !online {
        FLASH_PAUSE_WORKER_PARKED.store(false, Ordering::SeqCst);
    }
}

struct FlashPauseGuard;

impl FlashPauseGuard {
    fn acquire() -> Result<Self, NvsError> {
        FLASH_PAUSE_WORKER_PARKED.store(false, Ordering::SeqCst);
        FLASH_PAUSE_REQUESTED.store(true, Ordering::SeqCst);

        let delay = Delay::new();
        for _ in 0..FLASH_PAUSE_TIMEOUT_MS {
            if !FLASH_PAUSE_WORKER_ONLINE.load(Ordering::SeqCst)
                || FLASH_PAUSE_WORKER_PARKED.load(Ordering::SeqCst)
            {
                return Ok(Self);
            }
            delay.delay_millis(1u32);
        }

        FLASH_PAUSE_REQUESTED.store(false, Ordering::SeqCst);
        FLASH_PAUSE_WORKER_PARKED.store(false, Ordering::SeqCst);
        Err(NvsError::Flash)
    }
}

impl Drop for FlashPauseGuard {
    fn drop(&mut self) {
        FLASH_PAUSE_REQUESTED.store(false, Ordering::SeqCst);
        FLASH_PAUSE_WORKER_PARKED.store(false, Ordering::SeqCst);
    }
}

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

pub struct PreparedSeedInit {
    header: Header,
    slot_record: SlotRecord,
}

pub struct PreparedSeedRewrite {
    header: Header,
    records: Vec<SlotRecord>,
    labels: Vec<SeedSlotLabel>,
    calibration: Option<TouchCalibration>,
    gui_theme: Option<u8>,
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
    WriteFlash,
    Complete,
}

pub struct SoftwareNvsPepper;

impl NvsPepperSource for SoftwareNvsPepper {
    fn nvs_v2_pepper(&mut self, _salt: &[u8; 32]) -> Result<Option<[u8; 32]>, NvsError> {
        Ok(Some(NVS_V2_SOFTWARE_PEPPER))
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
        if nonce.iter().all(|byte| *byte == 0xFF)
            || enc_seed.iter().all(|byte| *byte == 0xFF)
            || pub_x == [u64::MAX; 6]
            || pub_y == [u64::MAX; 6]
            || (pub_x == [0; 6] && pub_y == [0; 6])
        {
            return None;
        }
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

fn address_book_label_valid(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= MAX_ADDRESS_BOOK_LABEL_LEN
        && label
            .bytes()
            .all(|byte| byte == b' ' || (0x21..=0x7e).contains(&byte))
}

fn address_book_pkh_valid(pkh: &str) -> bool {
    !pkh.is_empty()
        && pkh.len() <= MAX_ADDRESS_BOOK_PKH_LEN
        && pkh.bytes().all(|byte| {
            matches!(
                byte,
                b'1'..=b'9' | b'A'..=b'H' | b'J'..=b'N' | b'P'..=b'Z' | b'a'..=b'k' | b'm'..=b'z'
            )
        })
}

fn address_book_entry_valid(entry: &DeviceAddressBookEntry) -> bool {
    address_book_label_valid(entry.label.as_str()) && address_book_pkh_valid(entry.pkh.as_str())
}

fn read_fixed_ascii_string<const N: usize>(raw: &[u8]) -> Option<heapless::String<N>> {
    let len = raw
        .iter()
        .position(|byte| *byte == 0 || *byte == 0xFF)
        .unwrap_or(N);
    if len == 0 {
        return None;
    }
    let s = core::str::from_utf8(&raw[..len]).ok()?;
    let mut out = heapless::String::<N>::new();
    out.push_str(s).ok()?;
    Some(out)
}

fn write_fixed_ascii_string(dst: &mut [u8], value: &str) {
    let bytes = value.as_bytes();
    dst[..bytes.len()].copy_from_slice(bytes);
    if bytes.len() < dst.len() {
        dst[bytes.len()] = 0;
    }
}

fn matches_magic(buf: &[u8], current: &[u8; 4], legacy: &[u8; 4]) -> bool {
    let stored = &buf[..current.len()];
    stored == current.as_ref() || stored == legacy.as_ref()
}

fn relative_seed_offset(address: u32, len: usize) -> Result<usize, NvsError> {
    if address < NVS_BASE_ADDR {
        return Err(NvsError::Flash);
    }
    let start = (address - NVS_BASE_ADDR) as usize;
    let Some(end) = start.checked_add(len) else {
        return Err(NvsError::Flash);
    };
    if end > SEED_CORE_TOTAL_SIZE {
        return Err(NvsError::Flash);
    }
    Ok(start)
}

fn slot_sector_offset(index: usize) -> Result<usize, NvsError> {
    if index >= MAX_SEED_SLOTS {
        return Err(NvsError::InvalidSlot);
    }
    relative_seed_offset(
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

fn zeroize_seed_vec(seeds: &mut Vec<[u8; 64]>) {
    for seed in seeds.iter_mut() {
        seed.zeroize();
    }
}

impl NvsStore {
    pub fn new() -> Self {
        Self {
            flash: FlashStorage::new(),
        }
    }

    pub fn is_initialized(&mut self) -> bool {
        matches!(
            self.read_header(),
            Ok(Some(header)) if header.initialized() && self.seed_slots_present(&header).unwrap_or(false)
        )
    }

    pub fn get_attempts_remaining(&mut self) -> u8 {
        match self.read_header() {
            Ok(Some(header)) if header.initialized() => {
                let used = self.attempts_used(&header).unwrap_or(MAX_PIN_ATTEMPTS);
                MAX_PIN_ATTEMPTS.saturating_sub(used)
            }
            _ => MAX_PIN_ATTEMPTS,
        }
    }

    pub fn storage_status(&mut self) -> NvsStorageStatus {
        match self.read_header() {
            Ok(Some(header)) => {
                let initialized =
                    header.initialized() && self.seed_slots_present(&header).unwrap_or(false);
                NvsStorageStatus {
                    initialized,
                    schema_version: header.version,
                    slot_count: if initialized { header.slot_count } else { 0 },
                }
            }
            _ => NvsStorageStatus {
                initialized: false,
                schema_version: 0,
                slot_count: 0,
            },
        }
    }

    fn seed_slots_present(&mut self, header: &Header) -> Result<bool, NvsError> {
        if !header.initialized() || header.slot_count == 0 {
            return Ok(false);
        }

        for index in 0..header.slot_count as usize {
            if self.read_slot(index)?.is_none() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn initialize_pin(
        &mut self,
        pin: &str,
        seed64: &[u8; 64],
        pub_xy: ([u64; 6], [u64; 6]),
    ) -> Result<([u8; 32], u8), NvsError> {
        let mut pepper = SoftwareNvsPepper;
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
        let (prepared, mut key, slot) = self.prepare_initialize_pin_with_pepper_and_progress(
            pin,
            seed64,
            pub_xy,
            pepper_source,
            &mut progress,
        )?;
        progress(NvsInitStage::WriteFlash);
        if let Err(err) = self.commit_prepared_initialize_pin(prepared) {
            key.zeroize();
            return Err(err);
        }
        progress(NvsInitStage::Complete);
        Ok((key, slot))
    }

    pub fn prepare_initialize_pin_with_pepper_and_progress<P, F>(
        &mut self,
        pin: &str,
        seed64: &[u8; 64],
        pub_xy: ([u64; 6], [u64; 6]),
        pepper_source: &mut P,
        mut progress: F,
    ) -> Result<(PreparedSeedInit, [u8; 32], u8), NvsError>
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
        }

        let mut header = Header::new_uninitialized(VERSION);
        header.slot_count = 1;
        progress(NvsInitStage::RandomSalt);
        getrandom::getrandom(&mut header.salt).map_err(|_| NvsError::Crypto)?;

        progress(NvsInitStage::Pepper);
        let mut maybe_pepper = Self::pepper_for_new_header(&mut header, pepper_source)?;
        progress(NvsInitStage::Kdf);
        let mut key = match Self::derive_master_key_for_header(pin, &header, maybe_pepper.as_ref())
        {
            Ok(key) => key,
            Err(err) => {
                zeroize_optional_secret(&mut maybe_pepper);
                return Err(err);
            }
        };
        progress(NvsInitStage::KdfDone);
        zeroize_optional_secret(&mut maybe_pepper);
        progress(NvsInitStage::EncryptSeed);
        let slot_record = match self.encrypt_seed_record(&key, seed64, pub_xy) {
            Ok(record) => record,
            Err(err) => {
                key.zeroize();
                return Err(err);
            }
        };

        progress(NvsInitStage::WriteHeaderPending);
        progress(NvsInitStage::WriteSlot);
        header.set_initialized();
        progress(NvsInitStage::WriteHeaderFinal);
        progress(NvsInitStage::WriteLabels);
        Ok((
            PreparedSeedInit {
                header,
                slot_record,
            },
            key,
            0,
        ))
    }

    pub fn commit_prepared_initialize_pin(
        &mut self,
        prepared: PreparedSeedInit,
    ) -> Result<(), NvsError> {
        // Do not immediately read back here. On this target the flash write
        // can be durable while a same-cycle readback still reports failure.
        // Header-last commit ordering plus unlock-time seed/pub validation
        // protect against interrupted or corrupt writes.
        self.initialize_seed_storage_transaction(&prepared.header, &prepared.slot_record)
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
        match self.verify_master_key(master_key, &header) {
            Ok(()) => {}
            Err(NvsError::WrongPin) => {
                self.increment_attempts(&mut header)?;
                return Err(NvsError::WrongPin);
            }
            Err(err) => return Err(err),
        }

        let slot_record = self.encrypt_seed_record(master_key, seed64, pub_xy)?;

        header.slot_count += 1;
        header.attempts = 0;
        self.add_seed_record_transaction(&header, slot_index, &slot_record)?;
        Ok(slot_index as u8)
    }

    pub fn unlock(&mut self, pin: &str) -> Result<(Vec<[u8; 64]>, [u8; 32]), NvsError> {
        let mut pepper = SoftwareNvsPepper;
        self.unlock_with_pepper(pin, &mut pepper)
    }

    pub fn unlock_with_pepper<P: NvsPepperSource>(
        &mut self,
        pin: &str,
        pepper_source: &mut P,
    ) -> Result<(Vec<[u8; 64]>, [u8; 32]), NvsError> {
        let (mut seeds, mut key, clear_attempts) =
            self.unlock_with_pepper_readonly(pin, pepper_source)?;
        let mut header = self.read_header()?.ok_or(NvsError::NotInitialized)?;
        if clear_attempts {
            if let Err(err) = self.clear_attempts_if_needed(&mut header) {
                zeroize_seed_vec(&mut seeds);
                key.zeroize();
                return Err(err);
            }
        }
        Ok((seeds, key))
    }

    pub fn unlock_with_pepper_readonly<P: NvsPepperSource>(
        &mut self,
        pin: &str,
        pepper_source: &mut P,
    ) -> Result<(Vec<[u8; 64]>, [u8; 32], bool), NvsError> {
        self.unlock_with_pepper_inner(pin, pepper_source, true)
    }

    /// Unlock variant for callers that already charged this attempt via
    /// [`Self::begin_pin_attempt`]: the lockout gate is skipped here because
    /// the final allowed attempt legitimately runs with the persisted counter
    /// at MAX. Never call this without the pre-charge.
    pub fn unlock_with_pepper_precharged<P: NvsPepperSource>(
        &mut self,
        pin: &str,
        pepper_source: &mut P,
    ) -> Result<(Vec<[u8; 64]>, [u8; 32], bool), NvsError> {
        self.unlock_with_pepper_inner(pin, pepper_source, false)
    }

    fn unlock_with_pepper_inner<P: NvsPepperSource>(
        &mut self,
        pin: &str,
        pepper_source: &mut P,
        enforce_lockout: bool,
    ) -> Result<(Vec<[u8; 64]>, [u8; 32], bool), NvsError> {
        let header = self.read_header()?.ok_or(NvsError::NotInitialized)?;
        if !header.initialized() {
            return Err(NvsError::NotInitialized);
        }
        let attempts_used = self.attempts_used(&header)?;
        if enforce_lockout && attempts_used >= MAX_PIN_ATTEMPTS {
            return Err(NvsError::LockedOut);
        }

        let mut key = Self::derive_master_key_with_source(pin, &header, pepper_source)?;
        let mut seeds = Vec::new();
        if seeds.try_reserve_exact(header.slot_count as usize).is_err() {
            key.zeroize();
            return Err(NvsError::Crypto);
        }

        for index in 0..header.slot_count as usize {
            let record = match self.read_slot(index)? {
                Some(r) => r,
                None => {
                    zeroize_seed_vec(&mut seeds);
                    key.zeroize();
                    return Err(NvsError::Flash);
                }
            };
            match self.decrypt_seed(&key, &record) {
                Ok(mut seed) => {
                    if !Self::record_pub_matches_seed(&record, &seed) {
                        seed.zeroize();
                        zeroize_seed_vec(&mut seeds);
                        key.zeroize();
                        return Err(NvsError::Flash);
                    }
                    seeds.push(seed);
                }
                Err(_) => {
                    zeroize_seed_vec(&mut seeds);
                    key.zeroize();
                    return Err(NvsError::WrongPin);
                }
            }
        }

        Ok((seeds, key, attempts_used != 0))
    }

    /// Persist a provisional failed attempt BEFORE the PIN is checked, so
    /// cutting power right after a failed check cannot rewind the counter.
    /// A successful unlock clears it. Returns LockedOut when no attempts
    /// remain; on Ok the caller may run exactly one PIN check.
    pub fn begin_pin_attempt(&mut self) -> Result<(), NvsError> {
        let header = self.read_header()?.ok_or(NvsError::NotInitialized)?;
        if !header.initialized() {
            return Err(NvsError::NotInitialized);
        }
        let used = self.attempts_used(&header)?;
        if used >= MAX_PIN_ATTEMPTS {
            return Err(NvsError::LockedOut);
        }
        self.write_attempt_marks_up_to(used + 1)
    }

    pub fn record_wrong_pin_attempt(&mut self) -> Result<u8, NvsError> {
        let header = self.read_header()?.ok_or(NvsError::NotInitialized)?;
        if !header.initialized() {
            return Err(NvsError::NotInitialized);
        }
        let used = self.attempts_used(&header)?;
        if used >= MAX_PIN_ATTEMPTS {
            return Ok(0);
        }
        self.write_attempt_marks_up_to(used + 1)?;
        Ok(MAX_PIN_ATTEMPTS - (used + 1))
    }

    pub fn clear_pin_attempts(&mut self) -> Result<(), NvsError> {
        self.erase_attempt_marks()?;
        // Legacy devices may still carry a nonzero header counter; clearing it
        // rewrites the seed region, so it only happens when actually set.
        let mut header = self.read_header()?.ok_or(NvsError::NotInitialized)?;
        self.clear_attempts_if_needed(&mut header)
    }

    pub fn derive_master_key_for_pin(&mut self, pin: &str) -> Result<[u8; 32], NvsError> {
        let mut pepper = SoftwareNvsPepper;
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

        let mut key = Self::derive_master_key_with_source(pin, &header, pepper_source)?;
        match self.verify_master_key(&key, &header) {
            Ok(()) => {}
            Err(NvsError::WrongPin) => {
                if let Err(err) = self.increment_attempts(&mut header) {
                    key.zeroize();
                    return Err(err);
                }
                key.zeroize();
                return Err(NvsError::WrongPin);
            }
            Err(err) => {
                key.zeroize();
                return Err(err);
            }
        }

        if let Err(err) = self.clear_attempts_if_needed(&mut header) {
            key.zeroize();
            return Err(err);
        }
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

        match self.verify_master_key(master_key, &header) {
            Ok(()) => {}
            Err(NvsError::WrongPin) => {
                self.increment_attempts(&mut header)?;
                return Err(NvsError::WrongPin);
            }
            Err(err) => return Err(err),
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
        let mut pepper = SoftwareNvsPepper;
        self.change_pin_with_pepper(old_pin, new_pin, &mut pepper)
    }

    pub fn change_pin_with_pepper<P: NvsPepperSource>(
        &mut self,
        old_pin: &str,
        new_pin: &str,
        pepper_source: &mut P,
    ) -> Result<(), NvsError> {
        let (prepared, mut seeds, mut new_key) =
            match self.prepare_change_pin_with_pepper(old_pin, new_pin, pepper_source) {
                Ok(prepared) => prepared,
                Err(NvsError::WrongPin) => {
                    self.record_wrong_pin_attempt()?;
                    return Err(NvsError::WrongPin);
                }
                Err(err) => return Err(err),
            };
        if let Err(err) = self.commit_prepared_seed_rewrite(prepared) {
            new_key.zeroize();
            zeroize_seed_vec(&mut seeds);
            return Err(err);
        }
        new_key.zeroize();
        zeroize_seed_vec(&mut seeds);
        // The PIN changed successfully; a failed marks clear must not turn
        // that into an error (the marks also clear on the next unlock).
        let _ = self.erase_attempt_marks();
        Ok(())
    }

    pub fn prepare_change_pin_with_pepper<P: NvsPepperSource>(
        &mut self,
        old_pin: &str,
        new_pin: &str,
        pepper_source: &mut P,
    ) -> Result<(PreparedSeedRewrite, Vec<[u8; 64]>, [u8; 32]), NvsError> {
        self.prepare_change_pin_inner(old_pin, new_pin, pepper_source, true)
    }

    /// See [`Self::unlock_with_pepper_precharged`]: requires a prior
    /// [`Self::begin_pin_attempt`] for the old PIN.
    pub fn prepare_change_pin_with_pepper_precharged<P: NvsPepperSource>(
        &mut self,
        old_pin: &str,
        new_pin: &str,
        pepper_source: &mut P,
    ) -> Result<(PreparedSeedRewrite, Vec<[u8; 64]>, [u8; 32]), NvsError> {
        self.prepare_change_pin_inner(old_pin, new_pin, pepper_source, false)
    }

    fn prepare_change_pin_inner<P: NvsPepperSource>(
        &mut self,
        old_pin: &str,
        new_pin: &str,
        pepper_source: &mut P,
        enforce_lockout: bool,
    ) -> Result<(PreparedSeedRewrite, Vec<[u8; 64]>, [u8; 32]), NvsError> {
        let (mut seeds, mut old_key, _clear_attempts) =
            self.unlock_with_pepper_inner(old_pin, pepper_source, enforce_lockout)?;
        old_key.zeroize();
        let labels = self.read_seed_labels().unwrap_or_default();
        let calibration = self.read_touch_calibration().ok().flatten();
        let gui_theme = self.read_gui_theme().ok().flatten();

        if seeds.is_empty() {
            return Err(NvsError::NotInitialized);
        }

        let (prepared, new_key) = match self.prepare_rewrite_seed_storage(
            new_pin,
            seeds.as_slice(),
            labels.as_slice(),
            calibration,
            gui_theme,
            pepper_source,
        ) {
            Ok(key) => key,
            Err(err) => {
                zeroize_seed_vec(&mut seeds);
                return Err(err);
            }
        };
        Ok((prepared, seeds, new_key))
    }

    fn prepare_rewrite_seed_storage<P: NvsPepperSource>(
        &mut self,
        pin: &str,
        seeds: &[[u8; 64]],
        labels: &[SeedSlotLabel],
        calibration: Option<TouchCalibration>,
        gui_theme: Option<u8>,
        pepper_source: &mut P,
    ) -> Result<(PreparedSeedRewrite, [u8; 32]), NvsError> {
        if seeds.is_empty() {
            return Err(NvsError::NotInitialized);
        }
        if seeds.len() > MAX_SEED_SLOTS {
            return Err(NvsError::Crypto);
        }

        let mut header = Header::new_uninitialized(VERSION);
        header.slot_count = seeds.len() as u8;
        header.set_initialized();
        getrandom::getrandom(&mut header.salt).map_err(|_| NvsError::Crypto)?;

        let mut maybe_pepper = Self::pepper_for_new_header(&mut header, pepper_source)?;
        let mut key = match Self::derive_master_key_for_header(pin, &header, maybe_pepper.as_ref())
        {
            Ok(key) => key,
            Err(err) => {
                zeroize_optional_secret(&mut maybe_pepper);
                return Err(err);
            }
        };
        zeroize_optional_secret(&mut maybe_pepper);

        let mut records = Vec::new();
        if records.try_reserve_exact(seeds.len()).is_err() {
            key.zeroize();
            return Err(NvsError::Crypto);
        }
        for seed in seeds.iter() {
            let pub_xy = Self::seed_root_pub(seed);
            let record = match self.encrypt_seed_record(&key, seed, pub_xy) {
                Ok(record) => record,
                Err(err) => {
                    key.zeroize();
                    return Err(err);
                }
            };
            records.push(record);
        }

        let mut stored_labels = Vec::new();
        if stored_labels.try_reserve_exact(labels.len()).is_err() {
            key.zeroize();
            return Err(NvsError::Crypto);
        }
        for label in labels {
            stored_labels.push(label.clone());
        }

        header.attempts = 0;
        Ok((
            PreparedSeedRewrite {
                header,
                records,
                labels: stored_labels,
                calibration,
                gui_theme,
            },
            key,
        ))
    }

    pub fn commit_prepared_seed_rewrite(
        &mut self,
        prepared: PreparedSeedRewrite,
    ) -> Result<(), NvsError> {
        self.rewrite_seed_storage_transaction(
            &prepared.header,
            prepared.records.as_slice(),
            prepared.labels.as_slice(),
            prepared.calibration,
            prepared.gui_theme,
        )
    }

    pub fn wipe(&mut self) -> Result<(), NvsError> {
        let _pause = FlashPauseGuard::acquire()?;
        self.erase_flash_region(NVS_BASE_ADDR, NVS_STORAGE_END)?;
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

    pub fn read_device_address_book(&mut self) -> Result<Vec<DeviceAddressBookEntry>, NvsError> {
        let mut buf = [0u8; ADDRESS_BOOK_SIZE];
        self.flash
            .read(ADDRESS_BOOK_ADDR, &mut buf)
            .map_err(|_| NvsError::Flash)?;
        if buf.iter().all(|b| *b == 0xFF) {
            return Ok(Vec::new());
        }
        if &buf[..ADDRESS_BOOK_MAGIC.len()] != ADDRESS_BOOK_MAGIC.as_ref() {
            return Ok(Vec::new());
        }
        if buf[4] != ADDRESS_BOOK_VERSION {
            return Ok(Vec::new());
        }

        let count = core::cmp::min(buf[5] as usize, MAX_DEVICE_ADDRESS_BOOK_ENTRIES);
        let mut entries = Vec::new();
        for index in 0..count {
            let start = ADDRESS_BOOK_HEADER_SIZE + index * ADDRESS_BOOK_RECORD_SIZE;
            let label_raw = &buf[start..start + MAX_ADDRESS_BOOK_LABEL_LEN];
            let pkh_start = start + MAX_ADDRESS_BOOK_LABEL_LEN;
            let pkh_raw = &buf[pkh_start..pkh_start + MAX_ADDRESS_BOOK_PKH_LEN];

            let Some(label) = read_fixed_ascii_string::<MAX_ADDRESS_BOOK_LABEL_LEN>(label_raw)
            else {
                continue;
            };
            let Some(pkh) = read_fixed_ascii_string::<MAX_ADDRESS_BOOK_PKH_LEN>(pkh_raw) else {
                continue;
            };
            let entry = DeviceAddressBookEntry { label, pkh };
            if address_book_entry_valid(&entry) {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    pub fn write_device_address_book(
        &mut self,
        entries: &[DeviceAddressBookEntry],
    ) -> Result<(), NvsError> {
        let buf = Self::device_address_book_bytes(entries)?;
        self.write_address_book_region(&buf)
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
        let buf = Self::touch_calibration_bytes(calibration)?;
        self.write_seed_core_bytes(CALIBRATION_ADDR, &buf)
    }

    pub fn read_gui_theme(&mut self) -> Result<Option<u8>, NvsError> {
        let mut buf = [0u8; GUI_PREFS_SIZE];
        self.flash
            .read(GUI_PREFS_ADDR, &mut buf)
            .map_err(|_| NvsError::Flash)?;
        if buf.iter().all(|b| *b == 0xFF) {
            return Ok(None);
        }
        if &buf[..GUI_PREFS_MAGIC.len()] != GUI_PREFS_MAGIC.as_ref() {
            return Ok(None);
        }
        if buf[4] != GUI_PREFS_VERSION {
            return Ok(None);
        }
        if buf[5] == 0xFF {
            return Ok(None);
        }
        Ok(Some(buf[5]))
    }

    pub fn write_gui_theme(&mut self, theme_id: u8) -> Result<(), NvsError> {
        let buf = Self::gui_prefs_bytes(theme_id);
        self.write_seed_core_bytes(GUI_PREFS_ADDR, &buf)
    }

    fn touch_calibration_bytes(
        calibration: &TouchCalibration,
    ) -> Result<[u8; CALIBRATION_SIZE], NvsError> {
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
        Ok(buf)
    }

    fn gui_prefs_bytes(theme_id: u8) -> [u8; GUI_PREFS_SIZE] {
        let mut buf = [0xFFu8; GUI_PREFS_SIZE];
        buf[..GUI_PREFS_MAGIC.len()].copy_from_slice(&GUI_PREFS_MAGIC);
        buf[4] = GUI_PREFS_VERSION;
        buf[5] = theme_id;
        buf
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
            Some(Self::nvs_v2_pepper_or_software(
                &header.salt,
                pepper_source,
            )?)
        } else {
            None
        };
        let key = match Self::derive_master_key_for_header(pin, header, maybe_pepper.as_ref()) {
            Ok(key) => key,
            Err(err) => {
                zeroize_optional_secret(&mut maybe_pepper);
                return Err(err);
            }
        };
        zeroize_optional_secret(&mut maybe_pepper);
        Ok(key)
    }

    fn pepper_for_new_header<P: NvsPepperSource>(
        header: &mut Header,
        pepper_source: &mut P,
    ) -> Result<Option<[u8; 32]>, NvsError> {
        header.version = VERSION;
        if header.version == VERSION_V2 {
            return Ok(Some(Self::nvs_v2_pepper_or_software(
                &header.salt,
                pepper_source,
            )?));
        }
        Ok(None)
    }

    fn nvs_v2_pepper_or_software<P: NvsPepperSource>(
        salt: &[u8; 32],
        pepper_source: &mut P,
    ) -> Result<[u8; 32], NvsError> {
        Ok(pepper_source
            .nvs_v2_pepper(salt)?
            .unwrap_or(NVS_V2_SOFTWARE_PEPPER))
    }

    fn verify_master_key(&mut self, key: &[u8; 32], header: &Header) -> Result<(), NvsError> {
        if header.slot_count == 0 {
            return Err(NvsError::Flash);
        }
        if let Some(record) = self.read_slot(0)? {
            let mut seed = self
                .decrypt_seed(key, &record)
                .map_err(|_| NvsError::WrongPin)?;
            let matches = Self::record_pub_matches_seed(&record, &seed);
            seed.zeroize();
            if matches {
                Ok(())
            } else {
                Err(NvsError::Flash)
            }
        } else {
            Err(NvsError::Flash)
        }
    }

    fn seed_root_pub(seed: &[u8; 64]) -> ([u64; 6], [u64; 6]) {
        let (mut sk, mut cc) = cheetah::master_from_seed(seed);
        let pub_xy = cheetah::cheetah_pub_from_sk(sk);
        sk.zeroize();
        cc.zeroize();
        pub_xy
    }

    fn record_pub_matches_seed(record: &SlotRecord, seed: &[u8; 64]) -> bool {
        let pub_xy = Self::seed_root_pub(seed);
        record.pub_x == pub_xy.0 && record.pub_y == pub_xy.1
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
        self.write_seed_core_bytes(NVS_BASE_ADDR, &buf)
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

    fn write_seed_label_records(&mut self, labels: &[SeedSlotLabel]) -> Result<(), NvsError> {
        let buf = Self::seed_label_records_bytes(labels)?;
        self.write_seed_core_bytes(LABELS_ADDR, &buf)
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

    fn device_address_book_bytes(
        entries: &[DeviceAddressBookEntry],
    ) -> Result<[u8; ADDRESS_BOOK_SIZE], NvsError> {
        if entries.len() > MAX_DEVICE_ADDRESS_BOOK_ENTRIES {
            return Err(NvsError::Full);
        }

        let mut buf = [0xFFu8; ADDRESS_BOOK_SIZE];
        buf[..ADDRESS_BOOK_MAGIC.len()].copy_from_slice(&ADDRESS_BOOK_MAGIC);
        buf[4] = ADDRESS_BOOK_VERSION;
        buf[5] = entries.len() as u8;

        for (index, entry) in entries.iter().enumerate() {
            if !address_book_entry_valid(entry) {
                return Err(NvsError::InvalidLabel);
            }

            let start = ADDRESS_BOOK_HEADER_SIZE + index * ADDRESS_BOOK_RECORD_SIZE;
            write_fixed_ascii_string(
                &mut buf[start..start + MAX_ADDRESS_BOOK_LABEL_LEN],
                entry.label.as_str(),
            );
            let pkh_start = start + MAX_ADDRESS_BOOK_LABEL_LEN;
            write_fixed_ascii_string(
                &mut buf[pkh_start..pkh_start + MAX_ADDRESS_BOOK_PKH_LEN],
                entry.pkh.as_str(),
            );
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

    fn initialize_seed_storage_transaction(
        &mut self,
        header: &Header,
        record: &SlotRecord,
    ) -> Result<(), NvsError> {
        let mut sector = self.read_seed_core_region()?;
        sector[..SEED_TOTAL_SIZE].fill(0xFF);

        let header_bytes = header.to_bytes();
        sector[..HEADER_SIZE].copy_from_slice(&header_bytes);

        let slot = slot_sector_offset(0)?;
        let slot_bytes = record.to_bytes();
        sector[slot..slot + SLOT_SIZE].copy_from_slice(&slot_bytes);

        let labels = Self::seed_label_records_bytes(&[])?;
        let labels_start = relative_seed_offset(LABELS_ADDR, LABELS_SIZE)?;
        sector[labels_start..labels_start + LABELS_SIZE].copy_from_slice(&labels);

        self.write_seed_core_region(&sector)
    }

    fn rewrite_seed_storage_transaction(
        &mut self,
        header: &Header,
        records: &[SlotRecord],
        labels: &[SeedSlotLabel],
        calibration: Option<TouchCalibration>,
        gui_theme: Option<u8>,
    ) -> Result<(), NvsError> {
        if records.len() > MAX_SEED_SLOTS {
            return Err(NvsError::Crypto);
        }

        let mut sector = vec![0xFFu8; SEED_CORE_STORAGE_SIZE];

        let header_bytes = header.to_bytes();
        sector[..HEADER_SIZE].copy_from_slice(&header_bytes);

        for (index, record) in records.iter().enumerate() {
            let slot = slot_sector_offset(index)?;
            let slot_bytes = record.to_bytes();
            sector[slot..slot + SLOT_SIZE].copy_from_slice(&slot_bytes);
        }

        let labels = Self::seed_label_records_bytes(labels)?;
        let labels_start = relative_seed_offset(LABELS_ADDR, LABELS_SIZE)?;
        sector[labels_start..labels_start + LABELS_SIZE].copy_from_slice(&labels);

        if let Some(calibration) = calibration {
            let calibration = Self::touch_calibration_bytes(&calibration)?;
            let calibration_start = relative_seed_offset(CALIBRATION_ADDR, CALIBRATION_SIZE)?;
            sector[calibration_start..calibration_start + CALIBRATION_SIZE]
                .copy_from_slice(&calibration);
        }

        if let Some(theme_id) = gui_theme {
            let gui_prefs = Self::gui_prefs_bytes(theme_id);
            let prefs_start = relative_seed_offset(GUI_PREFS_ADDR, GUI_PREFS_SIZE)?;
            sector[prefs_start..prefs_start + GUI_PREFS_SIZE].copy_from_slice(&gui_prefs);
        }

        self.write_seed_core_region(&sector)
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

        let mut sector = self.read_seed_core_region()?;
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
        let labels_start = relative_seed_offset(LABELS_ADDR, LABELS_SIZE)?;
        sector[labels_start..labels_start + LABELS_SIZE].copy_from_slice(&labels);

        self.write_seed_core_region(&sector)
    }

    fn add_seed_record_transaction(
        &mut self,
        header: &Header,
        slot_index: usize,
        record: &SlotRecord,
    ) -> Result<(), NvsError> {
        if slot_index >= MAX_SEED_SLOTS {
            return Err(NvsError::Full);
        }

        let mut sector = self.read_seed_core_region()?;
        let slot = slot_sector_offset(slot_index)?;
        let slot_bytes = record.to_bytes();
        sector[slot..slot + SLOT_SIZE].copy_from_slice(&slot_bytes);

        let header_bytes = header.to_bytes();
        sector[..HEADER_SIZE].copy_from_slice(&header_bytes);

        self.write_seed_core_region(&sector)
    }

    fn read_attempt_marks(&mut self) -> Result<u8, NvsError> {
        let mut buf = [0u8; ATTEMPT_MARK_SIZE * MAX_PIN_ATTEMPTS as usize];
        self.flash
            .read(ATTEMPT_MARKS_ADDR, &mut buf)
            .map_err(|_| NvsError::Flash)?;
        let mut count = 0u8;
        for block in buf.chunks_exact(ATTEMPT_MARK_SIZE) {
            if !block.iter().all(|b| *b == 0xFF) {
                count += 1;
            }
        }
        Ok(count)
    }

    fn write_attempt_marks_up_to(&mut self, target: u8) -> Result<(), NvsError> {
        let target = target.min(MAX_PIN_ATTEMPTS);
        let mut buf = [0u8; ATTEMPT_MARK_SIZE * MAX_PIN_ATTEMPTS as usize];
        self.flash
            .read(ATTEMPT_MARKS_ADDR, &mut buf)
            .map_err(|_| NvsError::Flash)?;
        let mut used: u8 = 0;
        for block in buf.chunks_exact(ATTEMPT_MARK_SIZE) {
            if !block.iter().all(|b| *b == 0xFF) {
                used += 1;
            }
        }
        if used >= target {
            return Ok(());
        }
        let _pause = FlashPauseGuard::acquire()?;
        let zeros = [0u8; ATTEMPT_MARK_SIZE];
        for (index, block) in buf.chunks_exact(ATTEMPT_MARK_SIZE).enumerate() {
            if used >= target {
                break;
            }
            if block.iter().all(|b| *b == 0xFF) {
                let addr = ATTEMPT_MARKS_ADDR + (index * ATTEMPT_MARK_SIZE) as u32;
                embedded_storage::nor_flash::NorFlash::write(&mut self.flash, addr, &zeros)
                    .map_err(|_| NvsError::Flash)?;
                used += 1;
            }
        }
        Ok(())
    }

    fn erase_attempt_marks(&mut self) -> Result<(), NvsError> {
        let _pause = FlashPauseGuard::acquire()?;
        self.erase_flash_region(ATTEMPT_MARKS_ADDR, ATTEMPT_MARKS_END)
    }

    fn attempts_used(&mut self, header: &Header) -> Result<u8, NvsError> {
        Ok(header.attempts.max(self.read_attempt_marks()?))
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

    fn write_seed_core_bytes(&mut self, address: u32, bytes: &[u8]) -> Result<(), NvsError> {
        let start = relative_seed_offset(address, bytes.len())?;
        let mut sector = self.read_seed_core_region()?;
        let end = start + bytes.len();
        sector[start..end].copy_from_slice(bytes);
        self.write_seed_core_region(&sector)
    }

    fn read_seed_core_region(&mut self) -> Result<Vec<u8>, NvsError> {
        let mut sector = vec![0xFFu8; SEED_CORE_STORAGE_SIZE];
        self.flash
            .read(NVS_BASE_ADDR, &mut sector)
            .map_err(|_| NvsError::Flash)?;
        Ok(sector)
    }

    fn write_seed_core_region(&mut self, sector: &[u8]) -> Result<(), NvsError> {
        self.write_flash_region_defer_first_chunk(NVS_BASE_ADDR, SEED_CORE_STORAGE_END, sector)
    }

    fn write_address_book_region(
        &mut self,
        address_book: &[u8; ADDRESS_BOOK_SIZE],
    ) -> Result<(), NvsError> {
        let count = core::cmp::min(address_book[5] as usize, MAX_DEVICE_ADDRESS_BOOK_ENTRIES);
        let used_len = ADDRESS_BOOK_HEADER_SIZE + count * ADDRESS_BOOK_RECORD_SIZE;
        let storage_len = align_up(used_len.max(ADDRESS_BOOK_HEADER_SIZE), NVS_SECTOR_SIZE)
            .min(ADDRESS_BOOK_STORAGE_SIZE);
        let mut region = vec![0xFFu8; storage_len];
        region[..used_len].copy_from_slice(&address_book[..used_len]);
        self.write_flash_region(
            ADDRESS_BOOK_ADDR,
            ADDRESS_BOOK_ADDR + storage_len as u32,
            &region,
        )
    }

    fn erase_flash_region(&mut self, start: u32, end: u32) -> Result<(), NvsError> {
        let sector_size = NVS_SECTOR_SIZE as u32;
        if start >= end || start % sector_size != 0 || end % sector_size != 0 {
            return Err(NvsError::Flash);
        }

        let mut cursor = start;
        while cursor < end {
            let next = cursor + sector_size;
            embedded_storage::nor_flash::NorFlash::erase(&mut self.flash, cursor, next)
                .map_err(|_| NvsError::Flash)?;
            cursor = next;
        }
        Ok(())
    }

    fn write_flash_region(&mut self, start: u32, end: u32, region: &[u8]) -> Result<(), NvsError> {
        self.validate_flash_region(start, end, region)?;
        let _pause = FlashPauseGuard::acquire()?;
        self.erase_flash_region(start, end)?;
        self.write_flash_chunks(start, region, 0, region.len())
    }

    fn write_flash_region_defer_first_chunk(
        &mut self,
        start: u32,
        end: u32,
        region: &[u8],
    ) -> Result<(), NvsError> {
        self.validate_flash_region(start, end, region)?;
        if region.len() <= NVS_WRITE_CHUNK_SIZE {
            return self.write_flash_region(start, end, region);
        }

        let _pause = FlashPauseGuard::acquire()?;
        self.erase_flash_region(start, end)?;
        self.write_flash_chunks(start, region, NVS_WRITE_CHUNK_SIZE, region.len())?;
        self.write_flash_chunks(start, region, 0, NVS_WRITE_CHUNK_SIZE)
    }

    fn validate_flash_region(&self, start: u32, end: u32, region: &[u8]) -> Result<(), NvsError> {
        let Some(region_len) = end.checked_sub(start) else {
            return Err(NvsError::Flash);
        };
        if region.len() != region_len as usize {
            return Err(NvsError::Flash);
        }
        if NVS_WRITE_CHUNK_SIZE == 0 || NVS_WRITE_CHUNK_SIZE % FlashStorage::WORD_SIZE as usize != 0
        {
            return Err(NvsError::Flash);
        }
        Ok(())
    }

    fn write_flash_chunks(
        &mut self,
        start: u32,
        region: &[u8],
        mut offset: usize,
        limit: usize,
    ) -> Result<(), NvsError> {
        while offset < limit {
            let end = (offset + NVS_WRITE_CHUNK_SIZE).min(limit);
            let chunk = &region[offset..end];
            if chunk.iter().any(|byte| *byte != 0xFF) {
                embedded_storage::nor_flash::NorFlash::write(
                    &mut self.flash,
                    start + offset as u32,
                    chunk,
                )
                .map_err(|_| NvsError::Flash)?;
            }
            offset = end;
        }
        Ok(())
    }
}
