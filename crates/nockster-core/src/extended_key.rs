//! Nockchain extended-key (`zprv`/`zpub`) interop, mirroring the wallet's
//! `hoon/common/slip10.hoon` `++serialize-extended` / `++from-extended-key`.
//!
//! Wire format: base58 (Bitcoin alphabet) over a big-endian payload plus a
//! 4-byte double-SHA256 checksum:
//!
//! ```text
//! [type(4)][protocol-version(1)][depth(1)][parent-fp(4)][index(4)]
//! [chain-code(32)][key-data][checksum(4)]
//! ```
//!
//! `type` is `0x0110_6331` for private keys ("zprv" once encoded) and
//! `0x0c0e_bb09` for public keys ("zpub"). `key-data` is `0x00 ‖ sk(32)`
//! (33 bytes, the leading zero inherited from BIP32) for zprv and the 97-byte
//! cheetah point (`ser_a_pt` layout: 0x01 sentinel, then Y and X as six
//! big-endian limbs each) for zpub. Integers and the sk scalar are big-endian,
//! matching Hoon's MSB-first atom rendering — and the byte order the rest of
//! this workspace already uses for coil `sk`/`cc`.
//!
//! NOTE: `crates/nockster-wasm` carries a parallel implementation (it avoids
//! a nockster-core dependency to keep the browser bundle lean). If you change
//! a byte layout here, change it there too.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use sha2::{Digest, Sha256};
use zeroize::Zeroize;

const ZPRV_TYPE: [u8; 4] = [0x01, 0x10, 0x63, 0x31];
const ZPUB_TYPE: [u8; 4] = [0x0c, 0x0e, 0xbb, 0x09];
/// type + ver + depth + parent fp + index + chain code
const META_LEN: usize = 4 + 1 + 1 + 4 + 4 + 32;
const ZPRV_KEY_LEN: usize = 33;
const ZPUB_KEY_LEN: usize = 97;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtendedKeyError {
    /// Not valid base58 (Bitcoin alphabet).
    Base58,
    /// Double-SHA256 checksum mismatch.
    Checksum,
    /// Payload length does not match the declared key type.
    Length,
    /// Neither the zprv nor the zpub type prefix.
    UnknownType,
    /// zprv key-data did not start with the 0x00 private-key tag.
    KeyTag,
    /// zpub key-data did not start with the 0x01 point sentinel.
    PointSentinel,
}

/// A decoded `zprv`.
#[derive(Clone)]
pub struct ExtendedPrivKey {
    pub sk: [u8; 32],
    pub chain_code: [u8; 32],
    pub depth: u8,
    pub index: u32,
    pub parent_fingerprint: [u8; 4],
    /// Nockchain protocol version (0 = pre-Oct-2025 addressing, 1 = current).
    pub protocol_version: u8,
}

impl ExtendedPrivKey {
    /// The 64-byte coil (`sk ‖ cc`) the device and wallet store natively.
    pub fn coil64(&self) -> [u8; 64] {
        let mut coil = [0u8; 64];
        coil[..32].copy_from_slice(&self.sk);
        coil[32..].copy_from_slice(&self.chain_code);
        coil
    }
}

impl Drop for ExtendedPrivKey {
    fn drop(&mut self) {
        self.sk.zeroize();
        self.chain_code.zeroize();
    }
}

/// A decoded `zpub`.
#[derive(Clone)]
pub struct ExtendedPubKey {
    /// 97-byte cheetah point in `ser_a_pt` layout.
    pub point97: [u8; 97],
    pub chain_code: [u8; 32],
    pub depth: u8,
    pub index: u32,
    pub parent_fingerprint: [u8; 4],
    pub protocol_version: u8,
}

pub enum ExtendedKey {
    Private(ExtendedPrivKey),
    Public(ExtendedPubKey),
}

fn sha256d4(data: &[u8]) -> [u8; 4] {
    let first = Sha256::digest(data);
    let second = Sha256::digest(first);
    let mut out = [0u8; 4];
    out.copy_from_slice(&second[..4]);
    out
}

/// Parse a `zprv` or `zpub` string.
pub fn parse_extended_key(s: &str) -> Result<ExtendedKey, ExtendedKeyError> {
    let mut raw: Vec<u8> = bs58::decode(s.trim().as_bytes())
        .into_vec()
        .map_err(|_| ExtendedKeyError::Base58)?;
    let result = parse_raw(&raw);
    raw.zeroize();
    result
}

