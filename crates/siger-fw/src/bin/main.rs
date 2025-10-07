#![no_std]
#![no_main]
#![deny(clippy::mem_forget, reason = "unsafe for esp-hal types")]
mod gui;
mod touch;
mod random;
use panic_halt as _;
use siger_fw::nvs_store::{NvsError, NvsStore};
extern crate alloc;
use cobs::encode;
use core::fmt::Write as _;
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_hal::{clock::CpuClock, delay::Delay, main};
use gui::{Gui, GuiInteraction};
use touch::{TapFilter, Rotation, TouchCal};
use heapless::{String as HString, Vec as HVec};
use siger_core::alloc_path as pathmod;
use siger_core::{CheetahPub, *};
use bip32::{ChildNumber, DerivationPath, PublicKey, XPrv};
use k256::ecdsa::{signature::hazmat::PrehashSigner, Signature, SigningKey};
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};
use zeroize::Zeroize;
const DEMO_SK: [u8; 32] = [0x11; 32];
const FW_MAJOR: u16 = 0;
const FW_MINOR: u16 = 1;
// feature masks
const FEAT_CHEETAH: u32 = 1 << 0;
const FEAT_FRAG: u32 = 1 << 1;
const FEAT_XPUB: u32 = 1 << 2;
const MAX_FRAG: usize = 4096; // arbitrary
const TX_CHUNK: usize = 200;
const PLAIN_BUF_LEN: usize = 4096;
const ENC_BUF_LEN: usize = cobs::max_encoding_length(PLAIN_BUF_LEN) + 1;
// lock state (locked by default)
static mut DEVICE_LOCKED: bool = true;
static mut MASTER_KEY: [u8; 32] = [0; 32];
static mut MASTER_KEY_SET: bool = false;
esp_bootloader_esp_idf::esp_app_desc!();
// this is necessary because if there's no serial console and you try writing
// to the console, it will block and the device will appear to be frozen.
// this shit was really annoying!
#[inline]
fn usb_write(usb: &mut UsbSerialJtag<'_, esp_hal::Blocking>, buf: &[u8]) {
    for &byte in buf {
        let _ = usb.write_byte_nb(byte);
    }
    let _ = usb.flush_tx_nb();
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
    buf: HVec<u8, MAX_FRAG>,
}
static mut FRAG: Option<FragState> = None;
// outbound fragment queue
struct OutFrag {
    msg_id: u32,
    id: u16,
    kind: FragKind,
    off: u32,
    data: HVec<u8, MAX_FRAG>,
}
static mut OUT_FRAG: Option<OutFrag> = None;
enum UnlockAttempt {
    Success,
    WrongPin { attempts_remaining: u8 },
    LockedOut,
    NotInitialized,
    Failed,
}

