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
    use esp_hal::peripherals::RNG;
    use esp_hal::rng::Rng;

    // Read through esp-hal's RNG driver so the correct chip register is used.
    // The previous implementation hardcoded `0x60033110`, which is NOT the
    // ESP32-S3 RNG data register — it read back zeros, so every "random" value
    // (NVS salts/nonces and on-device seed generation) was all-zero.
    //
    // SAFETY: the real `Trng` created in `main()` owns the RNG peripheral and
    // keeps the hardware entropy source (ADC/RC_FAST) enabled. Stealing a handle
    // here only performs read-only access to the RNG data register, which is
    // sound to do concurrently.
    let mut rng = Rng::new(unsafe { RNG::steal() });
    rng.read(buf);
    Ok(())
}
