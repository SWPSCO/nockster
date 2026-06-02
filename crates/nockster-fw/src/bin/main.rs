#![no_std]
#![no_main]
#![deny(clippy::mem_forget, reason = "unsafe for esp-hal types")]
mod dispatch;
mod fragments;
mod gui;
mod jobs;
mod nvs_pepper;
mod protocol;
mod random;
mod seed_store;
mod session;
mod signing;
mod static_slot;
mod update_auth;
mod usb_hid;
use nockster_fw::nvs_store::{NvsError, NvsInitStage, NvsStore};
use panic_halt as _;
extern crate alloc;
use alloc::vec::Vec;
use cobs::encode;
use core::cell::RefCell;
use core::fmt::Write as _;
use critical_section::Mutex;
use dispatch::{frame_confirmation_prompt, update_mode_allows_frame};
use esp_hal::otg_fs::{Usb, UsbBus as OtgUsbBus};
use esp_hal::rng::Trng;
use esp_hal::system::{software_reset, CpuControl, Stack};
use esp_hal::time::Duration;
use esp_hal::{clock::CpuClock, delay::Delay, main};
use gui::{
    Gui, GuiInteraction, SeedInteraction, TxReviewSummary, TX_REVIEW_FLAG_HIGH_FEE,
    TX_REVIEW_FLAG_MULTIPLE_RECIPIENTS, TX_REVIEW_FLAG_NO_REFUND,
};
use heapless::{String as HString, Vec as HVec};
use jobs::{
    ChangePinController, ChangePinOutcome, ChangePinRequest, DirectSignController,
    DirectSignOutcome, DirectSignRequest, InitPinController, InitPinOutcome, InitPinRequest,
    SeedOp, SeedOpOutcome, SeedOpRequest, SignDraftController, SignDraftOutcome, SignDraftRequest,
    UnlockController, UnlockOutcome,
};
use nockster_core::*;
use protocol::{send_err, send_msg, send_response};
use seed_store::{
    clear_master_key, master_key_copy, root_pub_from_seed, set_seed, store_master_key,
    update_seed_store_from_slice, wipe_seed, PendingSeedSetup, SeedOpUiEffect,
};
use static_slot::StaticSlot;
use usb_device::prelude::{LangID, StringDescriptors, UsbDeviceBuilder, UsbVidPid};
use usb_hid::{debug as usb_debug, service_tx as usb_service_tx, write as usb_write};
use usbd_hid::hid_class::HIDClass;
use zeroize::Zeroize;
const FW_MAJOR: u16 = 0;
const FW_MINOR: u16 = 1;
const APP_CORE_STACK_SIZE: usize = 64 * 1024;
const APP_HEAP_SIZE: usize = 96 * 1024;
const BOOT_LOGO_HOLD_MS: u32 = 900;
const FIRMWARE_UPDATE_FEATURES: u32 = if valid_update_anchor_env() {
    FEATURE_SECURE_UPDATE
} else {
    0
};
const FIRMWARE_FEATURES: u32 = FEATURE_CHEETAH
    | FEATURE_FRAG
    | FEATURE_XPUB
    | FEATURE_SECURITY_STATUS
    | FEATURE_BUILD_INFO
    | FEATURE_TOUCH_CALIBRATION
    | FEATURE_TOUCH_DIAGNOSTICS
    | FEATURE_SEED_LABELS
    | FEATURE_PIN_CHANGE_UI
    | FEATURE_TOUCH_CALIBRATION_UI
    | FEATURE_RELEASE_INFO
    | FEATURE_UPDATE_BOOT_STATUS
    | FEATURE_DEVICE_REBOOT
    | FEATURE_DEVICE_ADDRESS_BOOK
    | FIRMWARE_UPDATE_FEATURES;

const fn valid_update_anchor_env() -> bool {
    match option_env!("NOCKSTER_UPDATE_PUBKEY_SHA256_HEX") {
        Some(hex) => valid_hex_32(hex.as_bytes()),
        None => false,
    }
}

const fn valid_hex_32(bytes: &[u8]) -> bool {
    if bytes.len() != 64 {
        return false;
    }

    let mut i = 0usize;
    while i < bytes.len() {
        let byte = bytes[i];
        if !((byte >= b'0' && byte <= b'9')
            || (byte >= b'a' && byte <= b'f')
            || (byte >= b'A' && byte <= b'F'))
        {
            return false;
        }
        i += 1;
    }
    true
}
static USB_EP_MEMORY: StaticSlot<[u32; 1024]> = StaticSlot::new([0; 1024]);
static APP_CORE_STACK: StaticSlot<Stack<APP_CORE_STACK_SIZE>> = StaticSlot::new(Stack::new());
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(clippy::declare_interior_mutable_const)]
static DEVICE_BUSY: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));
#[allow(clippy::declare_interior_mutable_const)]
static PENDING_USB_UNLOCK_ID: Mutex<RefCell<Option<u32>>> = Mutex::new(RefCell::new(None));
#[allow(clippy::declare_interior_mutable_const)]
static PENDING_USB_INITPIN_ID: Mutex<RefCell<Option<u32>>> = Mutex::new(RefCell::new(None));
#[allow(clippy::declare_interior_mutable_const)]
static INITPIN_PROGRESS: Mutex<RefCell<u8>> = Mutex::new(RefCell::new(InitPinProgress::Idle as u8));
#[allow(clippy::declare_interior_mutable_const)]
static PENDING_SOFT_REBOOT: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum InitPinProgress {
    Idle = 0,
    Queued = 1,
    Worker = 2,
    CheckNvs = 3,
    RootPub = 4,
    NvsReadHeader = 5,
    NvsWipePartial = 6,
    NvsRandomSalt = 7,
    NvsPepper = 8,
    NvsKdf = 9,
    NvsKdfDone = 10,
    NvsEncryptSeed = 11,
    NvsWriteHeaderPending = 12,
    NvsWriteSlot = 13,
    NvsWriteHeaderFinal = 14,
    NvsWriteLabels = 15,
    NvsWriteFlash = 16,
    Complete = 17,
    Error = 18,
}

impl InitPinProgress {
    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Queued,
            2 => Self::Worker,
            3 => Self::CheckNvs,
            4 => Self::RootPub,
            5 => Self::NvsReadHeader,
            6 => Self::NvsWipePartial,
            7 => Self::NvsRandomSalt,
            8 => Self::NvsPepper,
            9 => Self::NvsKdf,
            10 => Self::NvsKdfDone,
            11 => Self::NvsEncryptSeed,
            12 => Self::NvsWriteHeaderPending,
            13 => Self::NvsWriteSlot,
            14 => Self::NvsWriteHeaderFinal,
            15 => Self::NvsWriteLabels,
            16 => Self::NvsWriteFlash,
            17 => Self::Complete,
            18 => Self::Error,
            _ => Self::Idle,
        }
    }

    fn label(self) -> Option<&'static str> {
        match self {
            Self::Idle => None,
            Self::Queued => Some("Seed: queued"),
            Self::Worker => Some("Seed: worker"),
            Self::CheckNvs => Some("Seed: check"),
            Self::RootPub => Some("Seed: root pub"),
            Self::NvsReadHeader => Some("Seed: read NVS"),
            Self::NvsWipePartial => Some("Seed: wipe partial"),
            Self::NvsRandomSalt => Some("Seed: salt"),
            Self::NvsPepper => Some("Seed: pepper"),
            Self::NvsKdf => Some("Seed: KDF"),
            Self::NvsKdfDone => Some("Seed: KDF done"),
            Self::NvsEncryptSeed => Some("Seed: encrypt"),
            Self::NvsWriteHeaderPending => Some("Seed: hdr 1"),
            Self::NvsWriteSlot => Some("Seed: slot"),
            Self::NvsWriteHeaderFinal => Some("Seed: hdr 2"),
            Self::NvsWriteLabels => Some("Seed: labels"),
            Self::NvsWriteFlash => Some("Seed: flash"),
            Self::Complete => Some("Seed: done"),
            Self::Error => Some("Seed: error"),
        }
    }
}

fn set_initpin_progress(progress: InitPinProgress) {
    critical_section::with(|cs| {
        *INITPIN_PROGRESS.borrow_ref_mut(cs) = progress as u8;
    });
}

fn get_initpin_progress() -> InitPinProgress {
    critical_section::with(|cs| InitPinProgress::from_u8(*INITPIN_PROGRESS.borrow_ref(cs)))
}

#[inline]
fn request_soft_reboot() {
    critical_section::with(|cs| {
        *PENDING_SOFT_REBOOT.borrow_ref_mut(cs) = true;
    });
}

#[inline]
fn soft_reboot_requested() -> bool {
    critical_section::with(|cs| *PENDING_SOFT_REBOOT.borrow_ref(cs))
}

