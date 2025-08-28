mod address;

use address::*;
use anyhow::{Context, Result, anyhow};
use postcard::{from_bytes_cobs, to_slice};
use std::{env, io::Read, io::Write, time::Duration, thread};
use siger_core::{Msg, Request, Response, PROTO_V1};
use k256::{EncodedPoint};
use sha2::{Digest, Sha256};
use k256::ecdsa::{signature::hazmat::PrehashVerifier, Signature, VerifyingKey};
use bip39::{Language, Mnemonic};
use num_integer::Integer;
use tx_types::transaction_types::*;
use tx_types::collections::ZMap;

pub trait RW: Read + Write {}
impl<T: Read + Write> RW for T {}

const DEFAULT_SEED: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const ALPH: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

fn main() -> Result<()> {
  let port = env::args().nth(1).unwrap_or_else(|| "/dev/ttyACM0".into());
  let mut sp = serialport::new(port.clone(), 115_200)
      .timeout(Duration::from_millis(200))
      .open()
      .with_context(|| format!("open {}", port))?;

  thread::sleep(Duration::from_millis(100));
  flush_serial(&mut sp);

  // Hello
  let hello_req = Msg {
      v: PROTO_V1,
      id: 0x1000,
      msg: Request::Hello,
  };
  let hello: Msg<Response> = round_trip(&mut sp, &hello_req)?;
  println!("hello: {:?}", hello);

  if let Ok(seed_hex) = std::env::var("BIP39_SEED_HEX") {
      if !seed_hex.trim().is_empty() {
          let seed = parse_seed_hex(&seed_hex)?;
          set_seed(&mut sp, seed)?;
          println!("Seed set from BIP39_SEED_HEX");
      } else {
          println!("BIP39_SEED_HEX is empty, using default seed");
          let seed64 = seed64_from_mnemonic(
            DEFAULT_SEED,
              "",
          )?;
          set_seed(&mut sp, seed64)?;
      }
  } else {
      println!("Using default seed");
      let seed64 = seed64_from_mnemonic(
        DEFAULT_SEED,
          "",
      )?;
      set_seed(&mut sp, seed64)?;
  }
      
  let fp = get_fingerprint(&mut sp)?;
  println!("Master fingerprint: {}", hex::encode(&fp));
  
  // test pubkey generation
  let req_msg = Msg {
      v: PROTO_V1,
      id: 2,
      msg: Request::GetPubkey { 
          path: vec![0x8000002c, 0x80000000, 0x80000000, 0, 0], // m/44'/0'/0'/0/0
          compressed: false 
      },
  };
  let pubkey_resp: Msg<Response> = round_trip(&mut sp, &req_msg)?;
  
  if let Response::OkPubkey { uncompressed } = &pubkey_resp.msg {
      println!("Pubkey (uncompressed): {}", hex::encode(uncompressed));
  }
  
  // Test compressed pubkey
  let req_msg = Msg {
      v: PROTO_V1,
      id: 3,
      msg: Request::GetPubkey { 
          path: vec![0x8000002c, 0x80000000, 0x80000000, 0, 0], // m/44'/0'/0'/0/0
          compressed: true 
      },
  };
  let pubkey_comp_resp: Msg<Response> = round_trip(&mut sp, &req_msg)?;
  
  if let Response::OkPubkeyCompressed { compressed } = &pubkey_comp_resp.msg {
      println!("Pubkey (compressed): {}", hex::encode(compressed));
  }
  
  // Test signing
  let digest32 = [0x55; 32];
  let req_msg = Msg {
      v: PROTO_V1,
      id: 4,
      msg: Request::SignDigest { 
          path: vec![0x8000002c, 0x80000000, 0x80000000, 0, 0], // m/44'/0'/0'/0/0
          digest32,
      },
  };
  let sig_resp: Msg<Response> = round_trip(&mut sp, &req_msg)?;
  
  if let Response::OkSig { sig64 } = &sig_resp.msg {
      println!("Signature: {}", hex::encode(sig64));
      
      // Verify with uncompressed pubkey
      if let Response::OkPubkey { uncompressed } = &pubkey_resp.msg {
          let ok = verify_sig(uncompressed, &digest32, sig64)?;
          println!("Verify (uncompressed): {}", ok);
      }
      
      // Verify with compressed pubkey
      if let Response::OkPubkeyCompressed { compressed } = &pubkey_comp_resp.msg {
          let ep = EncodedPoint::from_bytes(&compressed[..])?;
          let vk = VerifyingKey::from_encoded_point(&ep)?;
          let sig = Signature::from_slice(sig64)?;
          let ok = vk.verify_prehash(&digest32, &sig).is_ok();
          println!("Verify (compressed): {}", ok);
      }

      if let Response::OkPubkeyCompressed { compressed } = &pubkey_comp_resp.msg {
          let addr_p2pkh = p2pkh_address(compressed, true);
          let addr_p2wpkh = p2wpkh_address(compressed, "bc");
          let addr_p2tr = p2tr_address(compressed, "bc"); // note: for taproot you normally use a tweaked x-only key (BIP340); this uses the plain x-only as demo
          println!("P2PKH:   {addr_p2pkh}");
          println!("P2WPKH:  {addr_p2wpkh}");
          println!("P2TR*:   {addr_p2tr} (demo; use tweaked key for real taproot)");
      }
  }

  let req = Msg { v: PROTO_V1, id: 0x3000, msg: Request::GetXpub { path: vec![0x8000002c,0x80000000,0x80000000,0,0] } };
  let xr: Msg<Response> = round_trip(&mut sp, &req)?;
  if let Response::OkXpub(x) = xr.msg {
      let xpub = to_xpub_string(x.depth, x.fp4, x.child, x.chain_code, x.pubkey33);
      println!("xpub: {xpub}");
  }

  Ok(())
}

