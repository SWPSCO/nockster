use alloc::vec::Vec;
use core::cell::RefCell;
use critical_section::Mutex;
use esp_hal::delay::Delay;
use heapless::String as HString;
use nockster_core::{Frame, Response};
use nockster_fw::nvs_store::{
    worker_flash_pause_requested, worker_flash_pause_set_online, worker_flash_pause_set_parked,
    PreparedSeedInit, PreparedSeedRewrite,
};

use crate::gui::SeedPhrase;

pub enum UnlockOutcome {
    Success {
        seeds: Vec<[u8; 64]>,
        master_key: [u8; 32],
        clear_attempts: bool,
    },
    WrongPin,
    LockedOut,
    NotInitialized,
    Flash,
    Failed,
}

pub struct UnlockRequest {
    pub pin: HString<16>,
}

pub struct InitPinRequest {
    pub pin: HString<16>,
    pub seed64: [u8; 64],
}

pub struct SeedDeriveRequest {
    pub phrase: SeedPhrase,
}

pub enum InitPinOutcome {
    Prepared {
        seed64: [u8; 64],
        master_key: [u8; 32],
        prepared: PreparedSeedInit,
    },
    AlreadyInitialized,
    Flash,
    Crypto,
    Failed,
}

pub enum SeedDeriveOutcome {
    Success { seed64: [u8; 64] },
    Failed,
}

pub struct SignDraftRequest {
    pub msg_id: u32,
    pub frag_id: u16,
    pub sk_be: [u8; 32],
    pub draft: Vec<u8>,
}

pub struct DirectSignRequest {
    pub msg_id: u32,
    pub frame: Frame,
}

pub struct ChangePinRequest {
    pub msg_id: u32,
    pub current_pin: HString<16>,
    pub new_pin: HString<16>,
}

pub enum SeedOp {
    Add {
        seed64: [u8; 64],
        master_key: [u8; 32],
    },
    Delete {
        slot: u8,
        master_key: [u8; 32],
    },
    Reset,
}

pub struct SeedOpRequest {
    pub msg_id: u32,
    pub op: SeedOp,
}

pub enum SignDraftOutcome {
    Success {
        msg_id: u32,
        frag_id: u16,
        out: Vec<u8>,
    },
    Unsupported {
        msg_id: u32,
    },
    Failed {
        msg_id: u32,
    },
}

pub struct DirectSignOutcome {
    pub msg_id: u32,
    pub response: Response,
}

pub enum ChangePinOutcome {
    Prepared {
        msg_id: u32,
        seeds: Vec<[u8; 64]>,
        master_key: [u8; 32],
        prepared: PreparedSeedRewrite,
    },
    WrongPin {
        msg_id: u32,
    },
    LockedOut {
        msg_id: u32,
    },
    Flash {
        msg_id: u32,
    },
    Crypto {
        msg_id: u32,
    },
    Failed {
        msg_id: u32,
    },
}

pub enum SeedOpOutcome {
    Added { msg_id: u32, seed64: [u8; 64] },
    Deleted { msg_id: u32, slot: u8 },
    Reset { msg_id: u32 },
    WrongPin { msg_id: u32 },
    LockedOut { msg_id: u32 },
    Full { msg_id: u32 },
    InvalidSlot { msg_id: u32 },
    NotInitialized { msg_id: u32 },
    Flash { msg_id: u32 },
    Crypto { msg_id: u32 },
    Failed { msg_id: u32 },
}

#[allow(clippy::declare_interior_mutable_const)]
static UNLOCK_REQUEST: Mutex<RefCell<Option<UnlockRequest>>> = Mutex::new(RefCell::new(None));
#[allow(clippy::declare_interior_mutable_const)]
static UNLOCK_RESULT: Mutex<RefCell<Option<UnlockOutcome>>> = Mutex::new(RefCell::new(None));

#[allow(clippy::declare_interior_mutable_const)]
static INITPIN_REQUEST: Mutex<RefCell<Option<InitPinRequest>>> = Mutex::new(RefCell::new(None));
#[allow(clippy::declare_interior_mutable_const)]
static INITPIN_RESULT: Mutex<RefCell<Option<InitPinOutcome>>> = Mutex::new(RefCell::new(None));

#[allow(clippy::declare_interior_mutable_const)]
static SEED_DERIVE_REQUEST: Mutex<RefCell<Option<SeedDeriveRequest>>> =
    Mutex::new(RefCell::new(None));
