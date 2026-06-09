//! nockchain-wallet keyfile interop: parse `keys.export` and build
//! `master-pubkey.export` (jammed coil) files.
//!
//! Byte-order convention: Hoon's `hmac-sha512l` renders atoms MSB-first, so
//! the `ser-p` atom's most-significant byte is the 0x01 sentinel and its
//! little-endian bytes are the *reverse* of the `ser_a_pt` array. pokenoun
//! atoms take LE bytes, hence the reversals below.
//!
//! NOTE: `crates/nockster-wasm` carries a parallel implementation of these
//! routines (it avoids a nockster-core dependency to keep the browser bundle
//! lean). If you change a byte layout here, change it there too.

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::draft_sign::{cue, jam, Arena, Noun};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct KeyfileSummary {
    /// Seed phrases found in the file (the wallet stores the mnemonic as a
    /// `%seed` entry); usually zero or one.
    pub seedphrases: Vec<String>,
    pub coil_pub_count: u32,
    pub coil_prv_count: u32,
    pub label_count: u32,
    pub watch_count: u32,
    /// Coil versions seen (0 = pre-Oct-2025 addressing, 1 = current).
    pub versions: Vec<u8>,
    pub entry_count: u32,
}

fn uncons(noun: Noun, arena: &Arena) -> Option<(Noun, Noun)> {
    match noun {
        Noun::Cell(id) => {
            let cell = arena.cell(id);
            Some((cell.head, cell.tail))
        }
        _ => None,
    }
}

fn atom_text(noun: Noun, arena: &Arena) -> Option<String> {
    match noun {
        Noun::Atom(id) => core::str::from_utf8(arena.atom_bytes(id))
            .ok()
            .map(|s| s.to_string()),
        _ => None,
    }
}

fn atom_is(noun: Noun, arena: &Arena, tag: &[u8]) -> bool {
    match noun {
        Noun::Atom(id) => arena.atom_eq_bytes(id, tag),
        _ => false,
    }
}

/// Walk one `meta` noun: `[%coil coil-v3]`, `[%label @t]`, `[%seed @t]`, or
/// `[%watch-key @t]`.
fn scan_meta(meta: Noun, arena: &Arena, out: &mut KeyfileSummary) {
    let Some((tag, payload)) = uncons(meta, arena) else {
        return;
    };
    if atom_is(tag, arena, b"seed") {
        if let Some(text) = atom_text(payload, arena) {
            if !text.is_empty() {
                out.seedphrases.push(text);
            }
        }
    } else if atom_is(tag, arena, b"label") {
        out.label_count += 1;
    } else if atom_is(tag, arena, b"watch-key") {
        out.watch_count += 1;
    } else if atom_is(tag, arena, b"coil") {
        // coil-v3: [%0|%1 [key cc]]; legacy coil-v0 is bare [key cc].
        let (version, coil_data) = match uncons(payload, arena) {
            Some((head, tail)) if matches!(head, Noun::Atom(_)) => {
                let v = match head {
                    Noun::Atom(id) => arena.atom_u64(id).unwrap_or(0) as u8,
                    _ => 0,
                };
                (v, tail)
            }
            _ => (0, payload),
        };
        if !out.versions.contains(&version) {
            out.versions.push(version);
        }
        if let Some((key, _cc)) = uncons(coil_data, arena) {
            if let Some((key_tag, _key_atom)) = uncons(key, arena) {
                if atom_is(key_tag, arena, b"pub") {
                    out.coil_pub_count += 1;
                } else if atom_is(key_tag, arena, b"prv") {
                    out.coil_prv_count += 1;
                }
            }
        }
    }
}

/// Parse a nockchain-wallet `keys.export` file: a jammed `(list [trek meta])`.
pub fn parse_keyfile(bytes: &[u8]) -> Result<KeyfileSummary, &'static str> {
    let mut arena = Arena::new();
    let root = cue(bytes, &mut arena).map_err(|_| "not a valid jammed noun")?;
    let mut out = KeyfileSummary::default();
    let mut cursor = root;
    while let Some((entry, rest)) = uncons(cursor, &arena) {
        out.entry_count += 1;
        if let Some((_trek, meta)) = uncons(entry, &arena) {
            scan_meta(meta, &arena, &mut out);
        }
        cursor = rest;
        if out.entry_count > 4096 {
            return Err("keyfile too large");
        }
    }
    if out.entry_count == 0 {
        return Err("no entries found (is this a keys.export file?)");
    }
    Ok(out)
}