fn write_cobs_frame(sp: &mut dyn RW, payload: &[u8]) -> Result<()> {
    let mut enc = vec![0u8; payload.len() + payload.len() / 254 + 2];
    let n = cobs::encode(payload, &mut enc);
    sp.write_all(&enc[..n])?;
    sp.write_all(&[0])?;
    Ok(())
}

fn read_cobs_frame(sp: &mut dyn RW, max_len: usize) -> Result<Vec<u8>> {
    let mut rx = Vec::with_capacity(128);
    let mut b = [0u8; 1];
    loop {
        match sp.read(&mut b) {
            Ok(1) => {
                rx.push(b[0]);
                if rx.len() > max_len {
                    return Err(anyhow!("frame too large (> {} bytes)", max_len));
                }
                if b[0] == 0 {
                    return Ok(rx);
                }
            }
            Ok(_) => continue,
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(e) => return Err(e.into()),
        }
    }
}

pub fn round_trip(sp: &mut dyn RW, req: &Msg<Request>) -> Result<Msg<Response>> {
    let mut plain = [0u8; 256];
    let used = to_slice(req, &mut plain)?;

    write_cobs_frame(sp, used)?;

    let mut frame = read_cobs_frame(sp, 512)?;
    let resp: Msg<Response> = from_bytes_cobs(&mut frame)?;

    if resp.v != PROTO_V1 {
        return Err(anyhow!("unsupported proto version {}", resp.v));
    }
    if resp.id != req.id {
        return Err(anyhow!("mismatched id: got {}, expected {}", resp.id, req.id));
    }
    Ok(resp)
}

fn flush_serial(sp: &mut dyn RW) {
    let mut buf = [0u8; 256];
    while sp.read(&mut buf).is_ok() {}
}

trait ReadWrite: Read + Write {}
impl<T: Read + Write> ReadWrite for T {}

fn verify_sig(uncompressed65: &[u8;65], digest32: &[u8;32], sig64: &[u8;64]) -> Result<bool> {
    let ep = EncodedPoint::from_bytes(uncompressed65.as_slice())?;
    let vk = VerifyingKey::from_encoded_point(&ep)?;
    let sig = Signature::from_slice(sig64)?;
    Ok(vk.verify_prehash(digest32, &sig).is_ok())
}

fn vk_from_response(resp: &Response) -> Result<VerifyingKey> {
  match resp {
      Response::OkPubkey { uncompressed } => {
          let ep = EncodedPoint::from_bytes(&uncompressed[..])?;
          Ok(VerifyingKey::from_encoded_point(&ep)?)
      }
      Response::OkPubkeyCompressed { compressed } => {
          let ep = EncodedPoint::from_bytes(&compressed[..])?;
          Ok(VerifyingKey::from_encoded_point(&ep)?)
      }
      _ => anyhow::bail!("response did not contain a pubkey"),
  }
}


