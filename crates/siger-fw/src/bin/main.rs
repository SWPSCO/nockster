#![no_std]
#![no_main]
#![deny(clippy::mem_forget, reason = "unsafe for esp-hal types")]
mod gui;
mod random;
use panic_halt as _;
use siger_fw::nvs_store::{NvsError, NvsStore};
extern crate alloc;
use alloc::vec::Vec;
use bip32::{ChildNumber, DerivationPath, PublicKey, XPrv};
use cobs::encode;
use core::cell::RefCell;
use critical_section::Mutex;
use esp_hal::system::{CpuControl, Stack};
use esp_hal::time::{Duration, Instant};
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_hal::{clock::CpuClock, delay::Delay, main};
use gui::{Gui, GuiInteraction, SeedInteraction};
use heapless::{String as HString, Vec as HVec};
use hmac::Hmac;
use k256::ecdsa::{signature::hazmat::PrehashSigner, Signature, SigningKey};
use ripemd::Ripemd160;
use pbkdf2::pbkdf2;
use sha2::{Digest, Sha256, Sha512};
use siger_core::alloc_path as pathmod;
use siger_core::{CheetahPub, *};
use zeroize::Zeroize;
const DEMO_SK: [u8; 32] = [0x11; 32];
const FW_MAJOR: u16 = 0;
const FW_MINOR: u16 = 1;
// feature masks
const FEAT_CHEETAH: u32 = 1 << 0;
const FEAT_FRAG: u32 = 1 << 1;
const FEAT_XPUB: u32 = 1 << 2;
// Maximum total size for fragment-assembled payloads (e.g. SignDraft).
// Keep this bounded to avoid unbounded heap growth on malformed hosts.
const MAX_FRAG_TOTAL: usize = 64 * 1024;
const TX_CHUNK: usize = 200;
const PLAIN_BUF_LEN: usize = 4096;
const ENC_BUF_LEN: usize = cobs::max_encoding_length(PLAIN_BUF_LEN) + 1;
// TX queue size: large enough to buffer multiple responses during concurrent requests
// (e.g., polling GetInfo while seeding with fragments). 3x ENC_BUF_LEN allows ~3 full messages.
const TX_QUEUE_LEN: usize = ENC_BUF_LEN * 3;
const BIP39_PBKDF2_ROUNDS: u32 = 2048;
// lock state (locked by default)
static mut DEVICE_LOCKED: bool = true;
static mut DEVICE_BUSY: bool = false;
static mut MASTER_KEY: [u8; 32] = [0; 32];
static mut MASTER_KEY_SET: bool = false;
static mut APP_CORE_STACK: Stack<8192> = Stack::new();
esp_bootloader_esp_idf::esp_app_desc!();
// USB TX Queue: Prevents device lockup when USB writes would block.
//
// Without this queue, writing to USB when no host is connected would block indefinitely.
// The queue also prevents lockups when the host is polling (e.g., GetInfo every 2s) while
// processing multi-message operations like fragment-based seeding.
//
// The queue must be large enough to buffer multiple responses during concurrent requests.
struct UsbTxQueue {
    buf: HVec<u8, TX_QUEUE_LEN>,
    pos: usize,
}

static mut USB_TX_QUEUE: Option<UsbTxQueue> = None;

#[inline]
fn usb_service_tx(usb: &mut UsbSerialJtag<'_, esp_hal::Blocking>) {
    unsafe {
        if let Some(queue) = USB_TX_QUEUE.as_mut() {
            while queue.pos < queue.buf.len() {
                match usb.write_byte_nb(queue.buf[queue.pos]) {
                    Ok(()) => queue.pos += 1,
                    Err(nb::Error::WouldBlock) => {
                        let _ = usb.flush_tx_nb();
                        return;
                    }
                    Err(_) => {
                        USB_TX_QUEUE = None;
                        return;
                    }
                }
            }
            let _ = usb.flush_tx_nb();
            USB_TX_QUEUE = None;
        }
    }
}

#[inline]
fn usb_write(usb: &mut UsbSerialJtag<'_, esp_hal::Blocking>, buf: &[u8]) {
    unsafe {
        let queue = USB_TX_QUEUE.get_or_insert_with(|| UsbTxQueue {
            buf: HVec::new(),
            pos: 0,
        });
        if queue.buf.is_empty() {
            queue.pos = 0;
        }

        // Try to extend the queue. If it fails, try to drain and retry once.
        if queue.buf.extend_from_slice(buf).is_err() {
            // Queue is full - try to drain some data first
            usb_service_tx(usb);

            // If still can't fit after draining, we have to drop this message
            // (better to drop than to lock up the device)
            let _ = queue.buf.extend_from_slice(buf);
        }
    }
    usb_service_tx(usb);
}

#[inline]
fn usb_debug(usb: &mut UsbSerialJtag<'_, esp_hal::Blocking>, msg: &[u8]) {
    // NOTE: Avoid writing plaintext logs on the protocol USB serial channel in
    // release builds; it can corrupt framing for host tooling.
    #[cfg(debug_assertions)]
    {
        usb_write(usb, msg);
    }
    let _ = (usb, msg);
}

struct SeedStore {
    slots: HVec<[u8; 64], MAX_SEED_SLOTS>,
    active: usize,
}
static mut SEED_STORE: SeedStore = SeedStore {
    slots: HVec::new(),
    active: 0,
};
struct FragState {
    id: u16,
    kind: FragKind,
    total_len: u32,
    next_off: u32,
    buf: Vec<u8>,
}
static mut FRAG: Option<FragState> = None;
// outbound fragment queue
struct OutFrag {
    msg_id: u32,
    id: u16,
    kind: FragKind,
    off: u32,
    data: Vec<u8>,
}
static mut OUT_FRAG: Option<OutFrag> = None;
enum UnlockAttempt {
    Success,
    WrongPin { attempts_remaining: u8 },
    LockedOut,
    NotInitialized,
    Failed,
}

enum UnlockOutcome {
    Success {
        seeds: Vec<[u8; 64]>,
        master_key: [u8; 32],
    },
    WrongPin {
        attempts_remaining: u8,
    },
    LockedOut,
    NotInitialized,
    Failed,
}

struct UnlockRequest {
    pin: HString<16>,
}

#[allow(clippy::declare_interior_mutable_const)]
static UNLOCK_REQUEST: Mutex<RefCell<Option<UnlockRequest>>> = Mutex::new(RefCell::new(None));
#[allow(clippy::declare_interior_mutable_const)]
static UNLOCK_RESULT: Mutex<RefCell<Option<UnlockOutcome>>> = Mutex::new(RefCell::new(None));

struct UnlockController {
    awaiting_result: bool,
    submitted_at: Option<Instant>,
    last_pin: Option<HString<16>>,
    ignore_next_result: bool,
}

struct PendingConfirmation {
    msg_id: u32,
    frame: Frame,
    prompt: &'static str,
}

