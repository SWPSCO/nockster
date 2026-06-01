use core::num::NonZeroU32;

#[derive(Debug)]
struct Error;

impl From<Error> for getrandom::Error {
    fn from(_: Error) -> Self {
        NonZeroU32::new(getrandom::Error::CUSTOM_START)
            .unwrap()
            .into()
    }
}

getrandom::register_custom_getrandom!(esp32_getrandom);

fn esp32_getrandom(buf: &mut [u8]) -> Result<(), getrandom::Error> {
    // ESP32-S3 RNG_DATA_REG is at 0x60033110
    // (ESP32 original used 0x3FF75144 - wrong for S3!)
    //
    // main() keeps esp_hal::rng::Trng alive so the ADC/RC_FAST entropy source
    // is enabled before salts and nonces are generated.
    const RNG_DATA_REG: *const u32 = 0x60033110 as *const u32;

    for chunk in buf.chunks_mut(4) {
        let random = unsafe { core::ptr::read_volatile(RNG_DATA_REG) };
        let bytes = random.to_ne_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
    Ok(())
}