fn nvs_init_progress(stage: NvsInitStage) {
    let progress = match stage {
        NvsInitStage::ReadHeader => InitPinProgress::NvsReadHeader,
        NvsInitStage::WipePartial => InitPinProgress::NvsWipePartial,
        NvsInitStage::RandomSalt => InitPinProgress::NvsRandomSalt,
        NvsInitStage::Pepper => InitPinProgress::NvsPepper,
        NvsInitStage::Kdf => InitPinProgress::NvsKdf,
        NvsInitStage::KdfDone => InitPinProgress::NvsKdfDone,
        NvsInitStage::EncryptSeed => InitPinProgress::NvsEncryptSeed,
        NvsInitStage::WriteHeaderPending => InitPinProgress::NvsWriteHeaderPending,
        NvsInitStage::WriteSlot => InitPinProgress::NvsWriteSlot,
        NvsInitStage::WriteHeaderFinal => InitPinProgress::NvsWriteHeaderFinal,
        NvsInitStage::WriteLabels => InitPinProgress::NvsWriteLabels,
        NvsInitStage::WriteFlash => InitPinProgress::NvsWriteFlash,
        NvsInitStage::Complete => InitPinProgress::Complete,
    };
    set_initpin_progress(progress);
    Delay::new().delay_millis(80);
}

#[inline]
fn device_busy() -> bool {
    critical_section::with(|cs| *DEVICE_BUSY.borrow_ref(cs))
}

#[inline]
fn set_device_busy(value: bool) {
    critical_section::with(|cs| {
        *DEVICE_BUSY.borrow_ref_mut(cs) = value;
    });
}

#[inline]
fn set_pending_usb_unlock_id(id: u32) {
    critical_section::with(|cs| {
        *PENDING_USB_UNLOCK_ID.borrow_ref_mut(cs) = Some(id);
    });
}

#[inline]
fn take_pending_usb_unlock_id() -> Option<u32> {
    critical_section::with(|cs| PENDING_USB_UNLOCK_ID.borrow_ref_mut(cs).take())
}

#[inline]
fn set_pending_usb_initpin_id(id: u32) {
    critical_section::with(|cs| {
        *PENDING_USB_INITPIN_ID.borrow_ref_mut(cs) = Some(id);
    });
}

#[inline]
fn take_pending_usb_initpin_id() -> Option<u32> {
    critical_section::with(|cs| PENDING_USB_INITPIN_ID.borrow_ref_mut(cs).take())
}

fn return_response_msg(id: u32, msg: Response) -> Option<Msg<Response>> {
    Some(Msg {
        v: PROTO_V1,
        id,
        msg,
    })
}

struct PendingConfirmation {
    msg_id: u32,
    frame: Frame,
}

struct PendingSignDraft {
    msg_id: u32,
    frag_id: u16,
    sk_be: [u8; 32],
    draft: Vec<u8>,
}

enum PendingPinChange {
    New {
        msg_id: u32,
        current_pin: HString<16>,
    },
    Confirm {
        msg_id: u32,
        current_pin: HString<16>,
        new_pin: HString<16>,
    },
}

#[allow(clippy::declare_interior_mutable_const)]
static PENDING_CONFIRMATION: Mutex<RefCell<Option<PendingConfirmation>>> =
    Mutex::new(RefCell::new(None));

#[allow(clippy::declare_interior_mutable_const)]
static PENDING_SIGN_DRAFT: Mutex<RefCell<Option<PendingSignDraft>>> =
    Mutex::new(RefCell::new(None));

struct AppCoreState<'d> {
    nvs_pepper: nvs_pepper::AppNvsPepper<'d>,
}

fn compute_initpin_outcome(
    state: &mut AppCoreState<'_>,
    request: InitPinRequest,
) -> InitPinOutcome {
    let InitPinRequest { pin, mut seed64 } = request;
    set_initpin_progress(InitPinProgress::Worker);
    let mut nvs = NvsStore::new();
    set_initpin_progress(InitPinProgress::CheckNvs);
    if nvs.is_initialized() {
        seed64.zeroize();
        set_initpin_progress(InitPinProgress::Complete);
        return InitPinOutcome::AlreadyInitialized;
    }

    set_initpin_progress(InitPinProgress::RootPub);
    let pub_xy = root_pub_from_seed(&seed64);
    match nvs.prepare_initialize_pin_with_pepper_and_progress(
        pin.as_str(),
        &seed64,
        pub_xy,
        &mut state.nvs_pepper,
        nvs_init_progress,
    ) {
        Ok((prepared, master_key, _slot)) => {
            set_initpin_progress(InitPinProgress::NvsWriteFlash);
            InitPinOutcome::Prepared {
                seed64,
                master_key,
                prepared,
            }
        }
        Err(NvsError::AlreadyInitialized) => {
            seed64.zeroize();
            set_initpin_progress(InitPinProgress::Complete);
            InitPinOutcome::AlreadyInitialized
        }
        Err(NvsError::Flash) => {
            seed64.zeroize();
            set_initpin_progress(InitPinProgress::Error);
            InitPinOutcome::Flash
        }
        Err(NvsError::Crypto) => {
            seed64.zeroize();
            set_initpin_progress(InitPinProgress::Error);
            InitPinOutcome::Crypto
        }
        Err(_) => {
            seed64.zeroize();
            set_initpin_progress(InitPinProgress::Error);
            InitPinOutcome::Failed
        }
    }
}

fn compute_change_pin_outcome(
    state: &mut AppCoreState<'_>,
    request: ChangePinRequest,
) -> ChangePinOutcome {
    let ChangePinRequest {
        msg_id,
        current_pin,
        new_pin,
    } = request;
    let mut nvs = NvsStore::new();
    match nvs.prepare_change_pin_with_pepper(
        current_pin.as_str(),
        new_pin.as_str(),
        &mut state.nvs_pepper,
    ) {
        Ok((prepared, seeds, master_key)) => ChangePinOutcome::Prepared {
            msg_id,
            seeds,
            master_key,
            prepared,
        },
        Err(NvsError::WrongPin) => ChangePinOutcome::WrongPin { msg_id },
        Err(NvsError::LockedOut) => ChangePinOutcome::LockedOut { msg_id },
        Err(NvsError::Flash) => ChangePinOutcome::Flash { msg_id },
        Err(NvsError::Crypto) => ChangePinOutcome::Crypto { msg_id },
        Err(_) => ChangePinOutcome::Failed { msg_id },
    }
}

fn compute_sign_draft_outcome(
    _state: &mut AppCoreState<'_>,
    mut request: SignDraftRequest,
) -> SignDraftOutcome {
    let cfg = nockster_core::draft_sign::SignerConfig {
        sk_be: request.sk_be,
    };
    let outcome = match nockster_core::draft_sign::sign_draft_v1(request.draft.as_slice(), &cfg) {
        Ok(out) => SignDraftOutcome::Success {
            msg_id: request.msg_id,
            frag_id: request.frag_id,
            out,
        },
        Err(nockster_core::draft_sign::SignDraftError::Unsupported) => {
            SignDraftOutcome::Unsupported {
                msg_id: request.msg_id,
            }
        }
        Err(_) => SignDraftOutcome::Failed {
            msg_id: request.msg_id,
        },
    };
    request.sk_be.zeroize();
    request.draft.zeroize();
    outcome
}

fn is_direct_sign_frame(frame: &Frame) -> bool {
    matches!(
        frame,
        Frame::One(
            Request::SignDigest { .. }
                | Request::SignSpendHash { .. }
                | Request::SignSpendHashFor { .. }
        )
    )
}

fn compute_direct_sign_outcome(
    _state: &mut AppCoreState<'_>,
    request: DirectSignRequest,
) -> DirectSignOutcome {
    let response = match &request.frame {
        Frame::One(req) if is_direct_sign_frame(&request.frame) => {
            signing::handle_request(req, is_device_locked()).unwrap_or(Response::Err {
                code: ERR_UNSUPPORTED_VERSION,
            })
        }
        _ => Response::Err {
            code: ERR_UNSUPPORTED_VERSION,
        },
    };

    DirectSignOutcome {
        msg_id: request.msg_id,
        response,
    }
}

fn tx_review_summary_from_draft(
    review: &nockster_core::draft_sign::DraftReviewV1,
) -> TxReviewSummary {
    let mut flags = 0u8;
    if review.fee_total > review.minimum_fee {
        flags |= TX_REVIEW_FLAG_HIGH_FEE;
    }
    if review.external_total != 0 && review.refund_total == 0 {
        flags |= TX_REVIEW_FLAG_NO_REFUND;
    }
    if review.external_output_count > 1 {
        flags |= TX_REVIEW_FLAG_MULTIPLE_RECIPIENTS;
    }

    TxReviewSummary {
        input_count: review.input_count,
        external_output_count: review.external_output_count,
        external_total: review.external_total,
        refund_total: review.refund_total,
        fee_total: review.fee_total,
        flags,
    }
}