/// Affine-point serialization matching Hoon's `ser-p` octet stream: 0x01
/// sentinel, then Y then X limbs, most-significant limb first, each limb
/// big-endian.
fn ser_a_pt(pk: &([u64; 6], [u64; 6])) -> [u8; 97] {
    let (x, y) = pk;
    let mut out = [0u8; 97];
    out[0] = 0x01;
    let mut off = 1;
    for &w in y.iter().rev().chain(x.iter().rev()) {
        out[off..off + 8].copy_from_slice(&w.to_be_bytes());
        off += 8;
    }
    out
}

/// Build the jammed `coil` noun nockchain-wallet's `import-master-pubkey`
/// expects: `[%coil [%1 [[%pub p=@] cc=@]]]`.
pub fn build_master_pubkey_export(x: [u64; 6], y: [u64; 6], chain_code: &[u8; 32]) -> Vec<u8> {
    let mut ser = ser_a_pt(&(x, y));
    ser.reverse();
    let mut cc_le = [0u8; 32];
    for (dst, src) in cc_le.iter_mut().zip(chain_code.iter().rev()) {
        *dst = *src;
    }
    let mut arena = Arena::new();
    let tag_coil = arena.alloc_atom_bytes(b"coil");
    let tag_pub = arena.alloc_atom_bytes(b"pub");
    let pub_atom = arena.alloc_atom_bytes(&ser);
    let cc_atom = arena.alloc_atom_bytes(&cc_le);
    let key = arena.alloc_cell(tag_pub, pub_atom);
    let coil_data = arena.alloc_cell(key, cc_atom);
    let version = arena.alloc_atom_u64(1);
    let coil_v3 = arena.alloc_cell(version, coil_data);
    let coil = arena.alloc_cell(tag_coil, coil_v3);
    jam(coil, &arena)
}

/// Wrap raw bytes as a jammed atom noun — the usual shape for a `%hax`
/// preimage going into the device vault.
pub fn jam_atom(bytes: &[u8]) -> Vec<u8> {
    let mut arena = Arena::new();
    let atom = arena.alloc_atom_bytes(bytes);
    jam(atom, &arena)
}

/// Extract the raw bytes of a jammed atom noun (e.g. a revealed vault
/// preimage). Errors if the noun is a cell.
pub fn cue_atom(bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut arena = Arena::new();
    let root = cue(bytes, &mut arena).map_err(|_| "not a valid jammed noun")?;
    match root {
        Noun::Atom(id) => Ok(arena.atom_bytes(id).to_vec()),
        Noun::Cell(_) => Err("noun is a cell, not an atom"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atom_roundtrip() {
        let secret = [0xABu8, 0xCD, 0x01, 0x00, 0x42];
        let jammed = jam_atom(&secret);
        // Trailing zero bytes are not significant in atoms.
        let back = cue_atom(&jammed).unwrap();
        assert_eq!(&back, &[0xAB, 0xCD, 0x01, 0x00, 0x42][..back.len()]);
    }

    #[test]
    fn master_pubkey_export_roundtrips_as_coil() {
        let x = [1u64, 2, 3, 4, 5, 6];
        let y = [7u64, 8, 9, 10, 11, u64::MAX];
        let cc = [0x42u8; 32];
        let bytes = build_master_pubkey_export(x, y, &cc);

        let mut arena = Arena::new();
        let root = cue(&bytes, &mut arena).unwrap();
        let (tag, coil_v3) = uncons(root, &arena).unwrap();
        assert!(atom_is(tag, &arena, b"coil"));
        let (version, coil_data) = uncons(coil_v3, &arena).unwrap();
        assert!(matches!(version, Noun::Atom(id) if arena.atom_u64(id) == Some(1)));
        let (key, cc_atom) = uncons(coil_data, &arena).unwrap();
        let (key_tag, pub_atom) = uncons(key, &arena).unwrap();
        assert!(atom_is(key_tag, &arena, b"pub"));
        // The pub atom is 97 bytes whose top (last LE) byte is the sentinel.
        let Noun::Atom(id) = pub_atom else { panic!() };
        let pub_bytes = arena.atom_bytes(id);
        assert_eq!(pub_bytes.len(), 97);
        assert_eq!(*pub_bytes.last().unwrap(), 0x01);
        // Chain code atom: LE bytes are the reversed array (here symmetric).
        let Noun::Atom(id) = cc_atom else { panic!() };
        assert_eq!(arena.atom_bytes(id), &cc[..]);
    }
}