struct PendingSignDraft {
    msg_id: u32,
    frag_id: u16,
    sk_be: [u8; 32],
    draft: Vec<u8>,
}

#[allow(clippy::declare_interior_mutable_const)]
static PENDING_CONFIRMATION: Mutex<RefCell<Option<PendingConfirmation>>> =
    Mutex::new(RefCell::new(None));

#[allow(clippy::declare_interior_mutable_const)]
static PENDING_SIGN_DRAFT: Mutex<RefCell<Option<PendingSignDraft>>> = Mutex::new(RefCell::new(None));

impl UnlockController {
    fn new() -> Self {
        Self {
            awaiting_result: false,
            submitted_at: None,
            last_pin: None,
            ignore_next_result: false,
        }
    }

    fn submit(&mut self, pin: &HString<16>) -> Result<(), ()> {
        if self.awaiting_result {
            return Err(());
        }

        let mut pin_buf = HString::<16>::new();
        pin_buf.push_str(pin.as_str()).map_err(|_| ())?;
        let stored_pin = pin_buf.clone();

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
        self.submitted_at = Some(Instant::now());
        self.last_pin = Some(stored_pin);
        self.ignore_next_result = false;
        Ok(())
    }

    fn poll(&mut self) -> Option<UnlockOutcome> {
        let outcome = critical_section::with(|cs| {
            let mut slot = UNLOCK_RESULT.borrow_ref_mut(cs);
            let outcome = slot.take();
            if outcome.is_some() {
                self.awaiting_result = false;
                self.submitted_at = None;
                self.last_pin = None;
            }
            outcome
        });

        if let Some(result) = outcome {
            if self.ignore_next_result {
                self.ignore_next_result = false;
                return None;
            }
            return Some(result);
        }

        if self.awaiting_result {
            if let Some(start) = self.submitted_at {
                if Instant::now() - start >= Duration::from_millis(2000) {
                    if let Some(pin) = self.last_pin.clone() {
                        let outcome = compute_unlock_outcome(pin.as_str());
                        self.awaiting_result = false;
                        self.submitted_at = None;
                        self.last_pin = None;
                        self.ignore_next_result = true;
                        return Some(outcome);
                    }
                    self.submitted_at = Some(Instant::now());
                }
            }
        }

        None
    }
}

fn take_unlock_request() -> Option<UnlockRequest> {
    critical_section::with(|cs| {
        let mut slot = UNLOCK_REQUEST.borrow_ref_mut(cs);
        slot.take()
    })
}

fn store_unlock_result(outcome: UnlockOutcome) {
    critical_section::with(|cs| {
        let mut slot = UNLOCK_RESULT.borrow_ref_mut(cs);
        *slot = Some(outcome);
    });
}

fn unlock_worker_loop() -> ! {
    let mut delay = Delay::new();
    loop {
        if let Some(request) = take_unlock_request() {
            let outcome = compute_unlock_outcome(request.pin.as_str());
            store_unlock_result(outcome);
        } else {
            delay.delay_millis(5);
        }
    }
}

