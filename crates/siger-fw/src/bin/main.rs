#![no_std]
#![no_main]
#![deny(clippy::mem_forget, reason = "unsafe for esp-hal types")]

mod random;
use panic_halt as _;
extern crate alloc;
use cobs::encode;
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_hal::{clock::CpuClock, main};
use serde::{Deserialize, Serialize};
use heapless::Vec as HVec;
use serde_big_array::BigArray;
use heapless::Vec;

use siger_core::*;
use siger_core::alloc_path as pathmod;

use bip32::{DerivationPath, XPrv, ChildNumber, PublicKey};
use k256::{EncodedPoint, ecdsa::{SigningKey, signature::hazmat::PrehashSigner, Signature}};
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

struct SeedStore {
    set: bool,
    seed: [u8; 64],
}
static mut SEED_STORE: SeedStore = SeedStore { set: false, seed: [0u8; 64] };

// Required by the ESP-IDF bootloader
esp_bootloader_esp_idf::esp_app_desc!();
const DEMO_SK: [u8; 32] = [0x11; 32];

#[main]
fn main() -> ! {
    let cfg = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let p = esp_hal::init(cfg);
    esp_alloc::heap_allocator!(size: 64 * 1024);

    let mut usb = UsbSerialJtag::new(p.USB_DEVICE);
    let _ = usb.write(b"siger-fw v1 online\r\n");

    // Reusable buffers
    let mut rx: HVec<u8, 256> = HVec::new();
    let mut plain = [0u8; 256];
    let mut enc   = [0u8; 300];

    'run: loop {
        if let Ok(b) = usb.read_byte() {
            // Resync-friendly: ignore leading delimiters
            if b == 0 && rx.is_empty() {
                continue;
            }

            if rx.push(b).is_err() {
                // Overflow before delimiter — drop frame and notify
                rx.clear();
                send_err(&mut usb, ERR_OVERFLOW, &mut enc);
                continue;
            }

            if b == 0 {
                // Decode Msg<Request> from COBS (expects trailing 0x00 present)
                let resp_msg = match postcard::from_bytes_cobs::<Msg<Request>>(rx.as_mut()) {
                    Ok(m) if m.v == PROTO_V1 => {
                        let body = handle_request_v1(&m.msg);
                        Msg { v: PROTO_V1, id: m.id, msg: body }
                    }
                    Ok(_m) => Msg { v: PROTO_V1, id: 0, msg: Response::Err { code: ERR_UNSUPPORTED_VERSION } },
                    Err(_) => Msg { v: PROTO_V1, id: 0, msg: Response::Err { code: ERR_BAD_COBS_OR_POSTCARD } },
                };

                // Serialize + COBS + 0x00
                match postcard::to_slice(&resp_msg, &mut plain) {
                    Ok(used) => {
                        let n = encode(used, &mut enc);
                        let _ = usb.write(&enc[..n]);
                        let _ = usb.write(&[0]);
                    }
                    Err(_) => {
                        send_err(&mut usb, ERR_ENCODE_TOO_BIG, &mut enc);
                    }
                }

                rx.clear();
            }
        }
    }
}

fn pk_uncompressed_65(sk: &SigningKey) -> [u8;65] {
  let ep = sk.verifying_key().to_encoded_point(false);
  let mut out = [0u8;65];
  out.copy_from_slice(ep.as_bytes());
  out
}

fn pk_compressed_33(sk: &SigningKey) -> [u8;33] {
  let ep = sk.verifying_key().to_encoded_point(true);
  let mut out = [0u8;33];
  out.copy_from_slice(ep.as_bytes());
  out
}