/// Derive the 64-byte BIP39 seed from a mnemonic + optional passphrase
pub fn seed64_from_mnemonic(mnemonic: &str, passphrase: &str) -> Result<[u8;64]> {
  let m = Mnemonic::parse_in(Language::English, mnemonic)
      .map_err(|e| anyhow!("bad mnemonic: {e}"))?;
  Ok(m.to_seed(passphrase))
}

fn verify_sig_with_vk(vk: &VerifyingKey, digest32: &[u8;32], sig64: &[u8;64]) -> Result<bool> {
  let sig = Signature::from_slice(sig64)?;
  Ok(vk.verify_prehash(digest32, &sig).is_ok())
}

fn parse_path(s: &str) -> anyhow::Result<Vec<u32>> {
    let s = s.trim();
    let rest = s.strip_prefix("m/").unwrap_or(s);
    let mut out = Vec::new();
    for seg in rest.split('/') {
        if seg.is_empty() { continue; }
        let hardened = seg.ends_with('\'');
        let n: u32 = seg.trim_end_matches('\'').parse()?;
        out.push(if hardened { n | 0x8000_0000 } else { n });
    }
    Ok(out)
}

fn b58check(payload: &[u8]) -> String {
  let chk = Sha256::digest(&Sha256::digest(payload));
  let mut buf = payload.to_vec();
  buf.extend_from_slice(&chk[..4]);

  // count leading zeroes for '1' prefix
  let zeros = buf.iter().take_while(|&&b| b == 0).count();

  // big integer base58 encode
  let mut num = num_bigint::BigUint::from_bytes_be(&buf);
  let mut out = Vec::new();
  while num > num_bigint::BigUint::from(0u8) {
      let (q, r) = num.div_rem(&num_bigint::BigUint::from(58u32));
      let digits = r.to_u32_digits();
      let digit = digits.get(0).copied().unwrap_or(0) as usize;
      out.push(ALPH[digit]);
      num = q;
  }
  for _ in 0..zeros { out.push(b'1'); }
  out.reverse();
  String::from_utf8(out).unwrap()
}

pub fn to_xpub_string(depth: u8, fp4: [u8;4], child: u32, chain_code: [u8;32], pubkey33: [u8;33]) -> String {
  let mut ser = Vec::with_capacity(78);
  ser.extend_from_slice(&0x0488B21Eu32.to_be_bytes()); // xpub mainnet
  ser.push(depth);
  ser.extend_from_slice(&fp4);
  ser.extend_from_slice(&child.to_be_bytes());
  ser.extend_from_slice(&chain_code);
  ser.extend_from_slice(&pubkey33);
  b58check(&ser)
}


fn parse_seed_hex(s: &str) -> Result<[u8;64]> {
  let bytes = hex::decode(s.trim())?;
  if bytes.len() != 64 { return Err(anyhow!("seed must be 64 bytes (128 hex chars)")); }
  let mut out = [0u8;64];
  out.copy_from_slice(&bytes);
  Ok(out)
}

fn set_seed(sp: &mut dyn RW, seed64: [u8;64]) -> Result<()> {
  let req = Msg { v: PROTO_V1, id: 0x2000, msg: Request::SetSeed { seed64 } };
  let resp: Msg<Response> = round_trip(sp, &req)?;
  match resp.msg {
      Response::Ok => Ok(()),
      Response::Err { code } => Err(anyhow!("SetSeed failed with code {}", code)),
      _ => Err(anyhow!("unexpected response to SetSeed")),
  }
}

fn get_fingerprint(sp: &mut dyn RW) -> Result<[u8;4]> {
  let req = Msg { v: PROTO_V1, id: 0x2001, msg: Request::GetFingerprint };
  let resp: Msg<Response> = round_trip(sp, &req)?;
  match resp.msg {
      Response::OkFingerprint { fp4 } => Ok(fp4),
      Response::Err { code } => Err(anyhow!("GetFingerprint failed with code {}", code)),
      _ => Err(anyhow!("unexpected response to GetFingerprint")),
  }
}