fn begin_confirmation(
    msg_id: u32,
    frame: Frame,
    prompt: &'static str,
    ui: Option<&mut Gui<'_>>,
) -> Result<(), ()> {
    let mut spend_outputs: HVec<(u64, HString<24>), 24> = HVec::new();
    let show_spend_outputs = match &frame {
        Frame::One(Request::SignSpendHash { meta: Some(meta), .. })
        | Frame::One(Request::SignSpendHashFor { meta: Some(meta), .. }) => {
            for out in meta.outputs.iter() {
                if out.gift == 0 || out.is_refund {
                    continue;
                }
                let recipient_short = shorten_b58(out.recipient_pkh_b58.as_str());
                let candidate = (out.gift, recipient_short);
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
            pending.replace(PendingConfirmation {
                msg_id,
                frame,
                prompt,
            });
            true
        }
    });

    if !stored {
        return Err(());
    }

    if let Some(ui) = ui {
        if show_spend_outputs {
            ui.request_tx_review_with_header(
                prompt,
                spend_outputs
                    .iter()
                    .map(|(gift, recipient)| (*gift, recipient.as_str())),
            );
        } else {
            ui.request_confirmation_with_details(prompt, None);
        }
    }

    Ok(())
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
    let mut out = alloc::vec::Vec::new();
    match postcard::from_bytes_cobs::<Msg<Frame>>(frame) {
        Ok(m) if m.v == PROTO_V1 => {
            let body = handle_frame_v1(m.id, &m.msg, None);
            let resp = Msg {
                v: PROTO_V1,
                id: m.id,
                msg: body,
            };
            let tmp = postcard::to_allocvec(&resp).unwrap();
            // COBS encode into a scratch slice, then append 0x00
            let mut enc = alloc::vec::Vec::with_capacity(cobs::max_encoding_length(tmp.len()));
            enc.resize(cobs::max_encoding_length(tmp.len()), 0);
            let used = cobs::encode(&tmp, &mut enc[..]);
            enc.truncate(used);
            out.extend_from_slice(&enc);
            out.push(0);
        }
        Ok(_) => {
            let err = Msg {
                v: PROTO_V1,
                id: 0,
                msg: Response::Err {
                    code: siger_core::ERR_UNSUPPORTED_VERSION,
                },
            };
            let tmp = postcard::to_allocvec(&err).unwrap();
            let mut enc = alloc::vec::Vec::with_capacity(cobs::max_encoding_length(tmp.len()));
            enc.resize(cobs::max_encoding_length(tmp.len()), 0);
            let used = cobs::encode(&tmp, &mut enc[..]);
            enc.truncate(used);
            out.extend_from_slice(&enc);
            out.push(0);
        }
        Err(_) => {
            let err = Msg {
                v: PROTO_V1,
                id: 0,
                msg: Response::Err {
                    code: siger_core::ERR_BAD_COBS_OR_POSTCARD,
                },
            };
            let tmp = postcard::to_allocvec(&err).unwrap();
            let mut enc = alloc::vec::Vec::with_capacity(cobs::max_encoding_length(tmp.len()));
            enc.resize(cobs::max_encoding_length(tmp.len()), 0);
            let used = cobs::encode(&tmp, &mut enc[..]);
            enc.truncate(used);
            out.extend_from_slice(&enc);
            out.push(0);
        }
    }
    out
}
#[main]
fn main() -> ! {
    let cfg = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let p = esp_hal::init(cfg);
    esp_alloc::heap_allocator!(size: 64 * 1024);
    let mut delay = Delay::new();
    let mut ui = Gui::new(
        p.SPI2, p.GPIO38, p.GPIO39, p.GPIO45, p.GPIO21, p.GPIO40, p.GPIO46, p.I2C0, p.GPIO41,
        p.GPIO42, p.GPIO47, p.GPIO48, &mut delay,
    )
    .ok();
    let pin_required = {
        let mut nvs = NvsStore::new();
        nvs.is_initialized()
    };
    delay.delay_millis(2_000);
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
    let mut cpu_control = CpuControl::new(p.CPU_CTRL);
    let _app_core_guard = cpu_control
        .start_app_core(unsafe { &mut APP_CORE_STACK }, move || unlock_worker_loop())
        .expect("failed to start app core");

    let mut unlock_controller = UnlockController::new();
    let mut pending_seed_setup: Option<[u8; 64]> = None;

    let mut usb = UsbSerialJtag::new(p.USB_DEVICE);
    // Bigger working buffers to accommodate TX_CHUNK
    let mut rx: HVec<u8, 512> = HVec::new();
    let mut plain = [0u8; PLAIN_BUF_LEN];
    let mut enc = [0u8; ENC_BUF_LEN];
    'main: loop {
        usb_service_tx(&mut usb);
    if let Some(ui) = ui.as_mut() {
        let event = ui.tick();
        if let Some(decision) = ui.poll_confirmation_result() {
            if let Some(pending) = take_pending_confirmation() {
                let resp_msg = if decision {
                    ui.show_idle_message_timed("Approved", Duration::from_millis(3_000));
                        let body = handle_frame_v1(pending.msg_id, &pending.frame, None);
                        Msg {
                            v: PROTO_V1,
                            id: pending.msg_id,
                            msg: body,
                        }
                } else {
                    ui.show_idle_message_timed("Rejected", Duration::from_millis(3_000));
                    Msg {
                        v: PROTO_V1,
                        id: pending.msg_id,
                        msg: Response::Err {
                            code: ERR_REJECTED_BY_USER,
                        },
                    }
                };
                if let Ok(used) = postcard::to_slice(&resp_msg, &mut plain) {
                    let n = encode(used, &mut enc);
                    let _ = usb_write(&mut usb, &enc[..n]);
                    let _ = usb_write(&mut usb, &[0]);
                } else {
                    send_err(&mut usb, ERR_ENCODE_TOO_BIG, &mut enc);
                }
                if decision {
                    usb_debug(&mut usb, b"confirm accepted\r\n");
                } else {
                    usb_debug(&mut usb, b"confirm rejected\r\n");
                }
            } else if let Some(mut pending) = take_pending_sign_draft() {
                let resp_msg = if decision {
                    ui.show_idle_message_timed("Signing...", Duration::from_millis(3_000));
                    let cfg = siger_core::draft_sign::SignerConfig { sk_be: pending.sk_be };
                    match siger_core::draft_sign::sign_draft_v1(pending.draft.as_slice(), &cfg) {
                        Ok(out) => {
                            let total = out.len() as u32;
                            unsafe {
                                OUT_FRAG = Some(OutFrag {
                                    msg_id: pending.msg_id,
                                    id: pending.frag_id,
                                    kind: FragKind::SignDraft,
                                    off: 0,
                                    data: out,
                                });
                            }
                            Msg {
                                v: PROTO_V1,
                                id: pending.msg_id,
                                msg: Response::FragBegin {
                                    id: pending.frag_id,
                                    total_len: total,
                                    kind: FragKind::SignDraft,
                                },
                            }
                        }
                        Err(siger_core::draft_sign::SignDraftError::Unsupported) => Msg {
                            v: PROTO_V1,
                            id: pending.msg_id,
                            msg: Response::Err {
                                code: ERR_UNSUPPORTED_VERSION,
                            },
                        },
                        Err(_) => Msg {
                            v: PROTO_V1,
                            id: pending.msg_id,
                            msg: Response::Err {
                                code: ERR_BAD_COBS_OR_POSTCARD,
                            },
                        },
                    }
                } else {
                    ui.show_idle_message_timed("Rejected", Duration::from_millis(3_000));
                    Msg {
                        v: PROTO_V1,
                        id: pending.msg_id,
                        msg: Response::Err {
                            code: ERR_REJECTED_BY_USER,
                        },
                    }
                };
                pending.sk_be.zeroize();
                if let Ok(used) = postcard::to_slice(&resp_msg, &mut plain) {
                    let n = encode(used, &mut enc);
                    let _ = usb_write(&mut usb, &enc[..n]);
                    let _ = usb_write(&mut usb, &[0]);
                } else {
                    send_err(&mut usb, ERR_ENCODE_TOO_BIG, &mut enc);
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
                        if let Some(mut seed64) = pending_seed_setup.take() {
                            ui.show_unlocking();

                            unsafe { DEVICE_BUSY = true; }
                            let mut nvs = NvsStore::new();
                            let pub_xy = root_pub_from_seed(&seed64);
                            let outcome = nvs.initialize_pin(pin.as_str(), &seed64, pub_xy);
                            unsafe { DEVICE_BUSY = false; }

                            match outcome {
                                Ok((master_key, _slot)) => {
                                    store_master_key(&master_key);
                                    set_seed(&seed64);
                                    unsafe {
                                        DEVICE_LOCKED = false;
                                    }
                                    ui.show_unlock_success();
                                    usb_debug(&mut usb, b"seed+pin initialized\r\n");
                                }
                                Err(NvsError::AlreadyInitialized) => {
                                    ui.show_pin_failure(None);
                                    usb_debug(&mut usb, b"already initialized\r\n");
                                }
                                Err(_) => {
                                    ui.show_pin_failure(None);
                                    usb_debug(&mut usb, b"pin init failed\r\n");
                                }
                            }

                            seed64.zeroize();
                        } else {
                            match unlock_controller.submit(&pin) {
                                Ok(()) => {
                                    ui.show_unlocking();
                                }
                                Err(()) => {
                                    ui.show_pin_failure(None);
                                    usb_debug(&mut usb, b"unlock queue busy\r\n");
                                }
                            }
                        }
                    }
                    GuiInteraction::ConfirmAccepted => {
                        usb_debug(&mut usb, b"confirm accepted\r\n");
                    }
                    GuiInteraction::ConfirmRejected => {
                        usb_debug(&mut usb, b"confirm rejected\r\n");
                    }
                    GuiInteraction::LockRequested => {
                        wipe_seed();
                        ui.begin_unlock(None);
                        usb_debug(&mut usb, b"locked\r\n");
                    }
                    GuiInteraction::Seed(seed_interaction) => match seed_interaction {
                        SeedInteraction::EntryCompleted(phrase) => {
                            match bip39_seed_from_phrase(&phrase) {
                                Ok(seed64) => {
                                    pending_seed_setup = Some(seed64);
                                    ui.clear_seed_entry_state();
                                    ui.begin_unlock(Some(4));
                                }
                                Err(()) => {
                                    ui.show_seed_setup();
                                    usb_debug(&mut usb, b"seed derive failed\r\n");
                                }
                            }
                        }
                        SeedInteraction::EntryCancelled => {
                            pending_seed_setup = None;
                        }
                        _ => {}
                    }
                    GuiInteraction::RawTouch(_coord) => {}
                }
            }
        }

        if let Some(outcome) = unlock_controller.poll() {
            if let Some(ui_ref) = ui.as_mut() {
                handle_unlock_outcome(outcome, Some(ui_ref), &mut usb);
            } else {
                handle_unlock_outcome(outcome, None, &mut usb);
            }
        }
        // 1) Proactive outbound frag, if any
        unsafe {
            if let Some(of) = OUT_FRAG.as_mut() {
                let start = of.off as usize;
                let end = core::cmp::min(start + TX_CHUNK, of.data.len());
                let last = end == of.data.len();
                let chunk: alloc::vec::Vec<u8> = of.data[start..end].to_vec();
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
                if let Ok(used) = postcard::to_slice(&resp, &mut plain) {
                    let n = encode(used, &mut enc);
                    let _ = usb_write(&mut usb, &enc[..n]);
                    let _ = usb_write(&mut usb, &[0]);
                }
                of.off = end as u32;
                if last {
                    OUT_FRAG = None;
                }
                // After sending a part, try to send again (or fall through to RX next loop)
                continue;
            }
        }
        // 2) RX path
        loop {
            match usb.read_byte() {
                Ok(b) => {
                    if b == 0 && rx.is_empty() {
                        continue 'main;
                    }
                    if rx.push(b).is_err() {
                        rx.clear();
                        send_err(&mut usb, ERR_OVERFLOW, &mut enc);
                        continue 'main;
                    }
                    if b == 0 {
                        // decode Msg<Frame>
                        let resp_msg = match postcard::from_bytes_cobs::<Msg<Frame>>(rx.as_mut()) {
                            Ok(m) if m.v == PROTO_V1 => {
                                // Check if device is busy with a long operation (PBKDF2, etc.)
                                // Reject all requests except Ping/GetInfo to prevent queue buildup
                                let is_blocking_request = unsafe { DEVICE_BUSY };
                                let is_ping_or_info = matches!(&m.msg,
                                    Frame::One(Request::Ping) | Frame::One(Request::GetInfo));

                                if is_blocking_request && !is_ping_or_info {
                                    Some(Msg {
                                        v: PROTO_V1,
                                        id: m.id,
                                        msg: Response::Err { code: ERR_BUSY },
                                    })
                                } else {
                                    // Show GUI unlock animation if unlock request comes over serial
                                    if let Frame::One(Request::Unlock { .. }) = &m.msg {
                                        if let Some(ui) = ui.as_mut() {
                                            ui.show_unlocking();
                                        }
                                    }

                                    // Show GUI busy animation while initializing the seed/PIN over serial
                                    if let Frame::One(Request::InitializePIN { .. }) = &m.msg {
                                        if let Some(ui) = ui.as_mut() {
                                            ui.show_unlocking();
                                        }
                                    }

                                    // Show GUI lock screen if lock request comes over serial
                                    if let Frame::One(Request::Lock) = &m.msg {
                                        if let Some(ui) = ui.as_mut() {
                                            ui.begin_unlock(None);
                                        }
                                    }

                                    if let Some(prompt) = frame_confirmation_prompt(&m.msg) {
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
                                            Err(()) => Some(Msg {
                                                v: PROTO_V1,
                                                id: m.id,
                                                msg: Response::Err { code: ERR_BUSY },
                                            }),
                                        }
                                    } else {
                                    let ui_ref = ui.as_mut().map(|u| u as &mut Gui);
                                    let body = handle_frame_v1(m.id, &m.msg, ui_ref);

                                    // Show result on GUI after unlock completes
                                    if let Frame::One(Request::Unlock { .. }) = &m.msg {
                                        if let Some(ui) = ui.as_mut() {
                                            match &body {
                                                Response::Ok => ui.show_unlock_success(),
                                                Response::Err { code } if *code == ERR_WRONG_PIN => {
                                                    // Get attempts remaining from lock status
                                                    let mut nvs = NvsStore::new();
                                                    let remaining = nvs.get_attempts_remaining();
                                                    ui.show_pin_failure(if remaining > 0 { Some(remaining) } else { None });
                                                }
                                                Response::Err { code } if *code == ERR_PIN_LOCKED_OUT => {
                                                    ui.show_pin_locked_out();
                                                }
                                                _ => {}
                                            }
                                        }
                                    }

                                    // Show result on GUI after seeding completes
                                    if let Frame::One(Request::InitializePIN { .. }) = &m.msg {
                                        if let Some(ui) = ui.as_mut() {
                                            if matches!(&body, Response::Ok) {
                                                ui.show_unlock_success();
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
                            if let Ok(used) = postcard::to_slice(&resp_msg, &mut plain) {
                                let n = encode(used, &mut enc);
                                let _ = usb_write(&mut usb, &enc[..n]);
                                let _ = usb_write(&mut usb, &[0]);
                            } else {
                                send_err(&mut usb, ERR_ENCODE_TOO_BIG, &mut enc);
                            }
                        }
                        rx.clear();
                        continue 'main;
                    }
                }
                Err(_) => break,
            }
        }
    }
}

fn handle_frame_v1(req_id: u32, frame: &Frame, mut ui: Option<&mut Gui<'_>>) -> Response {
    match frame {
        Frame::One(req) => handle_request_v1(req),
        Frame::FragBegin {
            id,
            total_len,
            kind,
        } => {
            unsafe {
                if (*total_len as usize) > MAX_FRAG_TOTAL {
                    return Response::Err { code: ERR_OVERFLOW };
                }
                FRAG = Some(FragState {
                    id: *id,
                    kind: *kind,
                    total_len: *total_len,
                    next_off: 0,
                    buf: Vec::with_capacity(*total_len as usize),
                });
            }
            Response::Ok
        }
        Frame::FragPart {
            id,
            offset,
            chunk,
            last,
        } => {
            unsafe {
                let Some(st) = FRAG.as_mut() else {
                    return Response::Err {
                        code: ERR_BAD_COBS_OR_POSTCARD,
                    };
                };
                if st.id != *id || st.next_off != *offset {
                    return Response::Err {
                        code: ERR_BAD_COBS_OR_POSTCARD,
                    };
                }
                if st.buf.len() + chunk.len() > (st.total_len as usize) {
                    return Response::Err { code: ERR_OVERFLOW };
                }
                st.buf.extend_from_slice(&chunk);
                st.next_off += chunk.len() as u32;
                if *last {
                    if st.next_off != st.total_len {
                        FRAG = None;
                        return Response::Err {
                            code: ERR_BAD_COBS_OR_POSTCARD,
                        };
                    }
                    // Decide by kind
                    match st.kind {
                        FragKind::SetSeed => {
                            if st.buf.len() != 64 {
                                FRAG = None;
                                return Response::Err {
                                    code: ERR_BAD_COBS_OR_POSTCARD,
                                };
                            }
                            let mut arr = [0u8; 64];
                            arr.copy_from_slice(st.buf.as_slice());
                            set_seed(&arr);
                            FRAG = None;
                            Response::Ok
                        }
                        FragKind::SignDraft => {
                            if is_device_locked() {
                                FRAG = None;
                                return Response::Err {
                                    code: ERR_DEVICE_LOCKED,
                                };
                            }
                            let slot = match active_slot_index() {
                                Ok(idx) => idx,
                                Err(_) => {
                                    FRAG = None;
                                    return Response::Err { code: ERR_NO_SEED };
                                }
                            };
                            let path = pathmod::Path::new();
                            let sk = match derive_child_sk_for_slot(&path, slot) {
                                Ok(sk) => sk,
                                Err(_) => {
                                    FRAG = None;
                                    return Response::Err { code: ERR_NO_SEED };
                                }
                            };
                            let cfg = siger_core::draft_sign::SignerConfig { sk_be: sk };
                            if let Some(ui) = ui.as_mut() {
                                let outputs = match siger_core::draft_sign::draft_outputs_v1(
                                    st.buf.as_slice(),
                                    &cfg,
                                ) {
                                    Ok(v) => v,
                                    Err(siger_core::draft_sign::SignDraftError::Unsupported) => {
                                        FRAG = None;
                                        return Response::Err {
                                            code: ERR_UNSUPPORTED_VERSION,
                                        };
                                    }
                                    Err(_) => {
                                        FRAG = None;
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
                                            sk_be: sk,
                                            draft,
                                        });
                                        true
                                    }
                                });

                                if !stored {
                                    FRAG = None;
                                    return Response::Err { code: ERR_BUSY };
                                }

                                ui.request_tx_review(
                                    outputs
                                        .iter()
                                        .filter(|o| !o.is_refund)
                                        .map(|o| (o.gift, o.recipient_b58.as_str())),
                                );
                                FRAG = None;
                                Response::Ok
                            } else {
                                let out =
                                    match siger_core::draft_sign::sign_draft_v1(st.buf.as_slice(), &cfg)
                                    {
                                        Ok(v) => v,
                                        Err(siger_core::draft_sign::SignDraftError::Unsupported) => {
                                            FRAG = None;
                                            return Response::Err {
                                                code: ERR_UNSUPPORTED_VERSION,
                                            };
                                        }
                                        Err(_) => {
                                            FRAG = None;
                                            return Response::Err {
                                                code: ERR_BAD_COBS_OR_POSTCARD,
                                            };
                                        }
                                    };

                                let total = out.len() as u32;
                                OUT_FRAG = Some(OutFrag {
                                    msg_id: req_id,
                                    id: st.id,
                                    kind: FragKind::SignDraft,
                                    off: 0,
                                    data: out,
                                });
                                FRAG = None;
                                Response::FragBegin {
                                    id: *id,
                                    total_len: total,
                                    kind: FragKind::SignDraft,
                                }
                            }
                        }
                    }
                } else {
                    Response::Ok
                }
            }
        }
    }
}

fn handle_request_v1(req: &Request) -> Response {
    match req {
        Request::Hello => Response::Hello(Caps {
            proto_v: PROTO_V1,
            compressed_pk: true,
        }),
        Request::GetInfo => {
            let mut nvs = NvsStore::new();
            let stored_pubs = nvs.list_seed_pubs().unwrap_or_default();
            let has_seed_persisted = !stored_pubs.is_empty() || nvs.is_initialized();
            let has_seed_ram = unsafe { !SEED_STORE.slots.is_empty() };
            let cheetah_pubs = if stored_pubs.is_empty() && has_seed_ram {
                collect_info_pubs_from_ram()
            } else {
                stored_pubs
            };
            Response::Info {
                proto_v: PROTO_V1,
                fw_major: FW_MAJOR,
                fw_minor: FW_MINOR,
                features: FEAT_CHEETAH | FEAT_FRAG | FEAT_XPUB,
                has_seed: has_seed_persisted || has_seed_ram,
                cheetah_pubs,
            }
        }
        Request::Ping => Response::Pong,
        Request::SetSeed { seed64 } => {
            set_seed(seed64);
            Response::Ok
        }
        Request::Wipe => {
            wipe_seed();
            Response::Ok
        }
        Request::GetFingerprint => match master_fingerprint_for_active() {
            Ok(fp4) => Response::OkFingerprint { fp4 },
            Err(_) => Response::Err { code: ERR_NO_SEED },
        },
        Request::GetPubkey { path, compressed } => match derive_signing_key_active(path) {
            Ok(sk) => {
                let vk = sk.verifying_key();
                if *compressed {
                    let mut out = [0u8; 33];
                    out.copy_from_slice(vk.to_encoded_point(true).as_bytes());
                    Response::OkPubkeyCompressed { compressed: out }
                } else {
                    let mut out = [0u8; 65];
                    out.copy_from_slice(vk.to_encoded_point(false).as_bytes());
                    Response::OkPubkey { uncompressed: out }
                }
            }
            Err(_) => Response::Err { code: ERR_NO_SEED },
        },
        Request::SignDigest { path, digest32 } => {
            if is_device_locked() {
                return Response::Err {
                    code: ERR_DEVICE_LOCKED,
                };
            }
            match derive_signing_key_active(path) {
                Ok(sk) => {
                    let mut sig: Signature = PrehashSigner::sign_prehash(&sk, digest32).unwrap();
                    if let Some(norm) = sig.normalize_s() {
                        sig = norm;
                    }
                    let mut out = [0u8; 64];
                    out.copy_from_slice(&sig.to_bytes());
                    Response::OkSig { sig64: out }
                }
                Err(_) => Response::Err { code: ERR_NO_SEED },
            }
        }
        Request::GetXpub { path } => match get_xpub(path) {
            Ok(x) => Response::OkXpub(x),
            Err(_) => Response::Err { code: ERR_NO_SEED },
        },
        Request::GetCheetahPub { slot, path } => {
            match derive_child_sk_for_slot(path, *slot as usize) {
                Ok(sk) => {
                    let pk = cheetah::cheetah_pub_from_sk(sk);
                    Response::OkCheetahPub { x: pk.0, y: pk.1 }
                }
                Err(_) => Response::Err { code: ERR_NO_SEED },
            }
        }
        Request::SignSpendHash {
            slot,
            path,
            msg5,
            ..
        } => {
            if is_device_locked() {
                return Response::Err {
                    code: ERR_DEVICE_LOCKED,
                };
            }
            match derive_child_sk_for_slot(path, *slot as usize) {
                Ok(sk) => {
                    let pk = cheetah::cheetah_pub_from_sk(sk);
                    let hash = cheetah::Hash { values: *msg5 };
                    let (e, s) = cheetah::schnorr_sign_tx(sk, pk, hash.values);
                    Response::OkCheetahSig {
                        chal: e.values,
                        sig: s.values,
                    }
                }
                Err(_) => Response::Err { code: ERR_NO_SEED },
            }
        }
        Request::SignSpendHashFor {
            slot,
            path,
            msg5,
            pubkey,
            ..
        } => {
            if is_device_locked() {
                return Response::Err {
                    code: ERR_DEVICE_LOCKED,
                };
            }
            match derive_child_sk_for_slot(path, *slot as usize) {
                Ok(sk) => {
                    let pk_dev = cheetah::cheetah_pub_from_sk(sk);
                    if &pk_dev != pubkey {
                        Response::Err {
                            code: ERR_WRONG_PUBKEY,
                        }
                    } else {
                        let hash = cheetah::Hash { values: *msg5 };
                        let (e, s) = cheetah::schnorr_sign_tx(sk, *pubkey, hash.values);
                        Response::OkCheetahSig {
                            chal: e.values,
                            sig: s.values,
                        }
                    }
                }
                Err(_) => Response::Err { code: ERR_NO_SEED },
            }
        }
        Request::Health => {
            let slot = match active_slot_index() {
                Ok(idx) => idx,
                Err(_) => return Response::Err { code: ERR_NO_SEED },
            };
            let path =
                pathmod::Path::from_iter([0x8000_002c, 0x8000_0000, 0x8000_0000, 0, 0].into_iter());
            match derive_child_sk_for_slot(&path, slot) {
                Ok(sk) => {
                    let pk = cheetah::cheetah_pub_from_sk(sk);
                    let hash = cheetah::Hash {
                        values: [0, 0, 0, 0, 0],
                    };
                    let (e, s) = cheetah::schnorr_sign_tx(sk, pk, hash.values);
                    Response::OkCheetahSig {
                        chal: e.values,
                        sig: s.values,
                    }
                }
                Err(_) => Response::Err { code: ERR_NO_SEED },
            }
        }
        Request::InitializePIN { pin, seed64 } => {
            unsafe { DEVICE_BUSY = true; }
            let mut nvs = NvsStore::new();
            let pub_xy = root_pub_from_seed(seed64);
            let result = match nvs.initialize_pin(pin.as_str(), seed64, pub_xy) {
                Ok((master_key, _slot)) => {
                    // Store the master key (already derived during initialize_pin)
                    store_master_key(&master_key);
                    // Also set in RAM for immediate use
                    set_seed(seed64);
                    unsafe {
                        DEVICE_LOCKED = false;
                    }
                    Response::Ok
                }
                Err(NvsError::AlreadyInitialized) => Response::Err {
                    code: ERR_ALREADY_INITIALIZED,
                },
                Err(_) => Response::Err { code: ERR_NO_SEED },
            };
            unsafe { DEVICE_BUSY = false; }
            result
        }
        Request::AddSeed { seed64 } => {
            if is_device_locked() {
                return Response::Err {
                    code: ERR_DEVICE_LOCKED,
                };
            }
            let master_key = match master_key_copy() {
                Some(key) => key,
                None => {
                    return Response::Err {
                        code: ERR_DEVICE_LOCKED,
                    }
                }
            };
            let mut nvs = NvsStore::new();
            let pub_xy = root_pub_from_seed(seed64);
            match nvs.add_seed_with_key(&master_key, seed64, pub_xy) {
                Ok(_) => {
                    append_seed_slot(seed64);
                    Response::Ok
                }
                Err(NvsError::WrongPin) => Response::Err {
                    code: ERR_WRONG_PIN,
                },
                Err(NvsError::LockedOut) => Response::Err {
                    code: ERR_PIN_LOCKED_OUT,
                },
                Err(NvsError::Full) => Response::Err { code: ERR_OVERFLOW },
                Err(_) => Response::Err { code: ERR_NO_SEED },
            }
        }
        Request::DeleteSeed { slot } => {
            if is_device_locked() {
                return Response::Err {
                    code: ERR_DEVICE_LOCKED,
                };
            }
            let master_key = match master_key_copy() {
                Some(key) => key,
                None => {
                    return Response::Err {
                        code: ERR_DEVICE_LOCKED,
                    }
                }
            };
            let mut nvs = NvsStore::new();
            match nvs.delete_seed_with_key(&master_key, *slot as usize) {
                Ok(_) => {
                    remove_seed_slot(*slot as usize);
                    Response::Ok
                }
                Err(NvsError::InvalidSlot) => Response::Err { code: ERR_NO_SEED },
                Err(NvsError::WrongPin) => Response::Err {
                    code: ERR_WRONG_PIN,
                },
                Err(NvsError::LockedOut) => Response::Err {
                    code: ERR_PIN_LOCKED_OUT,
                },
                Err(NvsError::NotInitialized) => Response::Err { code: ERR_NO_SEED },
                Err(_) => Response::Err { code: ERR_NO_SEED },
            }
        }
        Request::Unlock { pin } => {
            // WARNING: This blocks main thread for ~5s during PBKDF2
            // GUI unlock animation will freeze during this time (only for serial unlock)
            // GUI-initiated unlock uses APP_CORE worker and doesn't have this issue
            unsafe { DEVICE_BUSY = true; }
            let result = match unlock_device_with_pin(pin.as_str()) {
                UnlockAttempt::Success => Response::Ok,
                UnlockAttempt::WrongPin { .. } => Response::Err {
                    code: ERR_WRONG_PIN,
                },
                UnlockAttempt::LockedOut => Response::Err {
                    code: ERR_PIN_LOCKED_OUT,
                },
                UnlockAttempt::NotInitialized => Response::Err { code: ERR_NO_SEED },
                UnlockAttempt::Failed => Response::Err { code: ERR_NO_SEED },
            };
            unsafe { DEVICE_BUSY = false; }
            result
        },
        Request::Lock => {
            wipe_seed();
            clear_master_key();
            unsafe {
                DEVICE_LOCKED = true;
            }
            Response::Ok
        }
        Request::ResetPIN {
            current_pin,
            new_pin,
        } => {
            if is_device_locked() {
                return Response::Err {
                    code: ERR_DEVICE_LOCKED,
                };
            }
            let mut nvs = NvsStore::new();
            match nvs.change_pin(current_pin.as_str(), new_pin.as_str()) {
                Ok(_) => match nvs.derive_master_key_for_pin(new_pin.as_str()) {
                    Ok(master_key) => {
                        store_master_key(&master_key);
                        Response::Ok
                    }
                    Err(_) => Response::Err { code: ERR_NO_SEED },
                },
                Err(NvsError::WrongPin) => Response::Err {
                    code: ERR_WRONG_PIN,
                },
                Err(NvsError::LockedOut) => Response::Err {
                    code: ERR_PIN_LOCKED_OUT,
                },
                Err(_) => Response::Err { code: ERR_NO_SEED },
            }
        }
        Request::SelectSeed { slot } => match set_active_slot(*slot as usize) {
            Ok(()) => Response::Ok,
            Err(_) => Response::Err { code: ERR_NO_SEED },
        },
        Request::Reset => {
            wipe_seed();
            clear_master_key();
            let mut nvs = NvsStore::new();
            match nvs.factory_reset() {
                Ok(()) => Response::Ok,
                Err(_) => Response::Err { code: ERR_NO_SEED },
            }
        }
        Request::GetLockStatus => {
            let mut nvs = NvsStore::new();
            let has_seed_in_ram = unsafe { !SEED_STORE.slots.is_empty() };
            let persisted_seed = nvs.is_initialized();
            let locked = if has_seed_in_ram || persisted_seed {
                unsafe { DEVICE_LOCKED }
            } else {
                false
            };
            let attempts_remaining = nvs.get_attempts_remaining();
            Response::OkLockStatus {
                locked,
                attempts_remaining,
            }
        }
    }
}

fn frame_confirmation_prompt(frame: &Frame) -> Option<&'static str> {
    match frame {
        Frame::One(Request::SignDigest { .. }) => Some("Sign digest?"),
        Frame::One(Request::SignSpendHash { .. }) => Some("Approve spend?"),
        Frame::One(Request::SignSpendHashFor { .. }) => Some("Approve spend?"),
        _ => None,
    }
}