fn begin_confirmation(
    msg_id: u32,
    frame: Frame,
    prompt: &'static str,
    ui: Option<&mut Gui<'_>>,
) -> Result<(), u16> {
    let Some(ui) = ui else {
        return Err(ERR_UNSUPPORTED_VERSION);
    };

    let mut spend_outputs: HVec<(u64, HString<64>), 24> = HVec::new();
    let mut details = HString::<48>::new();
    let mut tx_review_header = prompt;

    if let Frame::One(Request::SignSpendHashFor {
        slot, path, pubkey, ..
    }) = &frame
    {
        signing::preflight_spend_pubkey(*slot, path, pubkey, is_device_locked())?;
        let _ = details.push_str("Pubkey OK");
        tx_review_header = "Pubkey OK";
    }
    if let Frame::One(Request::DeleteSeed { slot }) = &frame {
        if is_device_locked() {
            return Err(ERR_DEVICE_LOCKED);
        }
        let _ = core::write!(&mut details, "slot {}", slot);
    }
    if let Frame::One(Request::Reset) = &frame {
        let _ = details.push_str("Erase all seeds");
    }

    let show_spend_outputs = match &frame {
        Frame::One(Request::SignSpendHash {
            meta: Some(meta), ..
        })
        | Frame::One(Request::SignSpendHashFor {
            meta: Some(meta), ..
        }) => {
            for out in meta.outputs.iter() {
                if out.gift == 0 || out.is_refund {
                    continue;
                }
                let mut recipient = HString::<64>::new();
                let max = 64usize;
                let take = out.recipient_pkh_b58.len().min(max);
                let _ = recipient.push_str(&out.recipient_pkh_b58[..take]);
                let candidate = (out.gift, recipient);
                if let Err(candidate) = spend_outputs.push(candidate) {
                    // Keep only the largest outputs when more than the UI list can display.
                    let mut min_idx = 0usize;
                    let mut min_gift = spend_outputs[0].0;
                    for (idx, (gift, _)) in spend_outputs.iter().enumerate().skip(1) {
                        if *gift < min_gift {
                            min_gift = *gift;
                            min_idx = idx;
                        }
                    }
                    if candidate.0 > min_gift {
                        spend_outputs[min_idx] = candidate;
                    }
                }
            }
            spend_outputs
                .as_mut_slice()
                .sort_unstable_by(|a, b| b.0.cmp(&a.0));
            true
        }
        _ => false,
    };

    let stored = critical_section::with(|cs| {
        let mut pending = PENDING_CONFIRMATION.borrow_ref_mut(cs);
        if pending.is_some() {
            false
        } else {
            pending.replace(PendingConfirmation { msg_id, frame });
            true
        }
    });

    if !stored {
        return Err(ERR_BUSY);
    }

    if show_spend_outputs {
        ui.request_tx_review_with_header(
            tx_review_header,
            spend_outputs
                .iter()
                .map(|(gift, recipient)| (*gift, recipient.as_str())),
        );
    } else {
        let details = if details.is_empty() {
            None
        } else {
            Some(details.as_str())
        };
        ui.request_confirmation_with_details(prompt, details);
    }
    set_device_busy(true);

    Ok(())
}

enum AddressBookFragmentResult {
    NotAddressBook,
    Immediate(Response),
}

fn maybe_handle_address_book_fragment(
    frame: &Frame,
    ui: Option<&mut Gui<'_>>,
) -> AddressBookFragmentResult {
    let Frame::FragPart {
        id,
        offset,
        chunk,
        last,
    } = frame
    else {
        return AddressBookFragmentResult::NotAddressBook;
    };

    let Some(mut st) = fragments::take_inbound() else {
        return AddressBookFragmentResult::NotAddressBook;
    };
    if st.kind != FragKind::AddressBook {
        fragments::put_inbound(st);
        return AddressBookFragmentResult::NotAddressBook;
    }
    if st.id != *id || st.next_off != *offset {
        fragments::put_inbound(st);
        return AddressBookFragmentResult::Immediate(Response::Err {
            code: ERR_BAD_COBS_OR_POSTCARD,
        });
    }
    if st.buf.len() + chunk.len() > (st.total_len as usize) {
        fragments::put_inbound(st);
        return AddressBookFragmentResult::Immediate(Response::Err { code: ERR_OVERFLOW });
    }

    st.buf.extend_from_slice(chunk.as_slice());
    st.next_off += chunk.len() as u32;
    if !*last {
        fragments::put_inbound(st);
        return AddressBookFragmentResult::Immediate(Response::Ok);
    }
    if st.next_off != st.total_len {
        return AddressBookFragmentResult::Immediate(Response::Err {
            code: ERR_BAD_COBS_OR_POSTCARD,
        });
    }
    if is_device_locked() {
        return AddressBookFragmentResult::Immediate(Response::Err {
            code: ERR_DEVICE_LOCKED,
        });
    }

    if let Some(ui) = ui {
        ui.show_idle_message_timed("Saving address", Duration::from_millis(1_500));
    }
    AddressBookFragmentResult::Immediate(dispatch::write_address_book_payload(
        st.buf.as_slice(),
        is_device_locked(),
    ))
}

fn take_pending_confirmation() -> Option<PendingConfirmation> {
    critical_section::with(|cs| {
        let mut slot = PENDING_CONFIRMATION.borrow_ref_mut(cs);
        slot.take()
    })
}

fn take_pending_sign_draft() -> Option<PendingSignDraft> {
    critical_section::with(|cs| {
        let mut slot = PENDING_SIGN_DRAFT.borrow_ref_mut(cs);
        slot.take()
    })
}

