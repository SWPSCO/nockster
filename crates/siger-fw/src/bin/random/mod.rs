use core::num::NonZeroU32;

#[derive(Debug)]
struct Error;

impl From<Error> for getrandom::Error {
    fn from(_: Error) -> Self {
        NonZeroU32::new(getrandom::Error::CUSTOM_START).unwrap().into()
    }
}

getrandom::register_custom_getrandom!(esp32_getrandom);

fn esp32_getrandom(buf: &mut [u8]) -> Result<(), getrandom::Error> {
    for chunk in buf.chunks_mut(4) {
        let random = unsafe { 
            core::ptr::read_volatile(0x3FF75144 as *const u32)
        };
        let bytes = random.to_ne_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
    Ok(())
}