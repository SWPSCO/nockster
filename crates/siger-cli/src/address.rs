use bech32::{ToBase32, Variant};
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};

/// HASH160 = RIPEMD160(SHA256(input))
fn hash160(bytes: &[u8]) -> [u8;20] {
    let sha = Sha256::digest(bytes);
    let ripe = Ripemd160::digest(sha);
    let mut out = [0u8;20];
    out.copy_from_slice(&ripe);
    out
}

/// Base58Check with version byte
fn base58check(version: u8, payload: &[u8]) -> String {
  use num_integer::Integer;
  let mut raw = Vec::with_capacity(1 + payload.len() + 4);
  raw.push(version);
  raw.extend_from_slice(payload);
  let chk = Sha256::digest(&Sha256::digest(&raw));
  raw.extend_from_slice(&chk[..4]);

  const ALPH: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
  let zeros = raw.iter().take_while(|&&b| b == 0).count();
  let mut n = num_bigint::BigUint::from_bytes_be(&raw);
  let mut out = Vec::new();
  while n > num_bigint::BigUint::from(0u8) {
      let (q, r) = n.div_rem(&num_bigint::BigUint::from(58u32));
      let digits = r.to_u32_digits();
      let digit = digits.first().copied().unwrap_or(0) as usize;
      out.push(ALPH[digit]);
      n = q;
  }
  for _ in 0..zeros { out.push(b'1'); }
  out.reverse();
  String::from_utf8(out).unwrap()
}

/// P2PKH (legacy): version 0x00 mainnet, 0x6f testnet
pub fn p2pkh_address(pubkey_compressed33: &[u8;33], mainnet: bool) -> String {
    let h160 = hash160(pubkey_compressed33);
    base58check(if mainnet { 0x00 } else { 0x6f }, &h160)
}

/// Bech32 segwit v0 (P2WPKH): hrp = "bc" (mainnet) or "tb" (testnet)
pub fn p2wpkh_address(pubkey_compressed33: &[u8;33], hrp: &str) -> String {
    let prog = hash160(pubkey_compressed33);
    let mut data = vec![0u8]; // witness version 0
    // convert 8-bit -> 5-bit (no padding beyond what bech32 expects)
    let mut acc = 0u32;
    let mut bits = 0u32;
    for b in prog {
        acc = (acc << 8) | (b as u32);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            data.push(((acc >> bits) & 0x1f) as u8);
        }
    }
    // pad zero bits
    if bits > 0 { data.push(((acc << (5 - bits)) & 0x1f) as u8); }

    bech32::encode(hrp, data.to_base32(), Variant::Bech32).unwrap()
}

/// Bech32m segwit v1 (P2TR): use x-only pubkey (32 bytes) as program
pub fn p2tr_address(pubkey_compressed33: &[u8;33], hrp: &str) -> String {
    // compressed pubkey 0x02/0x03 || X; x-only is the 32-byte X
    let xonly = &pubkey_compressed33[1..33];
    let mut data = vec![1u8]; // witness version 1
    // 8->5 bit groups
    let mut acc = 0u32;
    let mut bits = 0u32;
    for &b in xonly {
        acc = (acc << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            data.push(((acc >> bits) & 0x1f) as u8);
        }
    }
    if bits > 0 { data.push(((acc << (5 - bits)) & 0x1f) as u8); }
    bech32::encode(hrp, data.to_base32(), Variant::Bech32m).unwrap()
}
