use core::cell::RefCell;
use critical_section::Mutex;
use embedded_storage::ReadStorage;
use esp_bootloader_esp_idf::ota::{Ota, OtaImageState, Slot};
use esp_bootloader_esp_idf::partitions::{
    read_partition_table, AppPartitionSubType, DataPartitionSubType, PartitionType,
    PARTITION_TABLE_MAX_LEN,
};
use esp_storage::FlashStorage;
use nockster_core::update::{
    should_mark_ota_image_valid, verify_update_bundle_signature, verify_update_manifest_policy,
    UpdateImageStreamError, UpdateImageVerifier, UpdateManifest, UpdateManifestPolicy,
    UpdateManifestPolicyError, UpdateSignatureError, MAX_UPDATE_CHUNK_LEN,
    UPDATE_BUILD_PROFILE_DEV, UPDATE_HARDWARE_TARGET_ESP32S3_TOUCH_LCD_1_47,
};
use nockster_core::{
    UpdateBootStatus, UpdateStatus, UPDATE_OTA_STATE_ABORTED, UPDATE_OTA_STATE_INVALID,
    UPDATE_OTA_STATE_NEW, UPDATE_OTA_STATE_PENDING_VERIFY, UPDATE_OTA_STATE_UNAVAILABLE,
    UPDATE_OTA_STATE_UNDEFINED, UPDATE_OTA_STATE_UNKNOWN, UPDATE_OTA_STATE_VALID, UPDATE_SLOT_NONE,
    UPDATE_SLOT_OTA0, UPDATE_SLOT_OTA1, UPDATE_SLOT_UNKNOWN,
};
use sha2::{Digest, Sha256};

