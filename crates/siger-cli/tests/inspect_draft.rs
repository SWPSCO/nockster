//! Inspect draft file structure to find the signing message

use bytes::Bytes;
use nockapp::noun::slab::NounSlab;
use nockvm::noun::Noun;
use std::fs;

fn inspect_noun(noun: &Noun, prefix: &str, depth: usize) {
    if depth > 5 {
        return;
    }

    if let Ok(atom) = noun.as_atom() {
        let bytes = atom.as_ne_bytes();
        if bytes.len() <= 8 {
            if let Ok(val) = atom.as_u64() {
                if val < 256 {
                    println!("{}{}: atom({} bytes) = {}", prefix, depth, bytes.len(), val);
                } else {
                    println!(
                        "{}{}: atom({} bytes) = {} = {:016x}",
                        prefix,
                        depth,
                        bytes.len(),
                        val,
                        val
                    );
                }
            } else {
                println!(
                    "{}{}: atom({} bytes) = {:02x?}",
                    prefix,
                    depth,
                    bytes.len(),
                    bytes
                );
            }
        } else if bytes.len() <= 40 {
            // Could be a hash
            if bytes.len() == 40 {
                // Try to interpret as 5x u64 hash
                let values: Vec<u64> = bytes
                    .chunks(8)
                    .map(|chunk| {
                        let mut arr = [0u8; 8];
                        arr.copy_from_slice(chunk);
                        u64::from_le_bytes(arr)
                    })
                    .collect();
                println!(
                    "{}{}: atom({} bytes) = Hash {:016x}_{:016x}_{:016x}_{:016x}_{:016x}",
                    prefix,
                    depth,
                    bytes.len(),
                    values[0],
                    values[1],
                    values[2],
                    values[3],
                    values[4]
                );
            } else {
                println!(
                    "{}{}: atom({} bytes) = {:02x?}",
                    prefix,
                    depth,
                    bytes.len(),
                    bytes
                );
            }
        } else if bytes.len() == 56 {
            // b58 string
            if let Ok(s) = std::str::from_utf8(&bytes) {
                println!(
                    "{}{}: atom({} bytes) = b58 string: {}",
                    prefix,
                    depth,
                    bytes.len(),
                    s
                );
            } else {
                println!("{}{}: atom({} bytes)", prefix, depth, bytes.len());
            }
        } else {
            println!("{}{}: atom({} bytes)", prefix, depth, bytes.len());
        }
    } else if let Ok(cell) = noun.as_cell() {
        println!("{}{}:", prefix, depth);
        inspect_noun(&cell.head(), &format!("{}  h", prefix), depth + 1);
        inspect_noun(&cell.tail(), &format!("{}  t", prefix), depth + 1);
    }
}

#[test]
fn inspect_draft_structure() {
    println!("\n=== Inspecting known-good.draft structure ===\n");

    let draft_data = fs::read("../../known-good.draft").expect("read known-good.draft");
    println!("File size: {} bytes", draft_data.len());

    let mut slab: NounSlab = NounSlab::new();
    let noun = slab.cue_into(Bytes::from(draft_data)).expect("cue");

    inspect_noun(&noun, "", 0);

    // Also check test.tx for comparison
    println!("\n=== Inspecting test.tx structure ===\n");

    let tx_data = fs::read("../../test.tx").expect("read test.tx");
    println!("File size: {} bytes", tx_data.len());

    let mut slab2: NounSlab = NounSlab::new();
    let noun2 = slab2.cue_into(Bytes::from(tx_data)).expect("cue");

    inspect_noun(&noun2, "", 0);

    println!("\n=== Done ===\n");
}