/// cobs test
#[cfg(test)]
pub fn handle_one_frame_cobs(frame: &[u8]) -> alloc::vec::Vec<u8> {
    dispatch::handle_one_frame_cobs_with(frame, |id, msg| handle_frame_v1(id, msg, None))
}
#[main]
fn main() -> ! {
    let cfg = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let p = esp_hal::init(cfg);
    esp_alloc::heap_allocator!(size: APP_HEAP_SIZE);
    let _trng = Trng::new(p.RNG, p.ADC1);
    let mut delay = Delay::new();
    let mut ui = Gui::new(
        p.SPI2, p.GPIO38, p.GPIO39, p.GPIO45, p.GPIO21, p.GPIO40, p.GPIO46, p.I2C0, p.GPIO41,
        p.GPIO42, p.GPIO47, p.GPIO48, &mut delay,
    )
    .ok();
    if ui.is_some() {
        delay.delay_millis(BOOT_LOGO_HOLD_MS);
    }
    let pin_required = {
        let mut nvs = NvsStore::new();
        nvs.is_initialized()
    };
    if pin_required {
        if let Some(ui) = ui.as_mut() {
            ui.begin_unlock(None);
        }
    } else {
        // No seed/PIN configured - show seed setup screen
        if let Some(ui) = ui.as_mut() {
            ui.show_seed_setup();
        }
    }
    #[cfg(feature = "chip-security")]
    let mut app_core_state = AppCoreState {
        nvs_pepper: nvs_pepper::AppNvsPepper::new(p.HMAC),
    };
    #[cfg(not(feature = "chip-security"))]
    let mut app_core_state = AppCoreState {
        nvs_pepper: nvs_pepper::AppNvsPepper::new(),
    };
    let mut cpu_control = CpuControl::new(p.CPU_CTRL);
    let _app_core_guard = cpu_control
        .start_app_core(unsafe { APP_CORE_STACK.as_mut() }, move || {
            jobs::worker_loop(
                &mut app_core_state,
                compute_unlock_outcome,
                compute_initpin_outcome,
                compute_sign_draft_outcome,
                compute_direct_sign_outcome,
                compute_change_pin_outcome,
            )
        })
        .expect("failed to start app core");

    let mut unlock_controller = UnlockController::new();
    let mut initpin_controller = InitPinController::new();
    let mut sign_draft_controller = SignDraftController::new();
    let mut direct_sign_controller = DirectSignController::new();
    let mut change_pin_controller = ChangePinController::new();
    let mut pending_seed_setup = PendingSeedSetup::new();
    let mut pending_seed_pin: Option<HString<16>> = None;
    let mut pending_pin_change: Option<PendingPinChange> = None;
    let mut pending_touch_calibration_id: Option<u32> = None;

    // USB-OTG + HID (WebHID-friendly).
    let usb = Usb::new(p.USB0, p.GPIO20, p.GPIO19);
    let usb_bus = OtgUsbBus::new(usb, unsafe { USB_EP_MEMORY.as_mut() });
    let mut hid = HIDClass::new(&usb_bus, usb_hid::REPORT_DESCRIPTOR, 10);
    let strings = [StringDescriptors::new(LangID::EN_US)
        .manufacturer("SWPSCo")
        .product("Nockster")
        .serial_number("nock001")];
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x303a, 0x2001))
        .strings(&strings)
        .expect("usb strings")
        .build();
    update_auth::mark_running_image_valid();

    // Bigger working buffers to accommodate TX_CHUNK
    let mut rx: HVec<u8, 512> = HVec::new();
    let mut plain = [0u8; usb_hid::PLAIN_BUF_LEN];
    let mut enc = [0u8; usb_hid::ENC_BUF_LEN];
    let mut outbound_chunk_buf = Vec::with_capacity(fragments::TX_CHUNK);
    let mut displayed_initpin_progress = InitPinProgress::Idle;
    'main: loop {
        usb_dev.poll(&mut [&mut hid]);
        usb_service_tx(&mut hid);
        if soft_reboot_requested() && usb_hid::tx_idle() {
            delay.delay_millis(150u32);
            software_reset();
        }
        if let Some(ui) = ui.as_mut() {
            let event = ui.tick();
            if let Some(decision) = ui.poll_confirmation_result() {
                if let Some(pending) = take_pending_confirmation() {
                    let resp_msg = if decision {
                        let approved_label = match &pending.frame {
                            Frame::One(Request::SignDigest { .. })
                            | Frame::One(Request::SignSpendHash { .. })
                            | Frame::One(Request::SignSpendHashFor { .. }) => "Signing...",
                            _ => "Approved",
                        };
                        ui.show_idle_message_timed(approved_label, Duration::from_millis(3_000));
                        if let Frame::One(Request::DeleteSeed { slot }) = &pending.frame {
                            if let Some(master_key) = master_key_copy() {
                                let request = SeedOpRequest {
                                    msg_id: pending.msg_id,
                                    op: SeedOp::Delete {
                                        slot: *slot,
                                        master_key,
                                    },
                                };
                                let outcome = seed_store::compute_seed_op_outcome(request);
                                let (msg_id, response) =
                                    handle_seed_op_outcome(outcome, Some(&mut *ui), &mut hid);
                                return_response_msg(msg_id, response)
                            } else {
                                return_response_msg(
                                    pending.msg_id,
                                    Response::Err {
                                        code: ERR_DEVICE_LOCKED,
                                    },
                                )
                            }
                        } else if matches!(&pending.frame, Frame::One(Request::Reset)) {
                            let request = SeedOpRequest {
                                msg_id: pending.msg_id,
                                op: SeedOp::Reset,
                            };
                            let outcome = seed_store::compute_seed_op_outcome(request);
                            wipe_seed();
                            clear_master_key();
                            let (msg_id, response) =
                                handle_seed_op_outcome(outcome, Some(&mut *ui), &mut hid);
                            return_response_msg(msg_id, response)
                        } else if is_direct_sign_frame(&pending.frame) {
                            let request = DirectSignRequest {
                                msg_id: pending.msg_id,
                                frame: pending.frame,
                            };
                            match direct_sign_controller.submit(request) {
                                Ok(()) => {
                                    set_device_busy(true);
                                    None
                                }
                                Err(request) => {
                                    set_device_busy(false);
                                    return_response_msg(
                                        request.msg_id,
                                        Response::Err { code: ERR_BUSY },
                                    )
                                }
                            }
                        } else {
                            let body = handle_frame_v1(pending.msg_id, &pending.frame, None);
                            return_response_msg(pending.msg_id, body)
                        }
                    } else {
                        ui.show_idle_message_timed("Rejected", Duration::from_millis(3_000));
                        return_response_msg(
                            pending.msg_id,
                            Response::Err {
                                code: ERR_REJECTED_BY_USER,
                            },
                        )
                    };
                    let response_was_immediate = resp_msg.is_some();
                    if let Some(resp_msg) = resp_msg {
                        if send_msg(&mut hid, &resp_msg, &mut plain, &mut enc).is_err() {
                            send_err(&mut hid, ERR_ENCODE_TOO_BIG, &mut enc);
                        }
                    }
                    if decision {
                        usb_debug(&mut hid, b"confirm accepted\r\n");
                    } else {
                        usb_debug(&mut hid, b"confirm rejected\r\n");
                    }
                    if response_was_immediate {
                        set_device_busy(false);
                    }
                } else if let Some(pending) = take_pending_sign_draft() {
                    let resp_msg = if decision {
                        ui.show_idle_message_timed("Signing...", Duration::from_millis(3_000));
                        let request = SignDraftRequest {
                            msg_id: pending.msg_id,
                            frag_id: pending.frag_id,
                            sk_be: pending.sk_be,
                            draft: pending.draft,
                        };
                        match sign_draft_controller.submit(request) {
                            Ok(()) => {
                                set_device_busy(true);
                                None
                            }
                            Err(mut request) => {
                                request.sk_be.zeroize();
                                request.draft.zeroize();
                                set_device_busy(false);
                                Some(Msg {
                                    v: PROTO_V1,
                                    id: request.msg_id,
                                    msg: Response::Err { code: ERR_BUSY },
                                })
                            }
                        }
                    } else {
                        ui.show_idle_message_timed("Rejected", Duration::from_millis(3_000));
                        let PendingSignDraft {
                            msg_id,
                            mut sk_be,
                            mut draft,
                            ..
                        } = pending;
                        sk_be.zeroize();
                        draft.zeroize();
                        set_device_busy(false);
                        Some(Msg {
                            v: PROTO_V1,
                            id: msg_id,
                            msg: Response::Err {
                                code: ERR_REJECTED_BY_USER,
                            },
                        })
                    };
                    if let Some(resp_msg) = resp_msg {
                        if send_msg(&mut hid, &resp_msg, &mut plain, &mut enc).is_err() {
                            send_err(&mut hid, ERR_ENCODE_TOO_BIG, &mut enc);
                        }
                    }
                }
            }
            if let Some(event) = event {
                match event {
                    GuiInteraction::PinComplete(digits) => {
                        let mut pin = HString::<16>::new();
                        for digit in digits.iter() {
                            let ch = char::from(b'0' + *digit);
                            let _ = pin.push(ch);
                        }
                        if let Some(pending) = pending_pin_change.take() {
                            match pending {
                                PendingPinChange::New {
                                    msg_id,
                                    current_pin,
                                } => {
                                    pending_pin_change = Some(PendingPinChange::Confirm {
                                        msg_id,
                                        current_pin,
                                        new_pin: pin,
                                    });
                                    ui.begin_pin_entry("Repeat PIN", None);
                                }
                                PendingPinChange::Confirm {
                                    msg_id,
                                    current_pin,
                                    new_pin,
                                } => {
                                    if new_pin.as_str() != pin.as_str() {
                                        set_device_busy(false);
                                        show_pin_change_failure(ui, "PIN mismatch");
                                        send_response(
                                            &mut hid,
                                            msg_id,
                                            Response::Err {
                                                code: ERR_PIN_MISMATCH,
                                            },
                                            &mut plain,
                                            &mut enc,
                                        );
                                    } else {
                                        let request = ChangePinRequest {
                                            msg_id,
                                            current_pin,
                                            new_pin,
                                        };
                                        match change_pin_controller.submit(request) {
                                            Ok(()) => {
                                                ui.show_unlocking();
                                            }
                                            Err(_) => {
                                                set_device_busy(false);
                                                show_pin_change_failure(ui, "Busy");
                                                send_response(
                                                    &mut hid,
                                                    msg_id,
                                                    Response::Err { code: ERR_BUSY },
                                                    &mut plain,
                                                    &mut enc,
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        } else if pending_seed_setup.has_seed() {
                            if let Some(first_pin) = pending_seed_pin.take() {
                                if first_pin.as_str() != pin.as_str() {
                                    ui.begin_pin_entry("PIN mismatch", Some(4));
                                } else if let Some(mut seed64) = pending_seed_setup.take() {
                                    match initpin_controller.submit(pin.as_str(), &seed64) {
                                        Ok(()) => {
                                            set_device_busy(true);
                                            ui.show_unlocking_stage("Seed: queued");
                                            seed64.zeroize();
                                        }
                                        Err(()) => {
                                            pending_seed_setup.store_from(&mut seed64);
                                            ui.show_pin_failure(None);
                                            usb_debug(&mut hid, b"init queue busy\r\n");
                                        }
                                    }
                                }
                            } else {
                                pending_seed_pin = Some(pin);
                                ui.begin_pin_entry("Repeat PIN", Some(4));
                            }
                        } else {
                            match unlock_controller.submit(pin.as_str()) {
                                Ok(()) => {
                                    set_device_busy(true);
                                    ui.show_unlocking();
                                }
                                Err(()) => {
                                    ui.show_pin_failure(None);
                                    usb_debug(&mut hid, b"unlock queue busy\r\n");
                                }
                            }
                        }
                    }
                    GuiInteraction::ConfirmAccepted => {
                        usb_debug(&mut hid, b"confirm accepted\r\n");
                    }
                    GuiInteraction::ConfirmRejected => {
                        usb_debug(&mut hid, b"confirm rejected\r\n");
                    }
                    GuiInteraction::LockRequested => {
                        wipe_seed();
                        ui.begin_unlock(None);
                        usb_debug(&mut hid, b"locked\r\n");
                    }
                    GuiInteraction::Seed(seed_interaction) => match seed_interaction {
                        SeedInteraction::EntryCompleted(phrase) => {
                            match seed_store::bip39_seed_from_words(
                                phrase.iter().map(|word| word.as_str()),
                            ) {
                                Ok(mut seed64) => {
                                    pending_seed_pin = None;
                                    pending_seed_setup.store_from(&mut seed64);
                                    ui.clear_seed_entry_state();
                                    ui.begin_pin_entry("Set PIN", Some(4));
                                }
                                Err(()) => {
                                    ui.show_seed_setup();
                                    usb_debug(&mut hid, b"seed derive failed\r\n");
                                }
                            }
                        }
                        SeedInteraction::EntryCancelled => {
                            pending_seed_pin = None;
                            pending_seed_setup.clear();
                        }
                        _ => {}
                    },
                    GuiInteraction::RawTouch(_coord) => {}
                    GuiInteraction::TouchCalibrationComplete(calibration) => {
                        let msg_id = pending_touch_calibration_id.take();
                        let response = dispatch::finish_touch_calibration(
                            calibration,
                            ui,
                            session::is_locked(),
                        );
                        if let Some(msg_id) = msg_id {
                            send_response(&mut hid, msg_id, response, &mut plain, &mut enc);
                        }
                        set_device_busy(false);
                    }
                }
            }
        }
        let current_initpin_progress = get_initpin_progress();
        if current_initpin_progress != displayed_initpin_progress {
            displayed_initpin_progress = current_initpin_progress;
            if let Some(label) = current_initpin_progress.label() {
                if let Some(ui) = ui.as_mut() {
                    ui.show_unlocking_stage(label);
                }
            }
        }

        if let Some(outcome) = unlock_controller.poll() {
            let resp = if let Some(ui_ref) = ui.as_mut() {
                handle_unlock_outcome(outcome, Some(ui_ref), &mut hid)
            } else {
                handle_unlock_outcome(outcome, None, &mut hid)
            };
            if let Some(id) = take_pending_usb_unlock_id() {
                send_response(&mut hid, id, resp, &mut plain, &mut enc);
            }
            set_device_busy(false);
        }
        if let Some(outcome) = initpin_controller.poll() {
            let resp = if let Some(ui_ref) = ui.as_mut() {
                handle_initpin_outcome(outcome, Some(ui_ref), &mut hid)
            } else {
                handle_initpin_outcome(outcome, None, &mut hid)
            };
            if let Some(id) = take_pending_usb_initpin_id() {
                send_response(&mut hid, id, resp, &mut plain, &mut enc);
            }
            set_device_busy(false);
            set_initpin_progress(InitPinProgress::Idle);
            displayed_initpin_progress = InitPinProgress::Idle;
        }
        if let Some(outcome) = sign_draft_controller.poll() {
            match outcome {
                SignDraftOutcome::Success {
                    msg_id,
                    frag_id,
                    out,
                } => {
                    let total = out.len() as u32;
                    fragments::set_outbound(msg_id, frag_id, out);
                    send_response(
                        &mut hid,
                        msg_id,
                        Response::FragBegin {
                            id: frag_id,
                            total_len: total,
                            kind: FragKind::SignDraft,
                        },
                        &mut plain,
                        &mut enc,
                    );
                }
                SignDraftOutcome::Unsupported { msg_id } => send_response(
                    &mut hid,
                    msg_id,
                    Response::Err {
                        code: ERR_UNSUPPORTED_VERSION,
                    },
                    &mut plain,
                    &mut enc,
                ),
                SignDraftOutcome::Failed { msg_id } => send_response(
                    &mut hid,
                    msg_id,
                    Response::Err {
                        code: ERR_BAD_COBS_OR_POSTCARD,
                    },
                    &mut plain,
                    &mut enc,
                ),
            }
            set_device_busy(false);
        }
        if let Some(outcome) = direct_sign_controller.poll() {
            let msg_id = outcome.msg_id;
            let response = outcome.response;
            let signed = matches!(
                &response,
                Response::OkSig { .. } | Response::OkCheetahSig { .. }
            );
            if let Some(ui) = ui.as_mut() {
                if signed {
                    ui.show_idle_message_timed("Signed", Duration::from_millis(3_000));
                } else {
                    ui.show_idle_message_timed("Sign failed", Duration::from_millis(3_000));
                }
            }
            send_response(&mut hid, msg_id, response, &mut plain, &mut enc);
            if signed {
                usb_debug(&mut hid, b"direct sign complete\r\n");
            } else {
                usb_debug(&mut hid, b"direct sign failed\r\n");
            }
            set_device_busy(false);
        }
        if let Some(outcome) = change_pin_controller.poll() {
            let (msg_id, response) = if let Some(ui_ref) = ui.as_mut() {
                handle_change_pin_outcome(outcome, Some(ui_ref), &mut hid)
            } else {
                handle_change_pin_outcome(outcome, None, &mut hid)
            };
            send_response(&mut hid, msg_id, response, &mut plain, &mut enc);
            set_device_busy(false);
        }
        // 1) Proactive outbound frag, if any
        if let Some(mut of) = fragments::take_outbound() {
            let start = of.off as usize;
            let end = core::cmp::min(start + fragments::TX_CHUNK, of.data.len());
            let last = end == of.data.len();
            let mut chunk = core::mem::take(&mut outbound_chunk_buf);
            chunk.clear();
            chunk.extend_from_slice(&of.data[start..end]);
            let resp = Msg {
                v: PROTO_V1,
                id: of.msg_id,
                msg: Response::FragPart {
                    id: of.id,
                    offset: of.off,
                    chunk,
                    last,
                },
            };
            let encoded_len = match postcard::to_slice(&resp, &mut plain) {
                Ok(used) => used.len(),
                Err(_) => {
                    if let Response::FragPart { chunk, .. } = resp.msg {
                        outbound_chunk_buf = chunk;
                    }
                    send_err(&mut hid, ERR_ENCODE_TOO_BIG, &mut enc);
                    continue;
                }
            };
            if let Response::FragPart { chunk, .. } = resp.msg {
                outbound_chunk_buf = chunk;
            }
            let n = encode(&plain[..encoded_len], &mut enc);
            enc[n] = 0;
            if !usb_write(&mut hid, &enc[..n + 1]) {
                fragments::put_outbound(of);
                continue;
            }
            of.off = end as u32;
            if !last {
                fragments::put_outbound(of);
            }
            // After sending a part, try to send again (or fall through to RX next loop)
            continue;
        }
        // 2) RX path
        let mut rx_reports_this_tick = 0usize;
        loop {
            if rx_reports_this_tick >= usb_hid::MAX_RX_REPORTS_PER_TICK {
                break;
            }
            let mut report = [0u8; usb_hid::REPORT_TOTAL_LEN];
            match hid.pull_raw_output(&mut report) {
                Ok(n) => {
                    rx_reports_this_tick += 1;
                    let Some(payload) = usb_hid::output_payload(&report, n) else {
                        continue;
                    };
                    for &b in payload.iter() {
                        if b == 0 && rx.is_empty() {
                            continue 'main;
                        }
                        if rx.push(b).is_err() {
                            rx.clear();
                            send_err(&mut hid, ERR_OVERFLOW, &mut enc);
                            continue 'main;
                        }
                        if b == 0 {
                            // decode Msg<Frame>
                            let resp_msg = match postcard::from_bytes_cobs::<Msg<Frame>>(
                                rx.as_mut(),
                            ) {
                                Ok(m) if m.v == PROTO_V1 => {
                                    if update_auth::stream_active()
                                        && !update_mode_allows_frame(&m.msg)
                                    {
                                        Some(Msg {
                                            v: PROTO_V1,
                                            id: m.id,
                                            msg: Response::Err { code: ERR_BUSY },
                                        })
                                    } else {
                                        // Check if device is busy with a long operation (PBKDF2, etc.)
                                        // Reject all requests except Ping/GetInfo to prevent queue buildup
                                        let is_blocking_request = device_busy();
                                        let is_ping_or_info = matches!(
                                            &m.msg,
                                            Frame::One(Request::Ping)
                                                | Frame::One(Request::GetInfo)
                                        );

                                        if is_blocking_request && !is_ping_or_info {
                                            Some(Msg {
                                                v: PROTO_V1,
                                                id: m.id,
                                                msg: Response::Err { code: ERR_BUSY },
                                            })
                                        } else if let Frame::One(Request::Unlock { pin }) = &m.msg {
                                            if let Some(ui) = ui.as_mut() {
                                                ui.show_unlocking();
                                            }
                                            match unlock_controller.submit(pin.as_str()) {
                                                Ok(()) => {
                                                    set_device_busy(true);
                                                    set_pending_usb_unlock_id(m.id);
                                                    None
                                                }
                                                Err(()) => Some(Msg {
                                                    v: PROTO_V1,
                                                    id: m.id,
                                                    msg: Response::Err { code: ERR_BUSY },
                                                }),
                                            }
                                        } else if let Frame::One(Request::InitializePIN {
                                            pin,
                                            seed64,
                                        }) = &m.msg
                                        {
                                            if let Some(ui) = ui.as_mut() {
                                                ui.show_unlocking();
                                            }
                                            match initpin_controller.submit(pin.as_str(), seed64) {
                                                Ok(()) => {
                                                    set_initpin_progress(InitPinProgress::Queued);
                                                    set_device_busy(true);
                                                    set_pending_usb_initpin_id(m.id);
                                                    None
                                                }
                                                Err(()) => Some(Msg {
                                                    v: PROTO_V1,
                                                    id: m.id,
                                                    msg: Response::Err { code: ERR_BUSY },
                                                }),
                                            }
                                        } else if let Frame::One(Request::AddSeed { seed64 }) =
                                            &m.msg
                                        {
                                            if is_device_locked() {
                                                Some(Msg {
                                                    v: PROTO_V1,
                                                    id: m.id,
                                                    msg: Response::Err {
                                                        code: ERR_DEVICE_LOCKED,
                                                    },
                                                })
                                            } else if let Some(master_key) = master_key_copy() {
                                                let request = SeedOpRequest {
                                                    msg_id: m.id,
                                                    op: SeedOp::Add {
                                                        seed64: *seed64,
                                                        master_key,
                                                    },
                                                };
                                                if let Some(ui) = ui.as_mut() {
                                                    ui.show_unlocking_stage("Seed: add");
                                                }
                                                let outcome =
                                                    seed_store::compute_seed_op_outcome(request);
                                                let (msg_id, body) = {
                                                    let ui_ref = ui.as_mut().map(|u| u as &mut Gui);
                                                    handle_seed_op_outcome(
                                                        outcome, ui_ref, &mut hid,
                                                    )
                                                };
                                                Some(Msg {
                                                    v: PROTO_V1,
                                                    id: msg_id,
                                                    msg: body,
                                                })
                                            } else {
                                                Some(Msg {
                                                    v: PROTO_V1,
                                                    id: m.id,
                                                    msg: Response::Err {
                                                        code: ERR_DEVICE_LOCKED,
                                                    },
                                                })
                                            }
                                        } else if let Frame::One(Request::ResetPIN {
                                            current_pin,
                                            new_pin,
                                        }) = &m.msg
                                        {
                                            if is_device_locked() {
                                                Some(Msg {
                                                    v: PROTO_V1,
                                                    id: m.id,
                                                    msg: Response::Err {
                                                        code: ERR_DEVICE_LOCKED,
                                                    },
                                                })
                                            } else {
                                                let mut current_pin_buf = HString::<16>::new();
                                                let mut new_pin_buf = HString::<16>::new();
                                                if current_pin_buf
                                                    .push_str(current_pin.as_str())
                                                    .is_err()
                                                    || new_pin_buf
                                                        .push_str(new_pin.as_str())
                                                        .is_err()
                                                {
                                                    Some(Msg {
                                                        v: PROTO_V1,
                                                        id: m.id,
                                                        msg: Response::Err {
                                                            code: ERR_BAD_COBS_OR_POSTCARD,
                                                        },
                                                    })
                                                } else {
                                                    let request = ChangePinRequest {
                                                        msg_id: m.id,
                                                        current_pin: current_pin_buf,
                                                        new_pin: new_pin_buf,
                                                    };
                                                    match change_pin_controller.submit(request) {
                                                        Ok(()) => {
                                                            set_device_busy(true);
                                                            None
                                                        }
                                                        Err(_) => Some(Msg {
                                                            v: PROTO_V1,
                                                            id: m.id,
                                                            msg: Response::Err { code: ERR_BUSY },
                                                        }),
                                                    }
                                                }
                                            }
                                        } else if let Frame::One(Request::ChangePinOnDevice {
                                            current_pin,
                                        }) = &m.msg
                                        {
                                            if ui.is_none() {
                                                Some(Msg {
                                                    v: PROTO_V1,
                                                    id: m.id,
                                                    msg: Response::Err {
                                                        code: ERR_UNSUPPORTED_VERSION,
                                                    },
                                                })
                                            } else if pending_pin_change.is_some() {
                                                Some(Msg {
                                                    v: PROTO_V1,
                                                    id: m.id,
                                                    msg: Response::Err { code: ERR_BUSY },
                                                })
                                            } else {
                                                let mut current_pin_buf = HString::<16>::new();
                                                if current_pin_buf
                                                    .push_str(current_pin.as_str())
                                                    .is_err()
                                                {
                                                    Some(Msg {
                                                        v: PROTO_V1,
                                                        id: m.id,
                                                        msg: Response::Err {
                                                            code: ERR_BAD_COBS_OR_POSTCARD,
                                                        },
                                                    })
                                                } else {
                                                    pending_pin_change =
                                                        Some(PendingPinChange::New {
                                                            msg_id: m.id,
                                                            current_pin: current_pin_buf,
                                                        });
                                                    set_device_busy(true);
                                                    if let Some(ui) = ui.as_mut() {
                                                        ui.begin_pin_entry("New PIN", None);
                                                    }
                                                    None
                                                }
                                            }
                                        } else if let Frame::One(Request::StartTouchCalibration) =
                                            &m.msg
                                        {
                                            let begin_result = {
                                                let ui_ref = ui.as_mut().map(|u| u as &mut Gui);
                                                dispatch::begin_touch_calibration(ui_ref)
                                            };
                                            match begin_result {
                                                Ok(()) => {
                                                    pending_touch_calibration_id = Some(m.id);
                                                    set_device_busy(true);
                                                    None
                                                }
                                                Err(code) => Some(Msg {
                                                    v: PROTO_V1,
                                                    id: m.id,
                                                    msg: Response::Err { code },
                                                }),
                                            }
                                        } else {
                                            // Show GUI lock screen if lock request comes over USB
                                            if let Frame::One(Request::Lock) = &m.msg {
                                                if let Some(ui) = ui.as_mut() {
                                                    ui.begin_unlock(None);
                                                }
                                            }

                                            if let Some(prompt) = frame_confirmation_prompt(&m.msg)
                                            {
                                                let frame_clone = m.msg.clone();
                                                let begin_result = {
                                                    let ui_ref = ui.as_mut().map(|u| u as &mut Gui);
                                                    begin_confirmation(
                                                        m.id,
                                                        frame_clone,
                                                        prompt,
                                                        ui_ref,
                                                    )
                                                };
                                                match begin_result {
                                                    Ok(()) => None,
                                                    Err(code) => Some(Msg {
                                                        v: PROTO_V1,
                                                        id: m.id,
                                                        msg: Response::Err { code },
                                                    }),
                                                }
                                            } else {
                                                let address_book_result = {
                                                    let ui_ref = ui.as_mut().map(|u| u as &mut Gui);
                                                    maybe_handle_address_book_fragment(
                                                        &m.msg, ui_ref,
                                                    )
                                                };
                                                match address_book_result {
                                                    AddressBookFragmentResult::Immediate(body) => {
                                                        Some(Msg {
                                                            v: PROTO_V1,
                                                            id: m.id,
                                                            msg: body,
                                                        })
                                                    }
                                                    AddressBookFragmentResult::NotAddressBook => {
                                                        let ui_ref =
                                                            ui.as_mut().map(|u| u as &mut Gui);
                                                        let body =
                                                            handle_frame_v1(m.id, &m.msg, ui_ref);

                                                        if let Frame::One(Request::Reboot) = &m.msg
                                                        {
                                                            if let Some(ui) = ui.as_mut() {
                                                                ui.show_idle_message_timed(
                                                                    "Rebooting...",
                                                                    Duration::from_millis(1_000),
                                                                );
                                                            }
                                                        }

                                                        // Show result on GUI after unlock completes
                                                        if let Frame::One(Request::Unlock {
                                                            ..
                                                        }) = &m.msg
                                                        {
                                                            if let Some(ui) = ui.as_mut() {
                                                                match &body {
                                                                        Response::Ok => ui
                                                                            .show_unlock_success(),
                                                                        Response::Err { code }
                                                                            if *code
                                                                                == ERR_WRONG_PIN =>
                                                                        {
                                                                            // Get attempts remaining from lock status
                                                                            let mut nvs =
                                                                                NvsStore::new();
                                                                            let remaining = nvs
                                                                                .get_attempts_remaining();
                                                                            ui.show_pin_failure(
                                                                                if remaining > 0 {
                                                                                    Some(remaining)
                                                                                } else {
                                                                                    None
                                                                                },
                                                                            );
                                                                        }
                                                                        Response::Err { code }
                                                                            if *code
                                                                                == ERR_PIN_LOCKED_OUT =>
                                                                        {
                                                                            ui.show_pin_locked_out();
                                                                        }
                                                                        _ => {}
                                                                    }
                                                            }
                                                        }

                                                        // Show result on GUI after seeding completes
                                                        if let Frame::One(
                                                            Request::InitializePIN { .. },
                                                        ) = &m.msg
                                                        {
                                                            if let Some(ui) = ui.as_mut() {
                                                                match &body {
                                                                        Response::Ok => ui
                                                                            .show_unlock_success(),
                                                                        Response::Err { code }
                                                                            if *code
                                                                                == ERR_ALREADY_INITIALIZED =>
                                                                        {
                                                                            ui.begin_unlock(None);
                                                                        }
                                                                        _ => ui.show_seed_setup(),
                                                                    }
                                                            }
                                                        }

                                                        Some(Msg {
                                                            v: PROTO_V1,
                                                            id: m.id,
                                                            msg: body,
                                                        })
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Ok(_) => Some(Msg {
                                    v: PROTO_V1,
                                    id: 0,
                                    msg: Response::Err {
                                        code: ERR_UNSUPPORTED_VERSION,
                                    },
                                }),
                                Err(_) => Some(Msg {
                                    v: PROTO_V1,
                                    id: 0,
                                    msg: Response::Err {
                                        code: ERR_BAD_COBS_OR_POSTCARD,
                                    },
                                }),
                            };
                            if let Some(resp_msg) = resp_msg {
                                if send_msg(&mut hid, &resp_msg, &mut plain, &mut enc).is_err() {
                                    send_err(&mut hid, ERR_ENCODE_TOO_BIG, &mut enc);
                                }
                            }
                            rx.clear();
                            continue 'main;
                        }
                    }
                }
                Err(usb_device::UsbError::WouldBlock) => break,
                Err(_) => break,
            }
        }
    }
}

fn handle_frame_v1(req_id: u32, frame: &Frame, mut ui: Option<&mut Gui<'_>>) -> Response {
    if update_auth::stream_active() && !update_mode_allows_frame(frame) {
        return Response::Err { code: ERR_BUSY };
    }

    match frame {
        Frame::One(Request::GetAddressBook) => {
            match dispatch::read_address_book_payload(is_device_locked()) {
                Ok(out) => {
                    let total = out.len() as u32;
                    let frag_id = (req_id as u16).max(1);
                    fragments::set_outbound(req_id, frag_id, out);
                    Response::FragBegin {
                        id: frag_id,
                        total_len: total,
                        kind: FragKind::AddressBook,
                    }
                }
                Err(resp) => resp,
            }
        }
        Frame::One(req) => handle_request_v1(req, ui),
        Frame::FragBegin {
            id,
            total_len,
            kind,
        } => {
            if fragments::begin_inbound(*id, *kind, *total_len).is_err() {
                return Response::Err { code: ERR_OVERFLOW };
            }
            Response::Ok
        }
        Frame::FragPart {
            id,
            offset,
            chunk,
            last,
        } => {
            let Some(mut st) = fragments::take_inbound() else {
                return Response::Err {
                    code: ERR_BAD_COBS_OR_POSTCARD,
                };
            };
            if st.id != *id || st.next_off != *offset {
                fragments::put_inbound(st);
                return Response::Err {
                    code: ERR_BAD_COBS_OR_POSTCARD,
                };
            }
            if st.buf.len() + chunk.len() > (st.total_len as usize) {
                fragments::put_inbound(st);
                return Response::Err { code: ERR_OVERFLOW };
            }

            st.buf.extend_from_slice(chunk.as_slice());
            st.next_off += chunk.len() as u32;
            if !*last {
                fragments::put_inbound(st);
                return Response::Ok;
            }

            if st.next_off != st.total_len {
                return Response::Err {
                    code: ERR_BAD_COBS_OR_POSTCARD,
                };
            }

            match st.kind {
                FragKind::SetSeed => {
                    if st.buf.len() != 64 {
                        return Response::Err {
                            code: ERR_BAD_COBS_OR_POSTCARD,
                        };
                    }
                    let mut arr = [0u8; 64];
                    arr.copy_from_slice(st.buf.as_slice());
                    set_seed(&arr);
                    Response::Ok
                }
                FragKind::SignDraft => {
                    if is_device_locked() {
                        return Response::Err {
                            code: ERR_DEVICE_LOCKED,
                        };
                    }
                    let cfg = match signing::active_root_signer_config() {
                        Ok(cfg) => cfg,
                        Err(code) => return Response::Err { code },
                    };
                    if let Some(ui) = ui.as_mut() {
                        let review = match nockster_core::draft_sign::draft_review_v1(
                            st.buf.as_slice(),
                            &cfg,
                        ) {
                            Ok(v) => v,
                            Err(nockster_core::draft_sign::SignDraftError::Unsupported) => {
                                return Response::Err {
                                    code: ERR_UNSUPPORTED_VERSION,
                                };
                            }
                            Err(_) => {
                                return Response::Err {
                                    code: ERR_BAD_COBS_OR_POSTCARD,
                                };
                            }
                        };

                        let draft = core::mem::take(&mut st.buf);
                        let stored = critical_section::with(|cs| {
                            let mut slot = PENDING_SIGN_DRAFT.borrow_ref_mut(cs);
                            if slot.is_some() {
                                false
                            } else {
                                slot.replace(PendingSignDraft {
                                    msg_id: req_id,
                                    frag_id: st.id,
                                    sk_be: cfg.sk_be,
                                    draft,
                                });
                                true
                            }
                        });

                        if !stored {
                            return Response::Err { code: ERR_BUSY };
                        }

                        let summary = tx_review_summary_from_draft(&review);
                        set_device_busy(true);
                        ui.request_tx_review_with_summary(
                            "Review Tx",
                            Some(summary),
                            review
                                .outputs
                                .iter()
                                .filter(|o| !o.is_refund)
                                .map(|o| (o.gift, o.recipient_b58.as_str())),
                        );
                        Response::Ok
                    } else {
                        let out =
                            match nockster_core::draft_sign::sign_draft_v1(st.buf.as_slice(), &cfg)
                            {
                                Ok(v) => v,
                                Err(nockster_core::draft_sign::SignDraftError::Unsupported) => {
                                    return Response::Err {
                                        code: ERR_UNSUPPORTED_VERSION,
                                    };
                                }
                                Err(_) => {
                                    return Response::Err {
                                        code: ERR_BAD_COBS_OR_POSTCARD,
                                    };
                                }
                            };

                        let total = out.len() as u32;
                        fragments::set_outbound(req_id, st.id, out);
                        Response::FragBegin {
                            id: st.id,
                            total_len: total,
                            kind: FragKind::SignDraft,
                        }
                    }
                }
                FragKind::AddressBook => {
                    dispatch::write_address_book_payload(st.buf.as_slice(), is_device_locked())
                }
            }
        }
    }
}

fn handle_request_v1(req: &Request, ui: Option<&mut Gui<'_>>) -> Response {
    match req {
        Request::Hello
        | Request::GetInfo
        | Request::Ping
        | Request::GetLockStatus
        | Request::GetSecurityStatus
        | Request::GetBuildInfo => {
            dispatch::handle_metadata_request(req, FW_MAJOR, FW_MINOR, FIRMWARE_FEATURES).unwrap_or(
                Response::Err {
                    code: ERR_UNSUPPORTED_VERSION,
                },
            )
        }
        Request::Reboot => {
            set_device_busy(true);
            request_soft_reboot();
            Response::Ok
        }
        Request::SetSeed { .. } | Request::Wipe | Request::Lock | Request::SelectSeed { .. } => {
            seed_store::handle_session_request(req).unwrap_or(Response::Err {
                code: ERR_UNSUPPORTED_VERSION,
            })
        }
        Request::GetFingerprint
        | Request::GetPubkey { .. }
        | Request::SignDigest { .. }
        | Request::GetXpub { .. }
        | Request::GetCheetahPub { .. }
        | Request::SignSpendHash { .. }
        | Request::SignSpendHashFor { .. }
        | Request::Health => {
            signing::handle_request(req, is_device_locked()).unwrap_or(Response::Err {
                code: ERR_UNSUPPORTED_VERSION,
            })
        }
        Request::InitializePIN { .. }
        | Request::AddSeed { .. }
        | Request::DeleteSeed { .. }
        | Request::Unlock { .. } => Response::Err {
            code: ERR_UNSUPPORTED_VERSION,
        },
        Request::ResetPIN { .. } => Response::Err {
            code: ERR_UNSUPPORTED_VERSION,
        },
        Request::ChangePinOnDevice { .. } => Response::Err {
            code: ERR_UNSUPPORTED_VERSION,
        },
        Request::StartTouchCalibration => Response::Err {
            code: ERR_UNSUPPORTED_VERSION,
        },
        Request::Reset => Response::Err {
            code: ERR_UNSUPPORTED_VERSION,
        },
        Request::GetReleaseInfo
        | Request::GetUpdateBootStatus
        | Request::GetUpdateTrust
        | Request::VerifyUpdateManifest { .. }
        | Request::BeginUpdate { .. }
        | Request::UpdateChunk { .. }
        | Request::FinishUpdate
        | Request::CancelUpdate
        | Request::GetUpdateStatus => {
            dispatch::handle_update_request(req).unwrap_or(Response::Err {
                code: ERR_UNSUPPORTED_VERSION,
            })
        }
        Request::GetTouchCalibration
        | Request::SetTouchCalibration { .. }
        | Request::ShowTouchDiagnostics { .. } => {
            dispatch::handle_touch_request(req, ui, session::is_locked()).unwrap_or(Response::Err {
                code: ERR_UNSUPPORTED_VERSION,
            })
        }
        Request::GetSeedLabels | Request::SetSeedLabel { .. } => {
            dispatch::handle_seed_label_request(req, is_device_locked()).unwrap_or(Response::Err {
                code: ERR_UNSUPPORTED_VERSION,
            })
        }
        Request::GetAddressBook => Response::Err {
            code: ERR_UNSUPPORTED_VERSION,
        },
    }
}

fn is_device_locked() -> bool {
    session::is_locked()
}

fn compute_unlock_outcome(state: &mut AppCoreState<'_>, pin: &str) -> UnlockOutcome {
    let mut nvs = NvsStore::new();
    match nvs.unlock_with_pepper_readonly(pin, &mut state.nvs_pepper) {
        Ok((seeds, master_key, clear_attempts)) => UnlockOutcome::Success {
            seeds,
            master_key,
            clear_attempts,
        },
        Err(NvsError::WrongPin) => UnlockOutcome::WrongPin,
        Err(NvsError::LockedOut) => UnlockOutcome::LockedOut,
        Err(NvsError::NotInitialized) => UnlockOutcome::NotInitialized,
        Err(NvsError::Flash) => UnlockOutcome::Flash,
        Err(_) => UnlockOutcome::Failed,
    }
}

fn apply_unlock_success(seeds: &[[u8; 64]], master_key: &[u8; 32]) {
    update_seed_store_from_slice(seeds);
    store_master_key(master_key);
    session::set_locked(false);
}

fn zeroize_seed_vec_runtime(seeds: &mut [[u8; 64]]) {
    for seed in seeds {
        seed.zeroize();
    }
}

fn handle_unlock_outcome<B: usb_device::bus::UsbBus>(
    outcome: UnlockOutcome,
    mut ui: Option<&mut Gui<'_>>,
    hid: &mut HIDClass<'_, B>,
) -> Response {
    match outcome {
        UnlockOutcome::Success {
            seeds,
            master_key,
            clear_attempts,
        } => {
            if clear_attempts {
                if NvsStore::new().clear_pin_attempts().is_err() {
                    if let Some(ui) = ui.as_mut() {
                        ui.show_pin_failure(None);
                    }
                    usb_debug(hid, b"unlock clear attempts failed\r\n");
                    return Response::Err { code: ERR_FLASH };
                }
            }
            apply_unlock_success(seeds.as_slice(), &master_key);
            if let Some(ui) = ui.as_mut() {
                ui.show_unlock_success();
            }
            usb_debug(hid, b"unlock success\r\n");
            Response::Ok
        }
        UnlockOutcome::WrongPin => {
            let attempts_remaining = match NvsStore::new().record_wrong_pin_attempt() {
                Ok(remaining) => remaining,
                Err(_) => {
                    if let Some(ui) = ui.as_mut() {
                        ui.show_pin_failure(None);
                    }
                    usb_debug(hid, b"wrong pin attempt record failed\r\n");
                    return Response::Err { code: ERR_FLASH };
                }
            };
            if attempts_remaining == 0 {
                if let Some(ui) = ui.as_mut() {
                    ui.show_pin_locked_out();
                }
                usb_debug(hid, b"pin locked out\r\n");
                return Response::Err {
                    code: ERR_PIN_LOCKED_OUT,
                };
            }
            if let Some(ui) = ui.as_mut() {
                ui.show_pin_failure(Some(attempts_remaining));
            }
            usb_debug(hid, b"wrong pin\r\n");
            Response::Err {
                code: ERR_WRONG_PIN,
            }
        }
        UnlockOutcome::LockedOut => {
            if let Some(ui) = ui.as_mut() {
                ui.show_pin_locked_out();
            }
            usb_debug(hid, b"pin locked out\r\n");
            Response::Err {
                code: ERR_PIN_LOCKED_OUT,
            }
        }
        UnlockOutcome::NotInitialized => {
            if let Some(ui) = ui.as_mut() {
                ui.show_pin_not_initialized();
            }
            usb_debug(hid, b"pin not set\r\n");
            Response::Err { code: ERR_NO_SEED }
        }
        UnlockOutcome::Flash => {
            if let Some(ui) = ui.as_mut() {
                ui.show_pin_failure(None);
            }
            usb_debug(hid, b"unlock flash error\r\n");
            Response::Err { code: ERR_FLASH }
        }
        UnlockOutcome::Failed => {
            if let Some(ui) = ui.as_mut() {
                ui.show_pin_failure(None);
            }
            usb_debug(hid, b"unlock failed\r\n");
            Response::Err { code: ERR_NO_SEED }
        }
    }
}

fn handle_initpin_outcome<B: usb_device::bus::UsbBus>(
    outcome: InitPinOutcome,
    mut ui: Option<&mut Gui<'_>>,
    hid: &mut HIDClass<'_, B>,
) -> Response {
    match outcome {
        InitPinOutcome::Prepared {
            seed64,
            master_key,
            prepared,
        } => {
            let mut seed64 = seed64;
            let mut master_key = master_key;
            set_initpin_progress(InitPinProgress::NvsWriteFlash);
            if let Some(ui) = ui.as_mut() {
                ui.show_unlocking_stage("Seed: flash");
            }
            let commit = NvsStore::new().commit_prepared_initialize_pin(prepared);
            if let Err(err) = commit {
                master_key.zeroize();
                seed64.zeroize();
                set_initpin_progress(InitPinProgress::Error);
                if let Some(ui) = ui.as_mut() {
                    ui.show_seed_setup();
                }
                usb_debug(hid, b"pin init flash failed\r\n");
                return match err {
                    NvsError::Flash => Response::Err { code: ERR_FLASH },
                    NvsError::Crypto => Response::Err { code: ERR_CRYPTO },
                    _ => Response::Err { code: ERR_NO_SEED },
                };
            }
            set_initpin_progress(InitPinProgress::Complete);
            store_master_key(&master_key);
            set_seed(&seed64);
            session::set_locked(false);
            if let Some(ui) = ui.as_mut() {
                ui.show_unlock_success();
            }
            usb_debug(hid, b"seed+pin initialized\r\n");
            master_key.zeroize();
            seed64.zeroize();
            Response::Ok
        }
        InitPinOutcome::AlreadyInitialized => {
            if let Some(ui) = ui {
                ui.begin_unlock(None);
            }
            usb_debug(hid, b"already initialized\r\n");
            Response::Err {
                code: ERR_ALREADY_INITIALIZED,
            }
        }
        InitPinOutcome::Flash => {
            if let Some(ui) = ui {
                ui.show_seed_setup();
            }
            usb_debug(hid, b"pin init failed\r\n");
            Response::Err { code: ERR_FLASH }
        }
        InitPinOutcome::Crypto => {
            if let Some(ui) = ui {
                ui.show_seed_setup();
            }
            usb_debug(hid, b"pin init failed\r\n");
            Response::Err { code: ERR_CRYPTO }
        }
        InitPinOutcome::Failed => {
            if let Some(ui) = ui {
                ui.show_seed_setup();
            }
            usb_debug(hid, b"pin init failed\r\n");
            Response::Err { code: ERR_NO_SEED }
        }
    }
}

fn show_pin_change_failure(ui: &mut Gui<'_>, message: &str) {
    if session::is_locked() {
        ui.show_pin_failure(None);
    } else {
        ui.show_idle_message_timed(message, Duration::from_millis(3_000));
    }
}

fn handle_change_pin_outcome<B: usb_device::bus::UsbBus>(
    outcome: ChangePinOutcome,
    mut ui: Option<&mut Gui<'_>>,
    hid: &mut HIDClass<'_, B>,
) -> (u32, Response) {
    match outcome {
        ChangePinOutcome::Prepared {
            msg_id,
            mut seeds,
            mut master_key,
            prepared,
        } => {
            if NvsStore::new()
                .commit_prepared_seed_rewrite(prepared)
                .is_err()
            {
                zeroize_seed_vec_runtime(seeds.as_mut_slice());
                master_key.zeroize();
                if let Some(ui) = ui.as_mut() {
                    show_pin_change_failure(ui, "Flash error");
                }
                usb_debug(hid, b"pin change flash error\r\n");
                return (msg_id, Response::Err { code: ERR_FLASH });
            }
            apply_unlock_success(seeds.as_slice(), &master_key);
            zeroize_seed_vec_runtime(seeds.as_mut_slice());
            master_key.zeroize();
            if let Some(ui) = ui.as_mut() {
                ui.show_unlock_success();
            }
            usb_debug(hid, b"pin changed\r\n");
            (msg_id, Response::Ok)
        }
        ChangePinOutcome::WrongPin { msg_id } => {
            let attempts_remaining = match NvsStore::new().record_wrong_pin_attempt() {
                Ok(remaining) => remaining,
                Err(_) => {
                    if let Some(ui) = ui.as_mut() {
                        show_pin_change_failure(ui, "Flash error");
                    }
                    usb_debug(hid, b"pin change wrong pin record failed\r\n");
                    return (msg_id, Response::Err { code: ERR_FLASH });
                }
            };
            if attempts_remaining == 0 {
                if let Some(ui) = ui.as_mut() {
                    ui.show_pin_locked_out();
                }
                usb_debug(hid, b"pin change locked out\r\n");
                return (
                    msg_id,
                    Response::Err {
                        code: ERR_PIN_LOCKED_OUT,
                    },
                );
            }
            if let Some(ui) = ui.as_mut() {
                if session::is_locked() {
                    ui.show_pin_failure(Some(attempts_remaining));
                } else {
                    ui.show_idle_message_timed("Bad PIN", Duration::from_millis(3_000));
                }
            }
            usb_debug(hid, b"pin change wrong pin\r\n");
            (
                msg_id,
                Response::Err {
                    code: ERR_WRONG_PIN,
                },
            )
        }
        ChangePinOutcome::LockedOut { msg_id } => {
            if let Some(ui) = ui {
                ui.show_pin_locked_out();
            }
            usb_debug(hid, b"pin change locked out\r\n");
            (
                msg_id,
                Response::Err {
                    code: ERR_PIN_LOCKED_OUT,
                },
            )
        }
        ChangePinOutcome::Flash { msg_id } => {
            if let Some(ui) = ui {
                show_pin_change_failure(ui, "Flash error");
            }
            usb_debug(hid, b"pin change flash error\r\n");
            (msg_id, Response::Err { code: ERR_FLASH })
        }
        ChangePinOutcome::Crypto { msg_id } => {
            if let Some(ui) = ui {
                show_pin_change_failure(ui, "Crypto error");
            }
            usb_debug(hid, b"pin change crypto error\r\n");
            (msg_id, Response::Err { code: ERR_CRYPTO })
        }
        ChangePinOutcome::Failed { msg_id } => {
            if let Some(ui) = ui {
                show_pin_change_failure(ui, "PIN change failed");
            }
            usb_debug(hid, b"pin change failed\r\n");
            (msg_id, Response::Err { code: ERR_NO_SEED })
        }
    }
}

fn handle_seed_op_outcome<B: usb_device::bus::UsbBus>(
    outcome: SeedOpOutcome,
    ui: Option<&mut Gui<'_>>,
    hid: &mut HIDClass<'_, B>,
) -> (u32, Response) {
    let applied = seed_store::apply_seed_op_outcome(outcome);
    if let Some(ui) = ui {
        match applied.ui_effect {
            SeedOpUiEffect::None => {}
            SeedOpUiEffect::Added => {
                ui.show_idle_message_timed("Seed added", Duration::from_millis(3_000));
            }
            SeedOpUiEffect::Deleted => {
                if session::has_seed() {
                    ui.show_idle_message_timed("Seed deleted", Duration::from_millis(3_000));
                } else {
                    ui.show_seed_setup();
                }
            }
            SeedOpUiEffect::Reset => {
                ui.show_seed_setup();
            }
        }
    }
    usb_debug(hid, applied.debug);
    (applied.msg_id, applied.response)
}