fn shorten_b58(s: &str) -> HString<24> {
    let mut out = HString::<24>::new();
    if s.len() <= 12 {
        let _ = out.push_str(s);
    } else {
        let head = &s[..4.min(s.len())];
        let tail = &s[s.len().saturating_sub(4)..];
        let _ = out.push_str(head);
        let _ = out.push_str("...");
        let _ = out.push_str(tail);
    }
    out
}

fn is_device_locked() -> bool {
    unsafe { DEVICE_LOCKED }
}

fn compute_unlock_outcome(pin: &str) -> UnlockOutcome {
    let mut nvs = NvsStore::new();
    match nvs.unlock(pin) {
        Ok((seeds, master_key)) => UnlockOutcome::Success { seeds, master_key },
        Err(NvsError::WrongPin) => {
            let remaining = nvs.get_attempts_remaining();
            if remaining == 0 {
                UnlockOutcome::LockedOut
            } else {
                UnlockOutcome::WrongPin {
                    attempts_remaining: remaining,
                }
            }
        }
        Err(NvsError::LockedOut) => UnlockOutcome::LockedOut,
        Err(NvsError::NotInitialized) => UnlockOutcome::NotInitialized,
        Err(_) => UnlockOutcome::Failed,
    }
}

fn apply_unlock_success(seeds: &[[u8; 64]], master_key: &[u8; 32]) {
    update_seed_store_from_slice(seeds);
    store_master_key(master_key);
    unsafe {
        DEVICE_LOCKED = false;
    }
}