/// Parse a `zprv` string specifically (rejects zpub).
pub fn parse_zprv(s: &str) -> Result<ExtendedPrivKey, ExtendedKeyError> {
    match parse_extended_key(s)? {
        ExtendedKey::Private(key) => Ok(key),
        ExtendedKey::Public(_) => Err(ExtendedKeyError::UnknownType),
    }
}

fn parse_raw(raw: &[u8]) -> Result<ExtendedKey, ExtendedKeyError> {
    if raw.len() < META_LEN + 4 {
        return Err(ExtendedKeyError::Length);
    }
    let (payload, checksum) = raw.split_at(raw.len() - 4);
    if sha256d4(payload) != checksum {
        return Err(ExtendedKeyError::Checksum);
    }

    let typ: [u8; 4] = payload[0..4].try_into().unwrap();
    let key_len = match typ {
        ZPRV_TYPE => ZPRV_KEY_LEN,
        ZPUB_TYPE => ZPUB_KEY_LEN,
        _ => return Err(ExtendedKeyError::UnknownType),
    };
    // The Hoon serializer always writes the protocol-version byte, but its
    // parser also accepts the older layout without it; we require it.
    if payload.len() != META_LEN + key_len {
        return Err(ExtendedKeyError::Length);
    }

    let protocol_version = payload[4];
    let depth = payload[5];
    let parent_fingerprint: [u8; 4] = payload[6..10].try_into().unwrap();
    let index = u32::from_be_bytes(payload[10..14].try_into().unwrap());
    let chain_code: [u8; 32] = payload[14..46].try_into().unwrap();
    let key_data = &payload[46..];

    match typ {
        ZPRV_TYPE => {
            if key_data[0] != 0x00 {
                return Err(ExtendedKeyError::KeyTag);
            }
            Ok(ExtendedKey::Private(ExtendedPrivKey {
                sk: key_data[1..].try_into().unwrap(),
                chain_code,
                depth,
                index,
                parent_fingerprint,
                protocol_version,
            }))
        }
        _ => {
            if key_data[0] != 0x01 {
                return Err(ExtendedKeyError::PointSentinel);
            }
            Ok(ExtendedKey::Public(ExtendedPubKey {
                point97: key_data.try_into().unwrap(),
                chain_code,
                depth,
                index,
                parent_fingerprint,
                protocol_version,
            }))
        }
    }
}

fn encode_with_type(typ: [u8; 4], key_data: &[u8], meta: (&[u8; 32], u8, u32, [u8; 4], u8)) -> String {
    let (chain_code, depth, index, parent_fingerprint, protocol_version) = meta;
    let mut payload = Vec::with_capacity(META_LEN + key_data.len() + 4);
    payload.extend_from_slice(&typ);
    payload.push(protocol_version);
    payload.push(depth);
    payload.extend_from_slice(&parent_fingerprint);
    payload.extend_from_slice(&index.to_be_bytes());
    payload.extend_from_slice(chain_code);
    payload.extend_from_slice(key_data);
    let checksum = sha256d4(&payload);
    payload.extend_from_slice(&checksum);
    let out = bs58::encode(&payload).into_string();
    payload.zeroize();
    out
}

/// Serialize a `zprv` (round-trips `parse_zprv`; same output as the wallet).
pub fn encode_zprv(key: &ExtendedPrivKey) -> String {
    let mut key_data = [0u8; ZPRV_KEY_LEN];
    key_data[1..].copy_from_slice(&key.sk);
    let out = encode_with_type(
        ZPRV_TYPE,
        &key_data,
        (
            &key.chain_code,
            key.depth,
            key.index,
            key.parent_fingerprint,
            key.protocol_version,
        ),
    );
    key_data.zeroize();
    out
}

