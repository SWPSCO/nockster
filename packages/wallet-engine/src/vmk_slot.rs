use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Mutex,
};

use once_cell::sync::Lazy;
use wasm_bindgen::JsValue;
use zeroize::Zeroize;

use crate::js_error::js_error;

static SLOT_COUNTER: AtomicU32 = AtomicU32::new(1);
static VMK_SLOTS: Lazy<Mutex<HashMap<u32, [u8; 32]>>> = Lazy::new(|| Mutex::new(HashMap::new()));

fn lock_slots() -> Result<std::sync::MutexGuard<'static, HashMap<u32, [u8; 32]>>, JsValue> {
    VMK_SLOTS
        .lock()
        .map_err(|_| js_error("vmk slot registry poisoned"))
}

pub struct VmkSlots;

impl VmkSlots {
    pub fn allocate(vmk: &[u8; 32]) -> Result<u32, JsValue> {
        let slot = SLOT_COUNTER.fetch_add(1, Ordering::SeqCst);
        let mut slots = lock_slots()?;
        slots.insert(slot, *vmk);
        Ok(slot)
    }

    pub fn with_vmk<T, F>(slot: u32, f: F) -> Result<T, JsValue>
    where
        F: FnOnce(&[u8; 32]) -> Result<T, JsValue>,
    {
        let slots = lock_slots()?;
        let vmk = slots
            .get(&slot)
            .ok_or_else(|| js_error("vmk slot not found"))?;
        f(vmk)
    }

    pub fn free(slot: u32) -> Result<(), JsValue> {
        let mut slots = lock_slots()?;
        if let Some(mut vmk) = slots.remove(&slot) {
            vmk.zeroize();
            Ok(())
        } else {
            Err(js_error("vmk slot not found"))
        }
    }

    /// Export the VMK from a slot (for session persistence).
    /// Returns a copy of the 32-byte key.
    /// SECURITY: The caller is responsible for securely handling and zeroizing the result.
    pub fn export(slot: u32) -> Result<[u8; 32], JsValue> {
        let slots = lock_slots()?;
        let vmk = slots
            .get(&slot)
            .ok_or_else(|| js_error("vmk slot not found"))?;
        Ok(*vmk)
    }
}