fn unlock_device_with_pin(pin: &str) -> UnlockAttempt {
    unsafe {
        if !DEVICE_LOCKED {
            return UnlockAttempt::Success;
        }
    }

    match compute_unlock_outcome(pin) {
        UnlockOutcome::Success { seeds, master_key } => {
            apply_unlock_success(seeds.as_slice(), &master_key);
            UnlockAttempt::Success
        }
        UnlockOutcome::WrongPin { attempts_remaining } => {
            UnlockAttempt::WrongPin { attempts_remaining }
        }
        UnlockOutcome::LockedOut => UnlockAttempt::LockedOut,
        UnlockOutcome::NotInitialized => UnlockAttempt::NotInitialized,
        UnlockOutcome::Failed => UnlockAttempt::Failed,
    }
}

fn handle_unlock_outcome(
    outcome: UnlockOutcome,
    ui: Option<&mut Gui<'_>>,
    usb: &mut UsbSerialJtag<'_, esp_hal::Blocking>,
) {
    match outcome {
        UnlockOutcome::Success { seeds, master_key } => {
            apply_unlock_success(seeds.as_slice(), &master_key);
            if let Some(ui) = ui {
                ui.show_unlock_success();
            }
            usb_debug(usb, b"unlock success\r\n");
        }
        UnlockOutcome::WrongPin { attempts_remaining } => {
            if let Some(ui) = ui {
                ui.show_pin_failure(Some(attempts_remaining));
            }
            usb_debug(usb, b"wrong pin\r\n");
        }
        UnlockOutcome::LockedOut => {
            if let Some(ui) = ui {
                ui.show_pin_locked_out();
            }
            usb_debug(usb, b"pin locked out\r\n");
        }
        UnlockOutcome::NotInitialized => {
            if let Some(ui) = ui {
                ui.show_pin_not_initialized();
            }
            usb_debug(usb, b"pin not set\r\n");
        }
        UnlockOutcome::Failed => {
            if let Some(ui) = ui {
                ui.show_pin_failure(None);
            }
            usb_debug(usb, b"unlock failed\r\n");
        }
    }
}

