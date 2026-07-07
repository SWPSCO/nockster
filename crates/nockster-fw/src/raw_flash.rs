use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};

use embedded_storage::nor_flash::{
    ErrorType, NorFlash, NorFlashError, NorFlashErrorKind, ReadNorFlash,
};
use embedded_storage::{ReadStorage, Storage};
use esp_storage::ll;

const FLASH_SIZE_BYTES: usize = 16 * 1024 * 1024;

#[repr(C, align(4))]
struct FlashSectorBuffer {
    data: [u8; RawFlashStorage::SECTOR_SIZE as usize],
}

impl Deref for FlashSectorBuffer {
    type Target = [u8; RawFlashStorage::SECTOR_SIZE as usize];

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl DerefMut for FlashSectorBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawFlashError {
    NotAligned,
    OutOfBounds,
    Rom(i32),
}

impl NorFlashError for RawFlashError {
    fn kind(&self) -> NorFlashErrorKind {
        match self {
            Self::NotAligned => NorFlashErrorKind::NotAligned,
            Self::OutOfBounds => NorFlashErrorKind::OutOfBounds,
            Self::Rom(_) => NorFlashErrorKind::Other,
        }
    }
}

#[derive(Debug)]
pub struct RawFlashStorage {
    unlocked: bool,
}

impl Default for RawFlashStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl RawFlashStorage {
    pub const WORD_SIZE: u32 = 4;
    pub const SECTOR_SIZE: u32 = 4096;

    pub fn new() -> Self {
        Self { unlocked: false }
    }

    fn check_bounds(&self, offset: u32, length: usize) -> Result<(), RawFlashError> {
        let offset = offset as usize;
        if length > FLASH_SIZE_BYTES || offset > FLASH_SIZE_BYTES - length {
            return Err(RawFlashError::OutOfBounds);
        }
        Ok(())
    }

    fn check_alignment<const ALIGN: u32>(
        &self,
        offset: u32,
        length: usize,
    ) -> Result<(), RawFlashError> {
        if offset as usize % ALIGN as usize != 0 || length % ALIGN as usize != 0 {
            return Err(RawFlashError::NotAligned);
        }
        Ok(())
    }

    fn unlock_once(&mut self) -> Result<(), RawFlashError> {
        if !self.unlocked {
            unsafe { ll::spiflash_unlock() }.map_err(RawFlashError::Rom)?;
            self.unlocked = true;
        }
        Ok(())
    }

    #[inline(never)]
    #[link_section = ".rwtext"]
    fn internal_read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), RawFlashError> {
        unsafe { ll::spiflash_read(offset, bytes.as_mut_ptr() as *mut u32, bytes.len() as u32) }
            .map_err(RawFlashError::Rom)
    }

    #[inline(never)]
    #[link_section = ".rwtext"]
    fn internal_erase(&mut self, sector: u32) -> Result<(), RawFlashError> {
        self.unlock_once()?;
        unsafe { ll::spiflash_erase_sector(sector) }.map_err(RawFlashError::Rom)
    }

    #[inline(never)]
    #[link_section = ".rwtext"]
    fn internal_write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), RawFlashError> {
        self.unlock_once()?;
        unsafe { ll::spiflash_write(offset, bytes.as_ptr() as *const u32, bytes.len() as u32) }
            .map_err(RawFlashError::Rom)
    }

    fn is_word_aligned<T: ?Sized>(value: &T) -> bool {
        (core::ptr::from_ref(value).cast::<()>() as usize) % Self::WORD_SIZE as usize == 0
    }
}

impl ReadStorage for RawFlashStorage {
    type Error = RawFlashError;

    fn read(&mut self, offset: u32, mut bytes: &mut [u8]) -> Result<(), Self::Error> {
        self.check_bounds(offset, bytes.len())?;

        let mut data_offset = offset % Self::WORD_SIZE;
        let mut aligned_offset = offset - data_offset;

        let mut sector_data = MaybeUninit::<FlashSectorBuffer>::uninit();
        let sector_data = unsafe { sector_data.assume_init_mut() };

        while !bytes.is_empty() {
            let len = bytes.len().min((Self::SECTOR_SIZE - data_offset) as usize);
            let aligned_end = (data_offset as usize + len + (Self::WORD_SIZE - 1) as usize)
                & !(Self::WORD_SIZE - 1) as usize;

            self.internal_read(aligned_offset, &mut sector_data[..aligned_end])?;
            bytes[..len].copy_from_slice(&sector_data[data_offset as usize..][..len]);

            aligned_offset += Self::SECTOR_SIZE;
            data_offset = 0;
            bytes = &mut bytes[len..];
        }

        Ok(())
    }