fn handle_request_v1(req: &Request) -> Response {
  match req {
      Request::Hello =>
          Response::Hello(Caps { proto_v: PROTO_V1, compressed_pk: true }),

      Request::Ping => Response::Pong,

      Request::SetSeed { seed64 } => {
          set_seed(seed64);
          Response::Ok
      }

      Request::Wipe => {
          wipe_seed();
          Response::Ok
      }

      Request::GetFingerprint => {
          match master_fingerprint() {
              Ok(fp4) => Response::OkFingerprint { fp4 },
              Err(_)  => Response::Err { code: ERR_NO_SEED },
          }
      }

      Request::GetPubkey { path, compressed } => {
          match derive_signing_key(path) {
              Ok(sk) => {
                  let vk = sk.verifying_key();
                  if *compressed {
                      let ep = vk.to_encoded_point(true);
                      let mut out = [0u8; 33];
                      out.copy_from_slice(ep.as_bytes());
                      Response::OkPubkeyCompressed { compressed: out }
                  } else {
                      let ep = vk.to_encoded_point(false);
                      let mut out = [0u8; 65];
                      out.copy_from_slice(ep.as_bytes());
                      Response::OkPubkey { uncompressed: out }
                  }
              }
              Err(_) => Response::Err { code: ERR_NO_SEED },
          }
      }

      Request::SignDigest { path, digest32 } => {
          match derive_signing_key(path) {
              Ok(sk) => {
                  let mut sig: Signature =
                      PrehashSigner::sign_prehash(&sk, digest32).unwrap();
                  if let Some(norm) = sig.normalize_s() { sig = norm; }
                  let mut out = [0u8; 64];
                  out.copy_from_slice(&sig.to_bytes());
                  Response::OkSig { sig64: out }
              }
              Err(_) => Response::Err { code: ERR_NO_SEED },
          }
      }

      Request::GetXpub { path } => {
          match get_xpub(path) {
              Ok(x) => Response::OkXpub(x),
              Err(_) => Response::Err { code: ERR_NO_SEED },
          }
      }

      Request::GetCheetahPub { path } => {
          match derive_child_sk_from_seed_store(path) {
              Ok(sk) => {
                  let pk = cheetah::cheetah_pub_from_sk(sk);
                  Response::OkCheetahPub { x: pk.0, y: pk.1 }
              }
              Err(_) => Response::Err { code: ERR_NO_SEED },
          }
      }

      Request::SignTxId { path, txid5 } => {
          match derive_child_sk_from_seed_store(path) {
              Ok(sk) => {
                  let pk = cheetah::cheetah_pub_from_sk(sk);
                  let hash = cheetah::Hash { values: *txid5 };
                  let (e, s) = cheetah::schnorr_sign_txid(sk, pk, hash);
                  Response::OkCheetahSig { chal: e.values, sig: s.values }
              }
              Err(_) => Response::Err { code: ERR_NO_SEED },
          }
      }
      Request::Health => {
          // sign well-known digest with m/44'/0'/0'/0/0
          let path = pathmod::Path::from_iter([0x8000002c, 0x80000000, 0x80000000, 0, 0].into_iter());
          let digest32 = [0u8; 32];
          match derive_signing_key(&path) {
              Ok(sk) => {
                  let mut sig: Signature = k256::ecdsa::signature::hazmat::PrehashSigner::sign_prehash(&sk, &digest32).unwrap();
                  if let Some(norm) = sig.normalize_s() { sig = norm; }
                  let mut out = [0u8; 64];
                  out.copy_from_slice(&sig.to_bytes());
                  Response::OkSig { sig64: out }
              }
              Err(_) => Response::Err { code: ERR_NO_SEED },
          }
      }
  }
}

fn signing_key_demo() -> k256::ecdsa::SigningKey {
  k256::ecdsa::SigningKey::from_bytes((&DEMO_SK).into()).unwrap()
}


// Frame Response::Err quickly without allocating `plain`
fn send_err(usb: &mut UsbSerialJtag<'_, esp_hal::Blocking>, code: u16, enc: &mut [u8; 300]) {
    let msg = Msg { v: PROTO_V1, id: 0, msg: Response::Err { code } };
    let mut tmp = [0u8; 64];
    if let Ok(used) = postcard::to_slice(&msg, &mut tmp) {
        let n = cobs::encode(used, enc);
        let _ = usb.write(&enc[..n]);
        let _ = usb.write(&[0]);
    }
}

fn get_xpub(path: &pathmod::Path) -> Result<Xpub, ()> {
  unsafe {
      if !SEED_STORE.set { return Err(()); }
      let dp = path_to_derivation(path);  // Define dp first
      let child = XPrv::derive_from_path(&SEED_STORE.seed, &dp).map_err(|_| ())?;
      let xpub = child.public_key();

      // Get attributes
      let attrs = child.attrs();
      let depth = attrs.depth;
      let child_u32 = u32::from(attrs.child_number);
      let fp4 = attrs.parent_fingerprint;
      let chain_code = attrs.chain_code;

      // Get compressed pubkey
      let mut pubkey33 = [0u8;33];
      pubkey33.copy_from_slice(&xpub.public_key().to_bytes());

      Ok(Xpub { depth, fp4, child: child_u32, chain_code, pubkey33 })
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

/// Convert our u32s with MSB=hard flag into a bip32 DerivationPath
fn path_to_derivation(path: &pathmod::Path) -> DerivationPath {
  let mut dp = DerivationPath::default();
  for &p in path.iter() {
      let hardened = (p & 0x8000_0000) != 0;
      let idx = p & 0x7FFF_FFFF;
      dp.push(ChildNumber::new(idx, hardened).unwrap());
  }
  dp
}

/// Create a k256 SigningKey from master seed + path
fn derive_signing_key(path: &pathmod::Path) -> Result<SigningKey, ()> {
  unsafe {
      if !SEED_STORE.set { return Err(()); }
      let xprv = XPrv::new(&SEED_STORE.seed).map_err(|_| ())?;
      
      // Derive child by child
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
/// BIP32 parent fingerprint (master): first 4 bytes of RIPEMD160(SHA256(compressed pubkey))
fn master_fingerprint() -> Result<[u8;4], ()> {
    unsafe {
        if !SEED_STORE.set { return Err(()); }
        let xprv = XPrv::new(&SEED_STORE.seed).map_err(|_| ())?;
        let xpub = xprv.public_key();
        let comp = xpub.public_key().to_bytes(); // compressed 33 bytes
        let sha = Sha256::digest(&comp);
        let ripe = Ripemd160::digest(&sha);
        let mut fp4 = [0u8;4];
        fp4.copy_from_slice(&ripe[..4]);
        Ok(fp4)
    }
}

fn derive_child_sk_from_seed_store(path: &pathmod::Path) -> Result<[u8; 32], ()> {
    unsafe {
        if !SEED_STORE.set { return Err(()); }
        // SLIP-10 master for Cheetah:
        let (sk, cc) = cheetah::master_from_seed(&SEED_STORE.seed);
        let mut xk = cheetah::XKey::from_master(sk, cc);
        for &i in path.iter() {
            xk = cheetah::xprv_derive_child(&xk, i);
        }
        xk.sk.ok_or(())
    }
}