fn pk_uncompressed_65(sk: &SigningKey) -> [u8; 65] {
    let ep = sk.verifying_key().to_encoded_point(false);
    let mut out = [0u8; 65];
    out.copy_from_slice(ep.as_bytes());
    out
}

fn pk_compressed_33(sk: &SigningKey) -> [u8; 33] {
    let ep = sk.verifying_key().to_encoded_point(true);
    let mut out = [0u8; 33];
    out.copy_from_slice(ep.as_bytes());
    out
}

fn send_err(usb: &mut UsbSerialJtag<'_, esp_hal::Blocking>, code: u16, enc: &mut [u8]) {
    let msg = Msg {
        v: PROTO_V1,
        id: 0,
        msg: Response::Err { code },
    };
    let mut tmp = [0u8; 64];
    if let Ok(used) = postcard::to_slice(&msg, &mut tmp) {
        let n = cobs::encode(used, enc);
        let _ = usb_write(usb, &enc[..n]);
        let _ = usb_write(usb, &[0]);
    }
}

fn bip39_seed_from_phrase(phrase: &gui::SeedPhrase) -> Result<[u8; 64], ()> {
    if phrase.len() != 24 {
        return Err(());
    }

    // Join to BIP-39 sentence and derive the 64-byte seed via PBKDF2-HMAC-SHA512.
    let mut sentence = HString::<320>::new();
    for (idx, word) in phrase.iter().enumerate() {
        if idx > 0 {
            sentence.push(' ').map_err(|_| ())?;
        }
        sentence.push_str(word.as_str()).map_err(|_| ())?;
    }

    let mut out = [0u8; 64];
    pbkdf2::<Hmac<Sha512>>(sentence.as_str().as_bytes(), b"mnemonic", BIP39_PBKDF2_ROUNDS, &mut out)
        .map_err(|_| ())?;
    Ok(out)
}

