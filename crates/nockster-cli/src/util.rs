extern crate alloc;

use anyhow::{anyhow, Result};
use hex;
use nockapp::AtomExt;
use nockvm::noun::{Atom, Noun};
use noun_serde::NounDecode;
use tx_types::transaction_types::SpendBody;
use tx_types::transaction_types_v1::RawTransactionV1;

fn is_printable_ascii(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .all(|&b| (b == 0x09) || (b == 0x0A) || (b == 0x0D) || (0x20..=0x7E).contains(&b))
}

/// Extract transaction ID from various noun formats
pub fn transaction_name_from_noun(noun: &Noun) -> Result<String> {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    if let Ok(cell) = noun.as_cell() {
        if let Ok(tag) = cell.head().as_atom() {
            if tag.as_u64() == Ok(1) {
                if let Ok(rest) = cell.tail().as_cell() {
                    if let Ok(name_atom) = rest.head().as_atom() {
                        if let Ok(bytes) = name_atom.to_bytes_until_nul() {
                            if !bytes.is_empty() && is_printable_ascii(&bytes) {
                                return Ok(String::from_utf8_lossy(&bytes).to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    if let Ok(Ok(raw)) = catch_unwind(AssertUnwindSafe(|| RawTransactionV1::from_noun(noun))) {
        return Ok(raw.id.to_b58());
    }

    if let Ok(cell) = noun.as_cell() {
        let head = cell.head();

        if let Ok(Ok(raw_head)) =
            catch_unwind(AssertUnwindSafe(|| RawTransactionV1::from_noun(&head)))
        {
            return Ok(raw_head.id.to_b58());
        }

        if let Ok(atom) = head.as_atom() {
            if let Ok(bytes) = atom.to_bytes_until_nul() {
                if !bytes.is_empty() && is_printable_ascii(&bytes) {
                    return Ok(String::from_utf8_lossy(&bytes).to_string());
                }
            }
        }
    }

    Err(anyhow!("unable to extract transaction identifier"))
}

pub fn print_raw_v1_details(raw: &RawTransactionV1) {
    println!("raw-tx (V1):");
    println!("  version      = {}", raw.version);
    println!("  id           = {}", raw.id.to_b58());
    println!("  spends       = {}", raw.spends.map.wyt());
    for (idx, (name, spend)) in raw.spends.map.tap().into_iter().enumerate() {
        println!("  - spend[{}]:", idx);
        if name.p.len() >= 2 {
            println!(
                "      name        = [{:?} {:?}]",
                name.p[0].to_b58(),
                name.p[1].to_b58()
            );
        }
        let fee = match &spend.body {
            SpendBody::V0(v0) => v0.fee.value,
            SpendBody::V0ToV1(v0tov1) => v0tov1.fee.value,
            SpendBody::V1(v1) => v1.fee.value,
        };
        println!("      fee         = {}", fee);
    }
}

pub fn pretty_noun(n: &Noun, max_depth: usize, max_items: usize) -> String {
    fn fmt_atom(atom: &Atom) -> String {
        // cord if it has a terminating NUL and is printable
        if let Ok(bytes) = atom.to_bytes_until_nul() {
            let b: Vec<u8> = bytes.to_vec();
            if is_printable_ascii(&b) {
                return format!("atom(cord:\"{}\")", String::from_utf8_lossy(&b));
            }
        }
        // otherwise: show small atoms as hex, big ones summarized
        let nbits = nockvm::serialization::met0_usize(atom.clone());
        let nbytes = (nbits + 7) / 8;
        if nbytes <= 64 {
            let v = vec![0u8; nbytes];
            let _ = atom.as_bitslice();
            format!("atom({} bytes, 0x{})", nbytes, hex::encode(v))
        } else {
            format!("atom({} bytes)", nbytes)
        }
    }

    fn try_collect_list(mut n: Noun, max_items: usize) -> Option<(Vec<Noun>, bool)> {
        let mut out = Vec::new();
        for _ in 0..max_items {
            if let Ok(cell) = n.as_cell() {
                out.push(cell.head());
                n = cell.tail();
                if let Ok(a) = n.as_atom() {
                    if a.as_u64() == Ok(0) {
                        return Some((out, false));
                    }
                }
            } else {
                return None;
            }
        }
        Some((out, true)) // truncated
    }

    fn go(n: Noun, depth: usize, max_depth: usize, max_items: usize, indent: usize) -> String {
        if depth >= max_depth {
            return "...".into();
        }
        if let Ok(a) = n.as_atom() {
            return fmt_atom(&a);
        }
        if let Ok(c) = n.as_cell() {
            // try render as list if shape matches
            if let Some((els, truncated)) = try_collect_list(n, max_items) {
                let mut s = String::new();
                s.push_str("[\n");
                for (i, el) in els.into_iter().enumerate() {
                    s.push_str(&" ".repeat(indent + 2));
                    s.push_str(&go(el, depth + 1, max_depth, max_items, indent + 2));
                    if i + 1 < max_items {
                        s.push('\n');
                    }
                }
                if truncated {
                    s.push_str(&" ".repeat(indent + 2));
                    s.push_str("…\n");
                }
                s.push_str(&" ".repeat(indent));
                s.push(']');
                return s;
            }
            // generic cell (head .. tail)
            let mut s = String::new();
            s.push_str("[\n");
            s.push_str(&" ".repeat(indent + 2));
            s.push_str(&go(c.head(), depth + 1, max_depth, max_items, indent + 2));
            s.push_str(",\n");
            s.push_str(&" ".repeat(indent + 2));
            s.push_str(&go(c.tail(), depth + 1, max_depth, max_items, indent + 2));
            s.push_str("\n");
            s.push_str(&" ".repeat(indent));
            s.push(']');
            return s;
        }
        "<?>".into()
    }

    go(n.clone(), 0, max_depth, max_items, 0)
}

// ---------- tiny utils -------------------------------------------------------

pub fn parse_64(s: &str) -> anyhow::Result<[u8; 64]> {
    let mut h = s.trim();
    if let Some(stripped) = h.strip_prefix("0x") {
        h = stripped;
    }
    let cleaned: String = h
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .collect();

    let bytes =
        hex::decode(&cleaned).map_err(|e| anyhow::anyhow!("invalid hex for 64-byte seed: {e}"))?;
    if bytes.len() != 64 {
        return Err(anyhow::anyhow!(
            "seed must be exactly 64 bytes (got {} bytes)",
            bytes.len()
        ));
    }

    let mut out = [0u8; 64];
    out.copy_from_slice(&bytes);
    Ok(out)
}

pub fn fmt_u64x5(v: &[u64; 5]) -> String {
    v.iter()
        .map(|w| format!("{w:016x}"))
        .collect::<Vec<_>>()
        .join("_")
}
pub fn fmt_u64x6(v: &[u64; 6]) -> String {
    v.iter()
        .map(|w| format!("{w:016x}"))
        .collect::<Vec<_>>()
        .join("_")
}
pub fn fmt_u64x8(v: &[u64; 8]) -> String {
    v.iter()
        .map(|w| format!("{w:016x}"))
        .collect::<Vec<_>>()
        .join("_")
}