    fn capacity(&self) -> usize {
        FLASH_SIZE_BYTES
    }
}

impl Storage for RawFlashStorage {
    fn write(&mut self, offset: u32, mut bytes: &[u8]) -> Result<(), Self::Error> {
        self.check_bounds(offset, bytes.len())?;

        let mut data_offset = offset % Self::SECTOR_SIZE;
        let mut aligned_offset = offset - data_offset;

        let mut sector_data = MaybeUninit::<FlashSectorBuffer>::uninit();
        let sector_data = unsafe { sector_data.assume_init_mut() };

        while !bytes.is_empty() {
            self.internal_read(aligned_offset, &mut sector_data[..])?;

            let len = bytes.len().min((Self::SECTOR_SIZE - data_offset) as usize);
            sector_data[data_offset as usize..][..len].copy_from_slice(&bytes[..len]);
            self.internal_erase(aligned_offset / Self::SECTOR_SIZE)?;
            self.internal_write(aligned_offset, &sector_data[..])?;

            aligned_offset += Self::SECTOR_SIZE;
            data_offset = 0;
            bytes = &bytes[len..];
        }

        Ok(())
    }
}

impl ErrorType for RawFlashStorage {
    type Error = RawFlashError;
}

impl ReadNorFlash for RawFlashStorage {
    const READ_SIZE: usize = Self::WORD_SIZE as usize;

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        self.check_alignment::<{ Self::READ_SIZE as u32 }>(offset, bytes.len())?;
        self.check_bounds(offset, bytes.len())?;

        if Self::is_word_aligned(bytes) {
            for (offset, chunk) in (offset..)
                .step_by(Self::SECTOR_SIZE as usize)
                .zip(bytes.chunks_mut(Self::SECTOR_SIZE as usize))
            {
                self.internal_read(offset, chunk)?;
            }
        } else {
            let mut buffer = MaybeUninit::<FlashSectorBuffer>::uninit();
            let buffer = unsafe { buffer.assume_init_mut() };

            for (offset, chunk) in (offset..)
                .step_by(Self::SECTOR_SIZE as usize)
                .zip(bytes.chunks_mut(Self::SECTOR_SIZE as usize))
            {
                self.internal_read(offset, &mut buffer[..chunk.len()])?;
                chunk.copy_from_slice(&buffer[..chunk.len()]);
            }
        }

        Ok(())
    }

    fn capacity(&self) -> usize {
        FLASH_SIZE_BYTES
    }
}

impl NorFlash for RawFlashStorage {
    const WRITE_SIZE: usize = Self::WORD_SIZE as usize;
    const ERASE_SIZE: usize = Self::SECTOR_SIZE as usize;

    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        let len = to.checked_sub(from).ok_or(RawFlashError::OutOfBounds)? as usize;
        self.check_alignment::<{ Self::SECTOR_SIZE }>(from, len)?;
        self.check_bounds(from, len)?;

        for sector in from / Self::SECTOR_SIZE..to / Self::SECTOR_SIZE {
            self.internal_erase(sector)?;
        }
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        self.check_alignment::<{ Self::WORD_SIZE }>(offset, bytes.len())?;
        self.check_bounds(offset, bytes.len())?;

        if Self::is_word_aligned(bytes) {
            for (offset, chunk) in (offset..)
                .step_by(Self::SECTOR_SIZE as usize)
                .zip(bytes.chunks(Self::SECTOR_SIZE as usize))
            {
                self.internal_write(offset, chunk)?;
            }
        } else {
            let mut buffer = MaybeUninit::<FlashSectorBuffer>::uninit();
            let buffer = unsafe { buffer.assume_init_mut() };

            for (offset, chunk) in (offset..)
                .step_by(Self::SECTOR_SIZE as usize)
                .zip(bytes.chunks(Self::SECTOR_SIZE as usize))
            {
                buffer[..chunk.len()].copy_from_slice(chunk);
                self.internal_write(offset, &buffer[..chunk.len()])?;
            }
        }

        Ok(())
    }
}
