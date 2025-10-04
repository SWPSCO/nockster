#![no_std]
#![no_main]
#![deny(clippy::mem_forget, reason = "unsafe for esp-hal types")]

mod random;
use siger_fw::nvs_store::{NvsError, NvsStore};
use panic_halt as _;
extern crate alloc;
use cobs::encode;
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_hal::{clock::CpuClock, main};
use heapless::Vec as HVec;
use heapless::Vec;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use siger_core::alloc_path as pathmod;
use siger_core::*;

use bip32::{ChildNumber, DerivationPath, PublicKey, XPrv};
use k256::{
    ecdsa::{signature::hazmat::PrehashSigner, Signature, SigningKey},
    EncodedPoint,
};
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

struct SeedStore {
    set: bool,
    seed: [u8; 64],
}
static mut SEED_STORE: SeedStore = SeedStore {
    set: false,
    seed: [0u8; 64],
};

// Lock state (device is locked by default)
static mut DEVICE_LOCKED: bool = true;

esp_bootloader_esp_idf::esp_app_desc!();

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

    let mut usb = UsbSerialJtag::new(p.USB_DEVICE);
    let _ = usb.write(b"siger-fw ready\r\n");

    // Bigger working buffers to accommodate TX_CHUNK
    let mut rx: HVec<u8, 512> = HVec::new();
    let mut plain = [0u8; 512];
    let mut enc = [0u8; 640];

    'run: loop {
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
                send_err(&mut usb, ERR_OVERFLOW, &mut enc);
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
            let (has_seed, cheetah_x, cheetah_y) = unsafe {
                if SEED_STORE.set {
                    let (sk, _cc) = cheetah::master_from_seed(&SEED_STORE.seed);
                    let pk = cheetah::cheetah_pub_from_sk(sk);
                    (true, pk.0, pk.1)
                } else {
                    (false, [0u64; 6], [0u64; 6])
                }
            };

            Response::Info {
                proto_v: PROTO_V1,
                fw_major: FW_MAJOR,
                fw_minor: FW_MINOR,
                features: FEAT_CHEETAH | FEAT_FRAG | FEAT_XPUB,
                has_seed,
                cheetah_x,
                cheetah_y,
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

        Request::GetFingerprint => match master_fingerprint() {
            Ok(fp4) => Response::OkFingerprint { fp4 },
            Err(_) => Response::Err { code: ERR_NO_SEED },
        },

        Request::GetPubkey { path, compressed } => match derive_signing_key(path) {
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
            match derive_signing_key(path) {
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

        Request::GetCheetahPub { path } => match derive_child_sk_from_seed_store(path) {
            Ok(sk) => {
                let pk = cheetah::cheetah_pub_from_sk(sk);
                Response::OkCheetahPub { x: pk.0, y: pk.1 }
            }
            Err(_) => Response::Err { code: ERR_NO_SEED },
        },

        Request::SignSpendHash { path, msg5 } => {
            if is_device_locked() {
                return Response::Err {
                    code: ERR_DEVICE_LOCKED,
                };
            }
            match derive_child_sk_from_seed_store(path) {
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

        Request::SignSpendHashFor { path, msg5, pubkey } => {
            if is_device_locked() {
                return Response::Err {
                    code: ERR_DEVICE_LOCKED,
                };
            }
            match derive_child_sk_from_seed_store(path) {
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
            let path =
                pathmod::Path::from_iter([0x8000_002c, 0x8000_0000, 0x8000_0000, 0, 0].into_iter());
            match derive_child_sk_from_seed_store(&path) {
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
            match nvs.initialize_pin(pin, seed64) {
                Ok(_) => {
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

        Request::Unlock { pin } => {
            unsafe {
                if !DEVICE_LOCKED {
                    return Response::Ok; // Already unlocked
                }
            }

            let mut nvs = NvsStore::new();
            match nvs.unlock(pin) {
                Ok(seed) => {
                    set_seed(&seed);
                    unsafe {
                        DEVICE_LOCKED = false;
                    }
                    Response::Ok
                }
                Err(NvsError::WrongPin) => {
                    let remaining = nvs.get_attempts_remaining();
                    if remaining == 0 {
                        Response::Err {
                            code: ERR_PIN_LOCKED_OUT,
                        }
                    } else {
                        Response::Err { code: ERR_WRONG_PIN }
                    }
                }
                Err(NvsError::LockedOut) => Response::Err {
                    code: ERR_PIN_LOCKED_OUT,
                },
                Err(NvsError::NotInitialized) => Response::Err { code: ERR_NO_SEED },
                Err(_) => Response::Err { code: ERR_NO_SEED },
            }
        }

        Request::Lock => {
            wipe_seed();
            unsafe {
                DEVICE_LOCKED = true;
            }
            Response::Ok
        }

        Request::ChangePIN { old_pin, new_pin } => {
            let mut nvs = NvsStore::new();
            match nvs.change_pin(old_pin, new_pin) {
                Ok(_) => Response::Ok,
                Err(NvsError::WrongPin) => Response::Err { code: ERR_WRONG_PIN },
                Err(NvsError::LockedOut) => Response::Err {
                    code: ERR_PIN_LOCKED_OUT,
                },
                Err(_) => Response::Err { code: ERR_NO_SEED },
            }
        }

        Request::GetLockStatus => {
            let mut nvs = NvsStore::new();
            let locked = unsafe { DEVICE_LOCKED };
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

fn send_err(usb: &mut UsbSerialJtag<'_, esp_hal::Blocking>, code: u16, enc: &mut [u8; 640]) {
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
    unsafe {
        if !SEED_STORE.set {
            return Err(());
        }
        let dp = path_to_derivation(path);
        let child = XPrv::derive_from_path(&SEED_STORE.seed, &dp).map_err(|_| ())?;
        let xpub = child.public_key();

        // Get attributes
        let attrs = child.attrs();
        let depth = attrs.depth;
        let child_u32 = u32::from(attrs.child_number);
        let fp4 = attrs.parent_fingerprint;
        let chain_code = attrs.chain_code;

        // Get compressed pubkey
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
}

fn set_seed(seed64: &[u8; 64]) {
    unsafe {
        SEED_STORE.seed.copy_from_slice(seed64);
        SEED_STORE.set = true;
    }
}

fn wipe_seed() {
    unsafe {
        SEED_STORE.seed.zeroize();
        SEED_STORE.set = false;
    }
}

/// convert MSB=hard u32s into a bip32 DerivationPath
fn path_to_derivation(path: &pathmod::Path) -> DerivationPath {
    let mut dp = DerivationPath::default();
    for &p in path.iter() {
        let hardened = (p & 0x8000_0000) != 0;
        let idx = p & 0x7FFF_FFFF;
        dp.push(ChildNumber::new(idx, hardened).unwrap());
    }
    dp
}

/// create k256 SigningKey from master seed + path
fn derive_signing_key(path: &pathmod::Path) -> Result<SigningKey, ()> {
    unsafe {
        if !SEED_STORE.set {
            return Err(());
        }
        let xprv = XPrv::new(&SEED_STORE.seed).map_err(|_| ())?;

        // derive child by child
        let mut key = xprv;
        for index in path.iter() {
            let child_num = ChildNumber::from(*index);
            key = key.derive_child(child_num).map_err(|_| ())?;
        }

        let sk_bytes = key.private_key().to_bytes();
        let sk = SigningKey::from_bytes((&sk_bytes).into()).map_err(|_| ())?;
        Ok(sk)
    }
}

/// bip32 parent fingerprint (master): first 4 bytes of RIPEMD160(SHA256(compressed pubkey))
fn master_fingerprint() -> Result<[u8; 4], ()> {
    unsafe {
        if !SEED_STORE.set {
            return Err(());
        }
        let xprv = XPrv::new(&SEED_STORE.seed).map_err(|_| ())?;
        let xpub = xprv.public_key();
        let comp = xpub.public_key().to_bytes(); // compressed 33 bytes
        let sha = Sha256::digest(&comp);
        let ripe = Ripemd160::digest(&sha);
        let mut fp4 = [0u8; 4];
        fp4.copy_from_slice(&ripe[..4]);
        Ok(fp4)
    }
}

fn derive_child_sk_from_seed_store(path: &pathmod::Path) -> Result<[u8; 32], ()> {
    unsafe {
        if !SEED_STORE.set {
            return Err(());
        }
        // SLIP-10 master for Cheetah:
        let (sk, cc) = cheetah::master_from_seed(&SEED_STORE.seed);
        let mut xk = cheetah::XKey::from_master(sk, cc);
        for &i in path.iter() {
            xk = cheetah::xprv_derive_child(&xk, i);
        }
        xk.sk.ok_or(())
    }
}
