use crate::vmk_slot::VmkSlots;
use base64::{engine::general_purpose::STANDARD, Engine};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use wasm_bindgen::prelude::*;
use zeroize::Zeroizing;

// Bind credentials to their service, network, and wallet without changing vault data.
#[wasm_bindgen(js_name = sealSecret)]
pub fn seal_secret(slot: u32, context: &str, value: &str) -> Result<String, JsValue> {
    VmkSlots::with_vmk(slot, |vmk| {
        let mut nonce = [0u8; 12];
        getrandom::getrandom(&mut nonce).map_err(|_| JsValue::from_str("Secret nonce failed"))?;
        let aad = format!("fletch-secret-v1:{context}");
        let ciphertext = ChaCha20Poly1305::new(vmk.into())
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: value.as_bytes(),
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| JsValue::from_str("Secret encryption failed"))?;
        Ok(STANDARD.encode([nonce.as_slice(), ciphertext.as_slice()].concat()))
    })
}

#[wasm_bindgen(js_name = openSecret)]
pub fn open_secret(slot: u32, context: &str, envelope: &str) -> Result<String, JsValue> {
    VmkSlots::with_vmk(slot, |vmk| {
        let blob = STANDARD
            .decode(envelope)
            .map_err(|_| JsValue::from_str("Invalid secret envelope"))?;
        if blob.len() < 28 {
            return Err(JsValue::from_str("Invalid secret envelope"));
        }
        let aad = format!("fletch-secret-v1:{context}");
        let plaintext = Zeroizing::new(
            ChaCha20Poly1305::new(vmk.into())
                .decrypt(
                    Nonce::from_slice(&blob[..12]),
                    Payload {
                        msg: &blob[12..],
                        aad: aad.as_bytes(),
                    },
                )
                .map_err(|_| JsValue::from_str("Secret authentication failed"))?,
        );
        std::str::from_utf8(&plaintext)
            .map(str::to_owned)
            .map_err(|_| JsValue::from_str("Invalid secret text"))
    })
}
