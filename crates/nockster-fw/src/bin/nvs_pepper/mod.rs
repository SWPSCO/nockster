#[cfg(feature = "chip-security")]
use esp_hal::{
    efuse::{
        Efuse, KEY_PURPOSE_0, KEY_PURPOSE_1, KEY_PURPOSE_2, KEY_PURPOSE_3, KEY_PURPOSE_4,
        KEY_PURPOSE_5,
    },
    hmac::{Hmac, HmacPurpose, KeyId},
    peripherals::HMAC,
};
#[cfg(feature = "chip-security")]
use nockster_core::HMAC_KEY_PURPOSE_UP;
#[cfg(feature = "chip-security")]
use nockster_fw::nvs_store::nvs_v2_pepper_message;
use nockster_fw::nvs_store::{NvsError, NvsPepperSource};
#[cfg(feature = "chip-security")]
use zeroize::Zeroize;

#[cfg(feature = "chip-security")]
pub struct AppNvsPepper<'d> {
    hmac: Hmac<'d>,
    key_id: Option<KeyId>,
    mac: [u8; 6],
}

#[cfg(not(feature = "chip-security"))]
pub struct AppNvsPepper<'d> {
    _marker: core::marker::PhantomData<&'d ()>,
}

#[cfg(feature = "chip-security")]
impl<'d> AppNvsPepper<'d> {
    pub fn new(hmac: HMAC<'d>) -> Self {
        Self {
            hmac: Hmac::new(hmac),
            key_id: hmac_up_key_id(),
            mac: Efuse::read_base_mac_address(),
        }
    }
}

#[cfg(not(feature = "chip-security"))]
impl<'d> AppNvsPepper<'d> {
    pub fn new() -> Self {
        Self {
            _marker: core::marker::PhantomData,
        }
    }
}

#[cfg(feature = "chip-security")]
impl NvsPepperSource for AppNvsPepper<'_> {
    fn nvs_v2_pepper(&mut self, salt: &[u8; 32]) -> Result<Option<[u8; 32]>, NvsError> {
        let Some(key_id) = self.key_id else {
            return Ok(None);
        };

        let mut message = nvs_v2_pepper_message(salt, &self.mac);
        let result = self.hmac_up(&message, key_id);
        message.zeroize();
        result
    }
}

#[cfg(not(feature = "chip-security"))]
impl NvsPepperSource for AppNvsPepper<'_> {
    fn nvs_v2_pepper(&mut self, _salt: &[u8; 32]) -> Result<Option<[u8; 32]>, NvsError> {
        Ok(None)
    }
}

#[cfg(feature = "chip-security")]
impl AppNvsPepper<'_> {
    fn hmac_up(&mut self, message: &[u8], key_id: KeyId) -> Result<Option<[u8; 32]>, NvsError> {
        let mut out = [0u8; 32];
        self.hmac.init();
        match nb::block!(self.hmac.configure(HmacPurpose::ToUser, key_id)) {
            Ok(()) => {}
            Err(_) => return Err(NvsError::Crypto),
        }

        let mut remaining = message;
        while !remaining.is_empty() {
            remaining = nb::block!(self.hmac.update(remaining)).map_err(|_| NvsError::Crypto)?;
        }
        nb::block!(self.hmac.finalize(&mut out)).map_err(|_| NvsError::Crypto)?;
        Ok(Some(out))
    }
}

#[cfg(feature = "chip-security")]
fn hmac_up_key_id() -> Option<KeyId> {
    let purposes = [
        Efuse::read_field_le::<u8>(KEY_PURPOSE_0),
        Efuse::read_field_le::<u8>(KEY_PURPOSE_1),
        Efuse::read_field_le::<u8>(KEY_PURPOSE_2),
        Efuse::read_field_le::<u8>(KEY_PURPOSE_3),
        Efuse::read_field_le::<u8>(KEY_PURPOSE_4),
        Efuse::read_field_le::<u8>(KEY_PURPOSE_5),
    ];

    purposes
        .iter()
        .position(|purpose| *purpose == HMAC_KEY_PURPOSE_UP)
        .and_then(key_id_from_index)
}

#[cfg(feature = "chip-security")]
fn key_id_from_index(index: usize) -> Option<KeyId> {
    match index {
        0 => Some(KeyId::Key0),
        1 => Some(KeyId::Key1),
        2 => Some(KeyId::Key2),
        3 => Some(KeyId::Key3),
        4 => Some(KeyId::Key4),
        5 => Some(KeyId::Key5),
        _ => None,
    }
}