#[allow(clippy::declare_interior_mutable_const)]
static SEED_DERIVE_RESULT: Mutex<RefCell<Option<SeedDeriveOutcome>>> =
    Mutex::new(RefCell::new(None));

#[allow(clippy::declare_interior_mutable_const)]
static SIGN_DRAFT_REQUEST: Mutex<RefCell<Option<SignDraftRequest>>> =
    Mutex::new(RefCell::new(None));
#[allow(clippy::declare_interior_mutable_const)]
static SIGN_DRAFT_RESULT: Mutex<RefCell<Option<SignDraftOutcome>>> = Mutex::new(RefCell::new(None));

#[allow(clippy::declare_interior_mutable_const)]
static DIRECT_SIGN_REQUEST: Mutex<RefCell<Option<DirectSignRequest>>> =
    Mutex::new(RefCell::new(None));
#[allow(clippy::declare_interior_mutable_const)]
static DIRECT_SIGN_RESULT: Mutex<RefCell<Option<DirectSignOutcome>>> =
    Mutex::new(RefCell::new(None));

#[allow(clippy::declare_interior_mutable_const)]
static CHANGE_PIN_REQUEST: Mutex<RefCell<Option<ChangePinRequest>>> =
    Mutex::new(RefCell::new(None));
#[allow(clippy::declare_interior_mutable_const)]
static CHANGE_PIN_RESULT: Mutex<RefCell<Option<ChangePinOutcome>>> = Mutex::new(RefCell::new(None));

pub struct UnlockController {
    awaiting_result: bool,
}

pub struct InitPinController;

pub struct SeedDeriveController {
    awaiting_result: bool,
}

pub struct SignDraftController {
    awaiting_result: bool,
}

pub struct DirectSignController {
    awaiting_result: bool,
}

pub struct ChangePinController {
    awaiting_result: bool,
}

impl UnlockController {
    pub fn new() -> Self {
        Self {
            awaiting_result: false,
        }
    }

    pub fn submit(&mut self, pin: &str) -> Result<(), ()> {
        if self.awaiting_result {
            return Err(());
        }

        let mut pin_buf = HString::<16>::new();
        pin_buf.push_str(pin).map_err(|_| ())?;

        critical_section::with(|cs| {
            let mut pending = UNLOCK_REQUEST.borrow_ref_mut(cs);
            if pending.is_some() {
                return Err(());
            }
            if UNLOCK_RESULT.borrow_ref(cs).is_some() {
                return Err(());
            }
            *pending = Some(UnlockRequest { pin: pin_buf });
            Ok(())
        })?;

        self.awaiting_result = true;
        Ok(())
    }

    pub fn poll(&mut self) -> Option<UnlockOutcome> {
        let outcome = critical_section::with(|cs| {
            let mut slot = UNLOCK_RESULT.borrow_ref_mut(cs);
            let outcome = slot.take();
            if outcome.is_some() {
                self.awaiting_result = false;
            }
            outcome
        });

        if let Some(result) = outcome {
            return Some(result);
        }

        None
    }
}

impl InitPinController {
    pub fn new() -> Self {
        Self
    }

    pub fn submit(&self, pin: &str, seed64: &[u8; 64]) -> Result<(), ()> {
        let mut pin_buf = HString::<16>::new();
        pin_buf.push_str(pin).map_err(|_| ())?;
        critical_section::with(|cs| {
            let mut pending = INITPIN_REQUEST.borrow_ref_mut(cs);
            if pending.is_some() || INITPIN_RESULT.borrow_ref(cs).is_some() {
                return Err(());
            }
            *pending = Some(InitPinRequest {
                pin: pin_buf,
                seed64: *seed64,
            });
            Ok(())
        })
    }

    pub fn poll(&mut self) -> Option<InitPinOutcome> {
        critical_section::with(|cs| {
            let mut slot = INITPIN_RESULT.borrow_ref_mut(cs);
            slot.take()
        })
    }
}

impl SeedDeriveController {
    pub fn new() -> Self {
        Self {
            awaiting_result: false,
        }
    }

    pub fn submit(&mut self, phrase: SeedPhrase) -> Result<(), SeedPhrase> {
        if self.awaiting_result {
            return Err(phrase);
        }

        let mut request = Some(SeedDeriveRequest { phrase });
        let queued = critical_section::with(|cs| {
            let mut pending = SEED_DERIVE_REQUEST.borrow_ref_mut(cs);
            if pending.is_some() || SEED_DERIVE_RESULT.borrow_ref(cs).is_some() {
                false
            } else {
                *pending = request.take();
                true
            }
        });

        if queued {
            self.awaiting_result = true;
            Ok(())
        } else {
            Err(request
                .expect("request still available when not queued")
                .phrase)
        }
    }