fn get_xpub(path: &pathmod::Path) -> Result<Xpub, ()> {
    let seed = get_active_seed_copy()?;
    let dp = path_to_derivation(path);
    let child = XPrv::derive_from_path(&seed, &dp).map_err(|_| ())?;
    let xpub = child.public_key();
    let attrs = child.attrs();
    let depth = attrs.depth;
    let child_u32 = u32::from(attrs.child_number);
    let fp4 = attrs.parent_fingerprint;
    let chain_code = attrs.chain_code;
    let mut pubkey33 = [0u8; 33];
    pubkey33.copy_from_slice(&xpub.public_key().to_bytes());
    Ok(Xpub {
        depth,
        fp4,
        child: child_u32,
        chain_code,
        pubkey33,
    })
}

fn master_key_copy() -> Option<[u8; 32]> {
    unsafe {
        if MASTER_KEY_SET {
            Some(MASTER_KEY)
        } else {
            None
        }
    }
}

fn store_master_key(key: &[u8; 32]) {
    unsafe {
        MASTER_KEY.copy_from_slice(key);
        MASTER_KEY_SET = true;
    }
}

fn clear_master_key() {
    unsafe {
        MASTER_KEY.zeroize();
        MASTER_KEY_SET = false;
    }
}

fn set_seed(seed64: &[u8; 64]) {
    update_seed_store_from_slice(core::slice::from_ref(seed64));
}