/// Serialize a `zpub` from a 97-byte cheetah point.
pub fn encode_zpub(key: &ExtendedPubKey) -> String {
    encode_with_type(
        ZPUB_TYPE,
        &key.point97,
        (
            &key.chain_code,
            key.depth,
            key.index,
            key.parent_fingerprint,
            key.protocol_version,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Known-good vector straight from `nockchain-wallet keygen` (throwaway
    // key): mnemonic, version, zprv, zpub, and master address all from one
    // run, so the test cross-checks our parser against the wallet's own
    // derivation chain.
    const VECTOR_ZPRV: &str = "zprvLxxkCBq3s5HYziqzm2wS3a4TWprJRMH2oUT5JepKhd3Wok8R8Ki759ZA4CtPEhwHuGna1V4Zrhb5uGvxoUeEZgtc5ktu21GWtiJnqRSSQq6U";
    const VECTOR_ZPUB: &str = "zpub2kRJ7D6VCvzVfDbhh4Y2ZRqze98yfNZ3fBCS9gizoYzgbjYaSXTrCTUiTwWURWzGPs4ySRpu7ZNnF9Jjok4mxW6y3XBmwFcYoaUcgLHbnMZrQvEzPvQecL1DthSYnuvvjKKG3uRo1r9MnxQsmuMF8dRXaUTvtfF2XGfv5hfEjUkKwzABpywvGu9163M71SbwrE91";
    const VECTOR_MNEMONIC: &str = "wedding chef bread absurd leader surge auction access document fiber chunk hurt earn rain swarm cotton leisure ozone drill switch cry jungle soda oxygen";
    const VECTOR_ADDRESS: &str = "9QnMES6nZnbyqPKfTysvisj1UqvxjSkL4XQaAsMUafGVw2m7rrUHzsW";

    #[test]
    fn parses_known_zprv() {
        let key = parse_zprv(VECTOR_ZPRV).expect("parse");
        assert_eq!(key.protocol_version, 1);
        assert_eq!(key.depth, 0);
        assert_eq!(key.index, 0);
        assert_eq!(key.parent_fingerprint, [0u8; 4]);

        // The zprv must contain exactly the master coil the wallet derives
        // from the matching seed phrase.
        let (sk, cc) = crate::cheetah::master_from_mnemonic(VECTOR_MNEMONIC, "");
        assert_eq!(key.sk, sk);
        assert_eq!(key.chain_code, cc);
    }

    #[test]
    fn zprv_round_trips() {
        let key = parse_zprv(VECTOR_ZPRV).expect("parse");
        assert_eq!(encode_zprv(&key), VECTOR_ZPRV);
    }

    #[test]
    fn parses_known_zpub_and_matches_zprv() {
        let ExtendedKey::Public(pubkey) = parse_extended_key(VECTOR_ZPUB).expect("parse") else {
            panic!("zpub parsed as private");
        };
        let key = parse_zprv(VECTOR_ZPRV).expect("parse");
        assert_eq!(pubkey.chain_code, key.chain_code);
        assert_eq!(pubkey.protocol_version, 1);

        let pk = crate::cheetah::cheetah_pub_from_sk(key.sk);
        let point97 = crate::cheetah::ser_a_pt(&(pk[0], pk[1]));
        assert_eq!(pubkey.point97[..], point97[..]);

        assert_eq!(encode_zpub(&pubkey), VECTOR_ZPUB);
    }

    #[test]
    fn derives_master_address_from_zprv() {
        let key = parse_zprv(VECTOR_ZPRV).expect("parse");
        let pk = crate::cheetah::cheetah_pub_from_sk(key.sk);
        let address =
            crate::draft_sign::cheetah_pubkey_pkh_v1((pk[0], pk[1])).expect("pkh");
        assert_eq!(address, VECTOR_ADDRESS);
    }

    #[test]
    fn rejects_corruption() {
        // Flip a character: checksum must catch it.
        let mut s: alloc::vec::Vec<u8> = VECTOR_ZPRV.bytes().collect();
        let last = s.len() - 1;
        s[last] = if s[last] == b'U' { b'V' } else { b'U' };
        let s = core::str::from_utf8(&s).unwrap();
        assert!(matches!(
            parse_zprv(s),
            Err(ExtendedKeyError::Checksum) | Err(ExtendedKeyError::Base58)
        ));

        // A zpub is not a zprv.
        assert!(parse_zprv(VECTOR_ZPUB).is_err());

        // Garbage.
        assert!(parse_extended_key("zprvnotakey").is_err());
        assert!(parse_extended_key("").is_err());
    }
}