    pub fn poll(&mut self) -> Option<SeedDeriveOutcome> {
        let outcome = critical_section::with(|cs| {
            let mut slot = SEED_DERIVE_RESULT.borrow_ref_mut(cs);
            let outcome = slot.take();
            if outcome.is_some() {
                self.awaiting_result = false;
            }
            outcome
        });

        if let Some(result) = outcome {
            return Some(result);
        }

        None
    }
}

impl SignDraftController {
    pub fn new() -> Self {
        Self {
            awaiting_result: false,
        }
    }

    pub fn submit(&mut self, request: SignDraftRequest) -> Result<(), SignDraftRequest> {
        if self.awaiting_result {
            return Err(request);
        }

        let mut request = Some(request);
        let queued = critical_section::with(|cs| {
            let mut pending = SIGN_DRAFT_REQUEST.borrow_ref_mut(cs);
            if pending.is_some() || SIGN_DRAFT_RESULT.borrow_ref(cs).is_some() {
                false
            } else {
                *pending = request.take();
                true
            }
        });

        if queued {
            self.awaiting_result = true;
            Ok(())
        } else {
            Err(request.expect("request still available when not queued"))
        }
    }

    pub fn poll(&mut self) -> Option<SignDraftOutcome> {
        let outcome = critical_section::with(|cs| {
            let mut slot = SIGN_DRAFT_RESULT.borrow_ref_mut(cs);
            let outcome = slot.take();
            if outcome.is_some() {
                self.awaiting_result = false;
            }
            outcome
        });

        if let Some(result) = outcome {
            return Some(result);
        }

        None
    }
}

impl DirectSignController {
    pub fn new() -> Self {
        Self {
            awaiting_result: false,
        }
    }

    pub fn submit(&mut self, request: DirectSignRequest) -> Result<(), DirectSignRequest> {
        if self.awaiting_result {
            return Err(request);
        }

        let mut request = Some(request);
        let queued = critical_section::with(|cs| {
            let mut pending = DIRECT_SIGN_REQUEST.borrow_ref_mut(cs);
            if pending.is_some() || DIRECT_SIGN_RESULT.borrow_ref(cs).is_some() {
                false
            } else {
                *pending = request.take();
                true
            }
        });

        if queued {
            self.awaiting_result = true;
            Ok(())
        } else {
            Err(request.expect("request still available when not queued"))
        }
    }

    pub fn poll(&mut self) -> Option<DirectSignOutcome> {
        let outcome = critical_section::with(|cs| {
            let mut slot = DIRECT_SIGN_RESULT.borrow_ref_mut(cs);
            let outcome = slot.take();
            if outcome.is_some() {
                self.awaiting_result = false;
            }
            outcome
        });

        if let Some(result) = outcome {
            return Some(result);
        }

        None
    }
}

impl ChangePinController {
    pub fn new() -> Self {
        Self {
            awaiting_result: false,
        }
    }

    pub fn submit(&mut self, request: ChangePinRequest) -> Result<(), ChangePinRequest> {
        if self.awaiting_result {
            return Err(request);
        }

        let mut request = Some(request);
        let queued = critical_section::with(|cs| {
            let mut pending = CHANGE_PIN_REQUEST.borrow_ref_mut(cs);
            if pending.is_some() || CHANGE_PIN_RESULT.borrow_ref(cs).is_some() {
                false
            } else {
                *pending = request.take();
                true
            }
        });

        if queued {
            self.awaiting_result = true;
            Ok(())
        } else {
            Err(request.expect("request still available when not queued"))
        }
    }

    pub fn poll(&mut self) -> Option<ChangePinOutcome> {
        let outcome = critical_section::with(|cs| {
            let mut slot = CHANGE_PIN_RESULT.borrow_ref_mut(cs);
            let outcome = slot.take();
            if outcome.is_some() {
                self.awaiting_result = false;
            }
            outcome
        });

        if let Some(result) = outcome {
            return Some(result);
        }

        None
    }
}

fn take_unlock_request() -> Option<UnlockRequest> {
    critical_section::with(|cs| UNLOCK_REQUEST.borrow_ref_mut(cs).take())
}