fn update_seed_store_from_slice(seeds: &[[u8; 64]]) {
    unsafe {
        SEED_STORE.slots.clear();
        for seed in seeds {
            let _ = SEED_STORE.slots.push(*seed);
        }
        SEED_STORE.active = 0;
        DEVICE_LOCKED = SEED_STORE.slots.is_empty();
    }
}

fn append_seed_slot(seed64: &[u8; 64]) {
    unsafe {
        if SEED_STORE.slots.len() < MAX_SEED_SLOTS {
            let _ = SEED_STORE.slots.push(*seed64);
        }
    }
}

fn remove_seed_slot(index: usize) {
    unsafe {
        if index < SEED_STORE.slots.len() {
            let len = SEED_STORE.slots.len();
            let mut i = index;
            while i + 1 < len {
                SEED_STORE.slots[i] = SEED_STORE.slots[i + 1];
                i += 1;
            }
            let _ = SEED_STORE.slots.pop();
            if SEED_STORE.active >= SEED_STORE.slots.len() {
                SEED_STORE.active = SEED_STORE.slots.len().saturating_sub(1);
            }
            DEVICE_LOCKED = SEED_STORE.slots.is_empty();
        }
    }
}

fn wipe_seed() {
    unsafe {
        SEED_STORE.slots.clear();
        SEED_STORE.active = 0;
        DEVICE_LOCKED = true;
    }
    clear_master_key();
}

fn collect_info_pubs() -> alloc::vec::Vec<CheetahPub> {
    let mut nvs = NvsStore::new();
    match nvs.list_seed_pubs() {
        Ok(pubs) if !pubs.is_empty() => pubs,
        _ => collect_info_pubs_from_ram(),
    }
}

fn collect_info_pubs_from_ram() -> alloc::vec::Vec<CheetahPub> {
    let mut out = alloc::vec::Vec::new();
    unsafe {
        for (idx, seed) in SEED_STORE.slots.iter().enumerate() {
            let pub_xy = root_pub_from_seed(seed);
            let path = pathmod::Path::new();
            out.push(CheetahPub {
                slot: idx as u8,
                path,
                x: pub_xy.0,
                y: pub_xy.1,
            });
        }
    }
    out
}

fn path_to_derivation(path: &pathmod::Path) -> DerivationPath {
    let mut dp = DerivationPath::default();
    for &p in path.iter() {
        let hardened = (p & 0x8000_0000) != 0;
        let idx = p & 0x7FFF_FFFF;
        dp.push(ChildNumber::new(idx, hardened).unwrap());
    }
    dp
}

fn derive_signing_key_for_slot(path: &pathmod::Path, slot: usize) -> Result<SigningKey, ()> {
    let seed = get_seed_for_slot(slot)?;
    let mut key = XPrv::new(&seed).map_err(|_| ())?;
    for index in path.iter() {
        let child_num = ChildNumber::from(*index);
        key = key.derive_child(child_num).map_err(|_| ())?;
    }
    let sk_bytes = key.private_key().to_bytes();
    SigningKey::from_bytes((&sk_bytes).into()).map_err(|_| ())
}

fn master_fingerprint_for_active() -> Result<[u8; 4], ()> {
    let seed = get_active_seed_copy()?;
    let xprv = XPrv::new(&seed).map_err(|_| ())?;
    let xpub = xprv.public_key();
    let comp = xpub.public_key().to_bytes();
    let sha = Sha256::digest(&comp);
    let ripe = Ripemd160::digest(&sha);
    let mut fp4 = [0u8; 4];
    fp4.copy_from_slice(&ripe[..4]);
    Ok(fp4)
}

fn derive_child_sk_for_slot(path: &pathmod::Path, slot: usize) -> Result<[u8; 32], ()> {
    let seed = get_seed_for_slot(slot)?;
    let (sk, cc) = cheetah::master_from_seed(&seed);
    let mut xk = cheetah::XKey::from_master(sk, cc);
    for &i in path.iter() {
        xk = cheetah::xprv_derive_child(&xk, i);
    }
    xk.sk.ok_or(())
}

fn root_pub_from_seed(seed: &[u8; 64]) -> ([u64; 6], [u64; 6]) {
    let (sk, _cc) = cheetah::master_from_seed(seed);
    cheetah::cheetah_pub_from_sk(sk)
}

fn get_seed_for_slot(slot: usize) -> Result<[u8; 64], ()> {
    unsafe {
        if slot >= SEED_STORE.slots.len() {
            return Err(());
        }
        Ok(SEED_STORE.slots[slot])
    }
}

fn get_active_seed_copy() -> Result<[u8; 64], ()> {
    unsafe {
        if SEED_STORE.slots.is_empty() {
            return Err(());
        }
        let idx = SEED_STORE.active.min(SEED_STORE.slots.len() - 1);
        Ok(SEED_STORE.slots[idx])
    }
}

fn set_active_slot(slot: usize) -> Result<(), ()> {
    unsafe {
        if slot >= SEED_STORE.slots.len() {
            return Err(());
        }
        SEED_STORE.active = slot;
    }
    Ok(())
}

fn active_slot_index() -> Result<usize, ()> {
    unsafe {
        if SEED_STORE.slots.is_empty() {
            return Err(());
        }
        Ok(SEED_STORE.active.min(SEED_STORE.slots.len() - 1))
    }
}

fn derive_signing_key_active(path: &pathmod::Path) -> Result<SigningKey, ()> {
    let slot = active_slot_index()?;
    derive_signing_key_for_slot(path, slot)
}