/// cobs test
#[cfg(test)]
pub fn handle_one_frame_cobs(frame: &[u8]) -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec::Vec::new();
    match postcard::from_bytes_cobs::<Msg<Frame>>(frame) {
        Ok(m) if m.v == PROTO_V1 => {
            let body = handle_frame_v1(m.id, &m.msg);
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
    }
    let mut usb = UsbSerialJtag::new(p.USB_DEVICE);
    if usb.read_byte().is_ok() {
        let _ = usb_write(&mut usb, b"siger-fw ready\r\n");
    }
    if ui.is_none() {
        let _ = usb_write(&mut usb, b"gui init failed\r\n");
    }
    // Bigger working buffers to accommodate TX_CHUNK
    let mut rx: HVec<u8, 512> = HVec::new();
    let mut plain = [0u8; PLAIN_BUF_LEN];
    let mut enc = [0u8; ENC_BUF_LEN];
    loop {
        if let Some(ui) = ui.as_mut() {
            if let Some((raw_x, raw_y)) = ui.take_debug_touch_raw() {
                let mut msg = HString::<40>::new();
                let _ = write!(msg, "raw {},{}\r\n", raw_x, raw_y);
                let _ = usb_write(&mut usb, msg.as_bytes());
            }
            let event = ui.tick();
            if let Some(result) = ui.poll_confirmation_result() {
                if result {
                    let _ = usb_write(&mut usb, b"confirm accepted\r\n");
                } else {
                    let _ = usb_write(&mut usb, b"confirm rejected\r\n");
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
                        ui.show_unlocking();
                        match unlock_device_with_pin(pin.as_str()) {
                            UnlockAttempt::Success => {
                                ui.show_unlock_success();
                                let _ = usb_write(&mut usb, b"unlock success\r\n");
                            }
                            UnlockAttempt::WrongPin { attempts_remaining } => {
                                ui.show_pin_failure(Some(attempts_remaining));
                                let _ = usb_write(&mut usb, b"wrong pin\r\n");
                            }
                            UnlockAttempt::LockedOut => {
                                ui.show_pin_locked_out();
                                let _ = usb_write(&mut usb, b"pin locked out\r\n");
                            }
                            UnlockAttempt::NotInitialized => {
                                ui.show_pin_not_initialized();
                                let _ = usb_write(&mut usb, b"pin not set\r\n");
                            }
                            UnlockAttempt::Failed => {
                                ui.show_pin_failure(None);
                                let _ = usb_write(&mut usb, b"unlock failed\r\n");
                            }
                        }
                    }
                    GuiInteraction::ConfirmAccepted => {
                        let _ = usb_write(&mut usb, b"confirm accepted\r\n");
                    }
                    GuiInteraction::ConfirmRejected => {
                        let _ = usb_write(&mut usb, b"confirm rejected\r\n");
                    }
                    GuiInteraction::RawTouch(coord) => {
                        let mut msg = HString::<32>::new();
                        let _ = write!(msg, "touch {},{}\r\n", coord.x, coord.y);
                        let _ =  usb_write(&mut usb, msg.as_bytes());
                    }
                }
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
                    let _ = usb.write(&enc[..n]);
                    let _ = usb.write(&[0]);
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
        if let Ok(b) = usb.read_byte() {
            if b == 0 && rx.is_empty() {
                continue;
            }
        if rx.push(b).is_err() {
            rx.clear();
            // Only send error if connected - avoid blocking
            if usb_connected(&mut usb) {
                send_err(&mut usb, ERR_OVERFLOW, &mut enc);
            }
            continue;
        }
            if b == 0 {
                // decode Msg<Frame>
                let resp_msg = match postcard::from_bytes_cobs::<Msg<Frame>>(rx.as_mut()) {
                    Ok(m) if m.v == PROTO_V1 => {
                        let body = handle_frame_v1(m.id, &m.msg);
                        Msg {
                            v: PROTO_V1,
                            id: m.id,
                            msg: body,
                        }
                    }
                    Ok(_) => Msg {
                        v: PROTO_V1,
                        id: 0,
                        msg: Response::Err {
                            code: ERR_UNSUPPORTED_VERSION,
                        },
                    },
                    Err(_) => Msg {
                        v: PROTO_V1,
                        id: 0,
                        msg: Response::Err {
                            code: ERR_BAD_COBS_OR_POSTCARD,
                        },
                    },
                };
                if let Ok(used) = postcard::to_slice(&resp_msg, &mut plain) {
                    let n = encode(used, &mut enc);
                    let _ = usb.write(&enc[..n]);
                    let _ = usb.write(&[0]);
                } else {
                    send_err(&mut usb, ERR_ENCODE_TOO_BIG, &mut enc);
                }
                rx.clear();
            }
        }
    }
}
fn handle_frame_v1(req_id: u32, frame: &Frame) -> Response {
    match frame {
        Frame::One(req) => handle_request_v1(req),
        Frame::FragBegin {
            id,
            total_len,
            kind,
        } => {
            unsafe {
                FRAG = Some(FragState {
                    id: *id,
                    kind: *kind,
                    total_len: *total_len,
                    next_off: 0,
                    buf: HVec::new(),
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
                if st.buf.extend_from_slice(&chunk).is_err() {
                    return Response::Err { code: ERR_OVERFLOW };
                }
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
                            // For now: echo the draft back (wire-up). Replace with actual signing.
                            let mut out = HVec::<u8, MAX_FRAG>::new();
                            let _ = out.extend_from_slice(st.buf.as_slice());
                            let total = out.len() as u32;
                            OUT_FRAG = Some(OutFrag {
                                msg_id: req_id,
                                id: st.id,
                                kind: FragKind::SignDraft,
                                off: 0,
                                data: out,
                            });
                            FRAG = None;
                            // Kick off outbound stream with FragBegin
                            Response::FragBegin {
                                id: *id,
                                total_len: total,
                                kind: FragKind::SignDraft,
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
        Request::SignSpendHash { slot, path, msg5 } => {
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
            let mut nvs = NvsStore::new();
            let pub_xy = root_pub_from_seed(seed64);
            match nvs.initialize_pin(pin.as_str(), seed64, pub_xy) {
                Ok(_) => {
                    match nvs.derive_master_key_for_pin(pin.as_str()) {
                        Ok(master_key) => store_master_key(&master_key),
                        Err(_) => return Response::Err { code: ERR_NO_SEED },
                    }
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
            }
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
        Request::Unlock { pin } => match unlock_device_with_pin(pin.as_str()) {
            UnlockAttempt::Success => Response::Ok,
            UnlockAttempt::WrongPin { .. } => Response::Err {
                code: ERR_WRONG_PIN,
            },
            UnlockAttempt::LockedOut => Response::Err {
                code: ERR_PIN_LOCKED_OUT,
            },
            UnlockAttempt::NotInitialized => Response::Err { code: ERR_NO_SEED },
            UnlockAttempt::Failed => Response::Err { code: ERR_NO_SEED },
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
fn is_device_locked() -> bool {
    unsafe { DEVICE_LOCKED }
}
fn unlock_device_with_pin(pin: &str) -> UnlockAttempt {
    unsafe {
        if !DEVICE_LOCKED {
            return UnlockAttempt::Success;
        }
    }
    let mut nvs = NvsStore::new();
    match nvs.unlock(pin) {
        Ok((seeds, master_key)) => {
            update_seed_store_from_slice(seeds.as_slice());
            store_master_key(&master_key);
            unsafe {
                DEVICE_LOCKED = false;
            }
            UnlockAttempt::Success
        }
        Err(NvsError::WrongPin) => {
            let remaining = nvs.get_attempts_remaining();
            if remaining == 0 {
                UnlockAttempt::LockedOut
            } else {
                UnlockAttempt::WrongPin {
                    attempts_remaining: remaining,
                }
            }
        }
        Err(NvsError::LockedOut) => UnlockAttempt::LockedOut,
        Err(NvsError::NotInitialized) => UnlockAttempt::NotInitialized,
        Err(_) => UnlockAttempt::Failed,
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
fn signing_key_demo() -> k256::ecdsa::SigningKey {
    k256::ecdsa::SigningKey::from_bytes((&DEMO_SK).into()).unwrap()
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
        let _ = usb.write(&enc[..n]);
        let _ = usb.write(&[0]);
    }
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
fn usb_connected(usb: &mut UsbSerialJtag<'_, esp_hal::Blocking>) -> bool {
    usb.read_byte().is_ok()
}