const HARDWARE_TARGET: &str = UPDATE_HARDWARE_TARGET_ESP32S3_TOUCH_LCD_1_47;
const OTA_SECTOR_SIZE: usize = FlashStorage::SECTOR_SIZE as usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateAuthError {
    NoTrustAnchor,
    UnsupportedManifest,
    Crypto(UpdateSignatureError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateStreamError {
    Auth(UpdateAuthError),
    Busy,
    NoActiveSession,
    Flash(UpdateFlashError),
    Image(UpdateImageStreamError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateFlashError {
    PartitionTable,
    MissingOtaData,
    MissingOtaSlot,
    ImageTooLarge,
    OutOfBounds,
    VerifyMismatch,
    Storage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OtaTarget {
    slot: Slot,
    offset: u32,
    len: u32,
}

struct OtaWriter {
    flash: FlashStorage,
    target: OtaTarget,
    image_size: u32,
    image_sha256: [u8; 32],
    sector_base: Option<u32>,
    sector: [u8; OTA_SECTOR_SIZE],
    sector_dirty: bool,
}

struct UpdateSession {
    verifier: UpdateImageVerifier,
    writer: Option<OtaWriter>,
}

#[allow(clippy::declare_interior_mutable_const)]
static UPDATE_SESSION: Mutex<RefCell<Option<UpdateSession>>> = Mutex::new(RefCell::new(None));

pub fn trusted_pubkey_sha256() -> Option<[u8; 32]> {
    let hex = option_env!("NOCKSTER_UPDATE_PUBKEY_SHA256_HEX")?;
    parse_hex_32(hex)
}

pub fn verify_manifest(
    manifest: &UpdateManifest,
    signature64: &[u8; 64],
    signing_pubkey_sec1: &[u8],
) -> Result<(), UpdateAuthError> {
    verify_update_manifest_policy(manifest, &firmware_update_policy()).map_err(
        |err| match err {
            UpdateManifestPolicyError::UnsupportedManifest => UpdateAuthError::UnsupportedManifest,
            UpdateManifestPolicyError::RollbackVersion => {
                UpdateAuthError::Crypto(UpdateSignatureError::RollbackVersion)
            }
        },
    )?;

    let Some(trusted_hash) = trusted_pubkey_sha256() else {
        return Err(UpdateAuthError::NoTrustAnchor);
    };

    verify_update_bundle_signature(manifest, signature64, signing_pubkey_sec1, &trusted_hash)
        .map_err(UpdateAuthError::Crypto)
}

fn firmware_update_policy() -> UpdateManifestPolicy<'static> {
    UpdateManifestPolicy {
        current_release_version: firmware_release_version(),
        hardware_target: HARDWARE_TARGET,
        current_build_profile: option_env!("NOCKSTER_BUILD_PROFILE")
            .unwrap_or(UPDATE_BUILD_PROFILE_DEV),
        protocol_v: nockster_core::PROTO_V1,
    }
}

pub fn begin_stream(
    manifest: &UpdateManifest,
    signature64: &[u8; 64],
    signing_pubkey_sec1: &[u8],
    write_flash: bool,
) -> Result<UpdateStatus, UpdateStreamError> {
    verify_manifest(manifest, signature64, signing_pubkey_sec1).map_err(UpdateStreamError::Auth)?;

    if stream_active() {
        return Err(UpdateStreamError::Busy);
    }

    let writer = if write_flash {
        Some(OtaWriter::new(manifest).map_err(UpdateStreamError::Flash)?)
    } else {
        None
    };

    critical_section::with(|cs| {
        let mut slot = UPDATE_SESSION.borrow_ref_mut(cs);
        if slot.is_some() {
            return Err(UpdateStreamError::Busy);
        }

        slot.replace(UpdateSession {
            verifier: UpdateImageVerifier::new(manifest.clone()),
            writer,
        });
        slot.as_ref()
            .map(|session| session.verifier.status(false))
            .ok_or(UpdateStreamError::Busy)
    })
}

pub fn append_chunk(offset: u32, chunk: &[u8]) -> Result<UpdateStatus, UpdateStreamError> {
    let Some(mut session) = critical_section::with(|cs| UPDATE_SESSION.borrow_ref_mut(cs).take())
    else {
        return Err(UpdateStreamError::NoActiveSession);
    };

    let result = append_chunk_inner(&mut session, offset, chunk);
    if result.is_ok() {
        critical_section::with(|cs| {
            UPDATE_SESSION.borrow_ref_mut(cs).replace(session);
        });
    }
    result
}

pub fn finish_stream() -> Result<UpdateStatus, UpdateStreamError> {
    let session = critical_section::with(|cs| UPDATE_SESSION.borrow_ref_mut(cs).take());
    let Some(session) = session else {
        return Err(UpdateStreamError::NoActiveSession);
    };

    let status = session
        .verifier
        .finish()
        .map_err(UpdateStreamError::Image)?;
    if let Some(writer) = session.writer {
        writer.activate().map_err(UpdateStreamError::Flash)?;
    }
    Ok(status)
}

pub fn cancel_stream() {
    critical_section::with(|cs| {
        UPDATE_SESSION.borrow_ref_mut(cs).take();
    });
}

pub fn stream_active() -> bool {
    critical_section::with(|cs| UPDATE_SESSION.borrow_ref(cs).is_some())
}

pub fn stream_status() -> UpdateStatus {
    critical_section::with(|cs| {
        UPDATE_SESSION
            .borrow_ref(cs)
            .as_ref()
            .map(|session| session.verifier.status(false))
            .unwrap_or(UpdateStatus {
                active: false,
                manifest_verified: false,
                image_verified: false,
                release_version: 0,
                bytes_received: 0,
                image_size: 0,
            })
    })
}

pub fn read_update_boot_status() -> UpdateBootStatus {
    let mut status = UpdateBootStatus {
        partition_table_ok: false,
        ota_data_present: false,
        ota0_present: false,
        ota1_present: false,
        current_slot: UPDATE_SLOT_UNKNOWN,
        next_slot: UPDATE_SLOT_UNKNOWN,
        ota_state: UPDATE_OTA_STATE_UNKNOWN,
        ota0_offset: 0,
        ota0_size: 0,
        ota1_offset: 0,
        ota1_size: 0,
    };

    let mut flash = FlashStorage::new();
    let mut table_storage = [0u8; PARTITION_TABLE_MAX_LEN];
    let Ok(table) = read_partition_table(&mut flash, &mut table_storage) else {
        return status;
    };
    status.partition_table_ok = true;

    let Ok(otadata) = table.find_partition(PartitionType::Data(DataPartitionSubType::Ota)) else {
        status.partition_table_ok = false;
        return status;
    };
    status.ota_data_present = otadata.is_some();

    if let Ok(Some(ota0)) = table.find_partition(PartitionType::App(AppPartitionSubType::Ota0)) {
        status.ota0_present = true;
        status.ota0_offset = ota0.offset();
        status.ota0_size = ota0.len();
    }
    if let Ok(Some(ota1)) = table.find_partition(PartitionType::App(AppPartitionSubType::Ota1)) {
        status.ota1_present = true;
        status.ota1_offset = ota1.offset();
        status.ota1_size = ota1.len();
    }

    let Some(otadata) = otadata else {
        status.current_slot = UPDATE_SLOT_NONE;
        status.next_slot = UPDATE_SLOT_OTA0;
        status.ota_state = UPDATE_OTA_STATE_UNAVAILABLE;
        return status;
    };

    let mut otadata_region = otadata.as_embedded_storage(&mut flash);
    let Ok(mut ota) = Ota::new(&mut otadata_region) else {
        return status;
    };
    let Ok(current_slot) = ota.current_slot() else {
        return status;
    };

    status.current_slot = slot_code(current_slot);
    status.next_slot = slot_code(current_slot.next());
    status.ota_state = match current_slot {
        Slot::None => UPDATE_OTA_STATE_UNAVAILABLE,
        _ => ota
            .current_ota_state()
            .map(ota_state_code)
            .unwrap_or(UPDATE_OTA_STATE_UNKNOWN),
    };
    status
}

pub fn mark_running_image_valid() {
    let _ = mark_running_image_valid_inner();
}

fn mark_running_image_valid_inner() -> Result<(), UpdateFlashError> {
    let mut flash = FlashStorage::new();
    let mut table_storage = [0u8; PARTITION_TABLE_MAX_LEN];
    let table = read_partition_table(&mut flash, &mut table_storage)
        .map_err(|_| UpdateFlashError::PartitionTable)?;
    let Some(otadata) = table
        .find_partition(PartitionType::Data(DataPartitionSubType::Ota))
        .map_err(|_| UpdateFlashError::PartitionTable)?
    else {
        return Ok(());
    };

    let mut otadata_region = otadata.as_embedded_storage(&mut flash);
    let mut ota = Ota::new(&mut otadata_region).map_err(|_| UpdateFlashError::PartitionTable)?;
    if ota
        .current_slot()
        .map_err(|_| UpdateFlashError::PartitionTable)?
        == Slot::None
    {
        return Ok(());
    }

    let current_state = ota
        .current_ota_state()
        .map(ota_state_code)
        .map_err(|_| UpdateFlashError::PartitionTable)?;
    if should_mark_ota_image_valid(current_state) {
        ota.set_current_ota_state(OtaImageState::Valid)
            .map_err(|_| UpdateFlashError::Storage)?;
    }
    Ok(())
}

fn validate_chunk_bounds(
    verifier: &UpdateImageVerifier,
    offset: u32,
    chunk: &[u8],
) -> Result<(), UpdateStreamError> {
    let status = verifier.status(false);
    if chunk.len() > MAX_UPDATE_CHUNK_LEN {
        return Err(UpdateStreamError::Image(
            UpdateImageStreamError::ChunkTooLarge,
        ));
    }
    if status.bytes_received != offset {
        return Err(UpdateStreamError::Image(
            UpdateImageStreamError::OffsetMismatch,
        ));
    }
    let next = offset
        .checked_add(chunk.len() as u32)
        .ok_or(UpdateStreamError::Image(UpdateImageStreamError::Overflow))?;
    if next > status.image_size {
        return Err(UpdateStreamError::Image(UpdateImageStreamError::Overflow));
    }
    Ok(())
}

fn append_chunk_inner(
    session: &mut UpdateSession,
    offset: u32,
    chunk: &[u8],
) -> Result<UpdateStatus, UpdateStreamError> {
    validate_chunk_bounds(&session.verifier, offset, chunk)?;
    if let Some(writer) = session.writer.as_mut() {
        writer
            .write_chunk(offset, chunk)
            .map_err(UpdateStreamError::Flash)?;
    }

    session
        .verifier
        .append_chunk(offset, chunk)
        .map_err(UpdateStreamError::Image)
}

impl OtaWriter {
    fn new(manifest: &UpdateManifest) -> Result<Self, UpdateFlashError> {
        let target = discover_ota_target(manifest.image_size)?;
        Ok(Self {
            flash: FlashStorage::new(),
            target,
            image_size: manifest.image_size,
            image_sha256: manifest.image_sha256,
            sector_base: None,
            sector: [0xff; OTA_SECTOR_SIZE],
            sector_dirty: false,
        })
    }

    fn write_chunk(&mut self, offset: u32, chunk: &[u8]) -> Result<(), UpdateFlashError> {
        let end = offset
            .checked_add(chunk.len() as u32)
            .ok_or(UpdateFlashError::OutOfBounds)?;
        if end > self.target.len {
            return Err(UpdateFlashError::OutOfBounds);
        }

        let mut cursor = offset;
        let mut remaining = chunk;
        while !remaining.is_empty() {
            let sector_base = cursor / FlashStorage::SECTOR_SIZE * FlashStorage::SECTOR_SIZE;
            self.ensure_sector(sector_base)?;

            let sector_offset = (cursor - sector_base) as usize;
            let room = OTA_SECTOR_SIZE - sector_offset;
            let take = remaining.len().min(room);
            self.sector[sector_offset..sector_offset + take].copy_from_slice(&remaining[..take]);
            self.sector_dirty = true;

            cursor = cursor
                .checked_add(take as u32)
                .ok_or(UpdateFlashError::OutOfBounds)?;
            remaining = &remaining[take..];

            if sector_offset + take == OTA_SECTOR_SIZE {
                self.flush_sector()?;
            }
        }
        Ok(())
    }

    fn ensure_sector(&mut self, sector_base: u32) -> Result<(), UpdateFlashError> {
        if self.sector_base == Some(sector_base) {
            return Ok(());
        }

        self.flush_sector()?;
        self.sector.fill(0xff);
        self.sector_base = Some(sector_base);
        Ok(())
    }

    fn flush_sector(&mut self) -> Result<(), UpdateFlashError> {
        if !self.sector_dirty {
            return Ok(());
        }

        let sector_base = self.sector_base.ok_or(UpdateFlashError::OutOfBounds)?;
        let address = self
            .target
            .offset
            .checked_add(sector_base)
            .ok_or(UpdateFlashError::OutOfBounds)?;
        let remaining = self
            .target
            .len
            .checked_sub(sector_base)
            .ok_or(UpdateFlashError::OutOfBounds)? as usize;
        let write_len = remaining.min(OTA_SECTOR_SIZE);
        let erase_end = address
            .checked_add(write_len as u32)
            .ok_or(UpdateFlashError::OutOfBounds)?;
        embedded_storage::nor_flash::NorFlash::erase(&mut self.flash, address, erase_end)
            .map_err(|_| UpdateFlashError::Storage)?;
        embedded_storage::nor_flash::NorFlash::write(
            &mut self.flash,
            address,
            &self.sector[..write_len],
        )
        .map_err(|_| UpdateFlashError::Storage)?;
        self.sector_dirty = false;
        Ok(())
    }

    fn activate(mut self) -> Result<(), UpdateFlashError> {
        self.flush_sector()?;
        self.verify_written_image()?;
        activate_ota_slot(self.target.slot)
    }

    fn verify_written_image(&mut self) -> Result<(), UpdateFlashError> {
        let mut hasher = Sha256::new();
        let mut cursor = 0u32;
        while cursor < self.image_size {
            let remaining = self
                .image_size
                .checked_sub(cursor)
                .ok_or(UpdateFlashError::OutOfBounds)? as usize;
            let take = remaining.min(self.sector.len());
            let address = self
                .target
                .offset
                .checked_add(cursor)
                .ok_or(UpdateFlashError::OutOfBounds)?;
            self.flash
                .read(address, &mut self.sector[..take])
                .map_err(|_| UpdateFlashError::Storage)?;
            hasher.update(&self.sector[..take]);
            cursor = cursor
                .checked_add(take as u32)
                .ok_or(UpdateFlashError::OutOfBounds)?;
        }

        let digest = hasher.finalize();
        if digest.as_slice() != self.image_sha256 {
            return Err(UpdateFlashError::VerifyMismatch);
        }
        Ok(())
    }
}

fn discover_ota_target(image_size: u32) -> Result<OtaTarget, UpdateFlashError> {
    let mut flash = FlashStorage::new();
    let mut table_storage = [0u8; PARTITION_TABLE_MAX_LEN];
    let table = read_partition_table(&mut flash, &mut table_storage)
        .map_err(|_| UpdateFlashError::PartitionTable)?;

    let current_slot = {
        let otadata = table
            .find_partition(PartitionType::Data(DataPartitionSubType::Ota))
            .map_err(|_| UpdateFlashError::PartitionTable)?
            .ok_or(UpdateFlashError::MissingOtaData)?;
        let mut otadata_region = otadata.as_embedded_storage(&mut flash);
        let mut ota =
            Ota::new(&mut otadata_region).map_err(|_| UpdateFlashError::PartitionTable)?;
        ota.current_slot()
            .map_err(|_| UpdateFlashError::PartitionTable)?
    };

    let target_slot = current_slot.next();
    let subtype = match target_slot {
        Slot::Slot0 | Slot::None => AppPartitionSubType::Ota0,
        Slot::Slot1 => AppPartitionSubType::Ota1,
    };
    let target = table
        .find_partition(PartitionType::App(subtype))
        .map_err(|_| UpdateFlashError::PartitionTable)?
        .ok_or(UpdateFlashError::MissingOtaSlot)?;

    if image_size > target.len() {
        return Err(UpdateFlashError::ImageTooLarge);
    }
    if target.offset() % FlashStorage::SECTOR_SIZE != 0
        || target.len() % FlashStorage::SECTOR_SIZE != 0
    {
        return Err(UpdateFlashError::OutOfBounds);
    }
    if target.offset().saturating_add(target.len()) as usize > flash.capacity() {
        return Err(UpdateFlashError::OutOfBounds);
    }

    Ok(OtaTarget {
        slot: target_slot,
        offset: target.offset(),
        len: target.len(),
    })
}

fn activate_ota_slot(slot: Slot) -> Result<(), UpdateFlashError> {
    let mut flash = FlashStorage::new();
    let mut table_storage = [0u8; PARTITION_TABLE_MAX_LEN];
    let table = read_partition_table(&mut flash, &mut table_storage)
        .map_err(|_| UpdateFlashError::PartitionTable)?;
    let otadata = table
        .find_partition(PartitionType::Data(DataPartitionSubType::Ota))
        .map_err(|_| UpdateFlashError::PartitionTable)?
        .ok_or(UpdateFlashError::MissingOtaData)?;
    let mut otadata_region = otadata.as_embedded_storage(&mut flash);
    let mut ota = Ota::new(&mut otadata_region).map_err(|_| UpdateFlashError::PartitionTable)?;
    ota.set_current_slot(slot)
        .map_err(|_| UpdateFlashError::Storage)?;
    ota.set_current_ota_state(OtaImageState::New)
        .map_err(|_| UpdateFlashError::Storage)
}

fn slot_code(slot: Slot) -> u8 {
    match slot {
        Slot::None => UPDATE_SLOT_NONE,
        Slot::Slot0 => UPDATE_SLOT_OTA0,
        Slot::Slot1 => UPDATE_SLOT_OTA1,
    }
}

fn ota_state_code(state: OtaImageState) -> u8 {
    match state {
        OtaImageState::New => UPDATE_OTA_STATE_NEW,
        OtaImageState::PendingVerify => UPDATE_OTA_STATE_PENDING_VERIFY,
        OtaImageState::Valid => UPDATE_OTA_STATE_VALID,
        OtaImageState::Invalid => UPDATE_OTA_STATE_INVALID,
        OtaImageState::Aborted => UPDATE_OTA_STATE_ABORTED,
        OtaImageState::Undefined => UPDATE_OTA_STATE_UNDEFINED,
    }
}

fn parse_hex_32(hex: &str) -> Option<[u8; 32]> {
    let mut out = [0u8; 32];
    let bytes = hex.as_bytes();
    if bytes.len() != 64 {
        return None;
    }

    let mut i = 0usize;
    while i < 32 {
        let hi = hex_nibble(bytes[i * 2])?;
        let lo = hex_nibble(bytes[i * 2 + 1])?;
        out[i] = (hi << 4) | lo;
        i += 1;
    }
    Some(out)
}

pub fn firmware_release_version() -> u32 {
    option_env!("NOCKSTER_RELEASE_VERSION")
        .and_then(parse_u32_decimal)
        .unwrap_or(0)
}

fn parse_u32_decimal(value: &str) -> Option<u32> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut out = 0u32;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if !b.is_ascii_digit() {
            return None;
        }
        out = out.checked_mul(10)?.checked_add((b - b'0') as u32)?;
        i += 1;
    }
    Some(out)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
