use core::cell::RefCell;
use core::num::NonZeroU32;

use critical_section::Mutex;

// Espressif's `esp_random()` rate-limits reads so fresh physical entropy can be
// mixed into the hardware RNG state. esp-hal's raw register wrapper does not.
// Ten kHz is conservatively below Espressif's documented 15-75 kHz chip-specific
// maximum read rate and costs less than a millisecond for a 256-bit seed.
const RNG_READ_INTERVAL_US: u32 = 100;

// Continuous RNG test state. Keeping this across getrandom calls detects a
// stuck RNG even when callers request only one word at a time.
static LAST_RNG_WORD: Mutex<RefCell<Option<u32>>> = Mutex::new(RefCell::new(None));

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
    use esp_hal::delay::Delay;
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
    let delay = Delay::new();

    for chunk in buf.chunks_mut(4) {
        // Serialize and rate-limit at the hardware register. The critical
        // section is per word, so interrupts are held off for at most 200 us
        // on the first read and 100 us thereafter.
        let word = critical_section::with(|cs| {
            let mut last = LAST_RNG_WORD.borrow_ref_mut(cs);

            // Prime the continuous test with a discarded sample so even a
            // one-byte request is checked against a previous hardware word.
            if last.is_none() {
                delay.delay_micros(RNG_READ_INTERVAL_US);
                *last = Some(rng.random());
            }

            delay.delay_micros(RNG_READ_INTERVAL_US);
            let word = rng.random();
            if *last == Some(word) {
                return Err(Error);
            }
            *last = Some(word);
            Ok(word)
        });

        let word = match word {
            Ok(word) => word,
            Err(error) => {
                // Never return partial or suspect entropy to a caller.
                buf.fill(0);
                return Err(error.into());
            }
        };
        let bytes = word.to_le_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
    Ok(())
}