fn store_unlock_result(outcome: UnlockOutcome) {
    critical_section::with(|cs| {
        *UNLOCK_RESULT.borrow_ref_mut(cs) = Some(outcome);
    });
}

fn take_initpin_request() -> Option<InitPinRequest> {
    critical_section::with(|cs| INITPIN_REQUEST.borrow_ref_mut(cs).take())
}

fn store_initpin_result(outcome: InitPinOutcome) {
    critical_section::with(|cs| {
        *INITPIN_RESULT.borrow_ref_mut(cs) = Some(outcome);
    });
}

fn take_seed_derive_request() -> Option<SeedDeriveRequest> {
    critical_section::with(|cs| SEED_DERIVE_REQUEST.borrow_ref_mut(cs).take())
}

fn store_seed_derive_result(outcome: SeedDeriveOutcome) {
    critical_section::with(|cs| {
        *SEED_DERIVE_RESULT.borrow_ref_mut(cs) = Some(outcome);
    });
}

fn take_sign_draft_request() -> Option<SignDraftRequest> {
    critical_section::with(|cs| SIGN_DRAFT_REQUEST.borrow_ref_mut(cs).take())
}

fn store_sign_draft_result(outcome: SignDraftOutcome) {
    critical_section::with(|cs| {
        *SIGN_DRAFT_RESULT.borrow_ref_mut(cs) = Some(outcome);
    });
}

fn take_direct_sign_request() -> Option<DirectSignRequest> {
    critical_section::with(|cs| DIRECT_SIGN_REQUEST.borrow_ref_mut(cs).take())
}

fn store_direct_sign_result(outcome: DirectSignOutcome) {
    critical_section::with(|cs| {
        *DIRECT_SIGN_RESULT.borrow_ref_mut(cs) = Some(outcome);
    });
}

fn take_change_pin_request() -> Option<ChangePinRequest> {
    critical_section::with(|cs| CHANGE_PIN_REQUEST.borrow_ref_mut(cs).take())
}

fn store_change_pin_result(outcome: ChangePinOutcome) {
    critical_section::with(|cs| {
        *CHANGE_PIN_RESULT.borrow_ref_mut(cs) = Some(outcome);
    });
}

#[esp_hal::ram]
fn park_while_flash_paused() -> bool {
    if !worker_flash_pause_requested() {
        worker_flash_pause_set_parked(false);
        return false;
    }

    worker_flash_pause_set_parked(true);
    while worker_flash_pause_requested() {
        core::hint::spin_loop();
    }
    worker_flash_pause_set_parked(false);
    true
}

pub fn worker_loop<S, CU, CI, CSDR, CSD, CDS, CCP>(
    state: &mut S,
    mut compute_unlock: CU,
    mut compute_initpin: CI,
    mut compute_seed_derive: CSDR,
    mut compute_sign_draft: CSD,
    mut compute_direct_sign: CDS,
    mut compute_change_pin: CCP,
) -> !
where
    CU: FnMut(&mut S, &str) -> UnlockOutcome,
    CI: FnMut(&mut S, InitPinRequest) -> InitPinOutcome,
    CSDR: FnMut(&mut S, SeedDeriveRequest) -> SeedDeriveOutcome,
    CSD: FnMut(&mut S, SignDraftRequest) -> SignDraftOutcome,
    CDS: FnMut(&mut S, DirectSignRequest) -> DirectSignOutcome,
    CCP: FnMut(&mut S, ChangePinRequest) -> ChangePinOutcome,
{
    let delay = Delay::new();
    worker_flash_pause_set_online(true);
    loop {
        if park_while_flash_paused() {
            continue;
        }

        if let Some(request) = take_initpin_request() {
            let outcome = compute_initpin(state, request);
            store_initpin_result(outcome);
        } else if let Some(request) = take_seed_derive_request() {
            let outcome = compute_seed_derive(state, request);
            store_seed_derive_result(outcome);
        } else if let Some(request) = take_unlock_request() {
            let outcome = compute_unlock(state, request.pin.as_str());
            store_unlock_result(outcome);
        } else if let Some(request) = take_sign_draft_request() {
            let outcome = compute_sign_draft(state, request);
            store_sign_draft_result(outcome);
        } else if let Some(request) = take_direct_sign_request() {
            let outcome = compute_direct_sign(state, request);
            store_direct_sign_result(outcome);
        } else if let Some(request) = take_change_pin_request() {
            let outcome = compute_change_pin(state, request);
            store_change_pin_result(outcome);
        } else {
            delay.delay_millis(5);
        }
    }
}
