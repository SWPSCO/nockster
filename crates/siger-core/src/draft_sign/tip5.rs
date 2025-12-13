extern crate alloc;

use alloc::vec::Vec;

use crate::draft_sign::noun_codec::{Arena, Noun};

pub const GOLDILOCKS_P: u64 = crate::math::math::GOLDILOCKS_P;

const STATE_SIZE: usize = 16;
const RATE: usize = 10;
const DIGEST_LENGTH: usize = 5;

// Montgomery constants for Goldilocks field (see nockchain-math tip5 implementation)
const R2: u64 = 0xffff_fffe_0000_0001;
const R_MOD_P: u64 = 0xffff_ffff;
const RP: u128 = 0xffff_ffff_0000_0001_0000_0000_0000_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tip5Error {
    NotBased,
    BadLength,
}

#[inline(always)]
fn mont_reduction(x: u128) -> u64 {
    debug_assert!(x < RP);
    let x1: u128 = (x >> 32) & 0xffff_ffff;
    let x2: u128 = x >> 64;
    let c: u128 = {
        let x0: u128 = x & 0xffff_ffff;
        (x0 + x1) << 32
    };
    let f: u128 = c >> 64;
    let d: u128 = c - (x1 + (f * (GOLDILOCKS_P as u128)));
    if x2 >= d {
        (x2 - d) as u64
    } else {
        (x2 + (GOLDILOCKS_P as u128) - d) as u64
    }
}

#[inline(always)]
fn montify(x: u64) -> u64 {
    debug_assert!(x < GOLDILOCKS_P);
    mont_reduction((x as u128) * (R2 as u128))
}

#[inline(always)]
fn is_based(x: u64) -> bool {
    x < GOLDILOCKS_P
}

fn create_init_sponge_variable() -> [u64; STATE_SIZE] {
    [0u64; STATE_SIZE]
}

fn create_init_sponge_fixed() -> [u64; STATE_SIZE] {
    [
        0u64,
        0u64,
        0u64,
        0u64,
        0u64,
        0u64,
        0u64,
        0u64,
        0u64,
        0u64,
        R_MOD_P,
        R_MOD_P,
        R_MOD_P,
        R_MOD_P,
        R_MOD_P,
        R_MOD_P,
    ]
}

#[inline(always)]
fn tip5_absorb_rate(sponge: &mut [u64; STATE_SIZE], input: &[u64; RATE]) {
    sponge[..RATE].copy_from_slice(input);
    crate::math::math::tip5_permute(sponge);
}

fn tip5_pad(words: &mut Vec<u64>, r: usize) {
    words.push(1);
    for _ in 0..((RATE - r) - 1) {
        words.push(0);
    }
}

fn tip5_montify(words: &mut [u64]) -> Result<(), Tip5Error> {
    for w in words.iter_mut() {
        if !is_based(*w) {
            return Err(Tip5Error::NotBased);
        }
        *w = montify(*w);
    }
    Ok(())
}

fn tip5_calc_digest(sponge: &[u64; STATE_SIZE]) -> [u64; DIGEST_LENGTH] {
    let mut digest = [0u64; DIGEST_LENGTH];
    for i in 0..DIGEST_LENGTH {
        digest[i] = mont_reduction(sponge[i] as u128);
    }
    digest
}

/// TIP5 variable-length sponge hash over a Hoon list of belts.
///
/// This mirrors `hash-varlen:tip5` in the zkvm-jetpack jets.
pub fn hash_varlen_words(words: &[u64]) -> Result<[u64; 5], Tip5Error> {
    let mut input = words.to_vec();
    let r = input.len() % RATE;
    tip5_pad(&mut input, r);
    tip5_montify(&mut input)?;

    let q = words.len() / RATE;
    let mut sponge = create_init_sponge_variable();

    let mut idx = 0usize;
    let mut cnt_q = q;
    loop {
        let block: &[u64] = &input[idx..idx + RATE];
        let block_arr: &[u64; RATE] = block.try_into().expect("slice len RATE");
        tip5_absorb_rate(&mut sponge, block_arr);

        idx += RATE;
        if cnt_q == 0 {
            break;
        }
        cnt_q -= 1;
    }

    Ok(tip5_calc_digest(&sponge))
}

/// TIP5 fixed-length hash over exactly 10 belts.
///
/// Mirrors `hash-10:tip5` / `hash_10` jet.
pub fn hash_10_words(words10: &[u64; 10]) -> Result<[u64; 5], Tip5Error> {
    for &w in words10.iter() {
        if !is_based(w) {
            return Err(Tip5Error::NotBased);
        }
    }
    let mut block = *words10;
    tip5_montify(&mut block)?;
    let mut sponge = create_init_sponge_fixed();
    tip5_absorb_rate(&mut sponge, &block);
    Ok(tip5_calc_digest(&sponge))
}

#[inline(always)]
pub fn hash_ten_cell(left: [u64; 5], right: [u64; 5]) -> Result<[u64; 5], Tip5Error> {
    let words10: [u64; 10] = [
        left[0], left[1], left[2], left[3], left[4], right[0], right[1], right[2], right[3],
        right[4],
    ];
    hash_10_words(&words10)
}

fn leaf_sequence(noun: Noun, arena: &Arena, out: &mut Vec<u64>) -> Result<(), Tip5Error> {
    let mut stack: Vec<Noun> = Vec::new();
    stack.push(noun);
    while let Some(n) = stack.pop() {
        match n {
            Noun::Atom(id) => {
                let Some(v) = arena.atom_u64(id) else {
                    return Err(Tip5Error::NotBased);
                };
                if !is_based(v) {
                    return Err(Tip5Error::NotBased);
                }
                out.push(v);
            }
            Noun::Cell(id) => {
                let cell = arena.cell(id);
                // DFS: head then tail => push tail first.
                stack.push(cell.tail);
                stack.push(cell.head);
            }
        }
    }
    Ok(())
}

fn dyck_sequence(noun: Noun, arena: &Arena, out: &mut Vec<u64>) -> Result<(), Tip5Error> {
    enum Frame {
        Node(Noun),
        AfterHead(Noun),
    }

    let mut stack: Vec<Frame> = Vec::new();
    stack.push(Frame::Node(noun));
    while let Some(frame) = stack.pop() {
        match frame {
            Frame::Node(n) => match n {
                Noun::Atom(_) => {}
                Noun::Cell(id) => {
                    let cell = arena.cell(id);
                    out.push(0);
                    stack.push(Frame::AfterHead(cell.tail));
                    stack.push(Frame::Node(cell.head));
                }
            },
            Frame::AfterHead(tail) => {
                out.push(1);
                stack.push(Frame::Node(tail));
            }
        }
    }
    Ok(())
}

/// Hash a noun using the TIP5 `hash-noun-varlen` algorithm.
pub fn hash_noun_varlen(noun: Noun, arena: &Arena) -> Result<[u64; 5], Tip5Error> {
    let mut leaf: Vec<u64> = Vec::new();
    let mut dyck: Vec<u64> = Vec::new();
    leaf_sequence(noun, arena, &mut leaf)?;
    dyck_sequence(noun, arena, &mut dyck)?;

    let mut transcript: Vec<u64> = Vec::with_capacity(1 + leaf.len() + dyck.len());
    let size = leaf.len() as u64;
    if !is_based(size) {
        return Err(Tip5Error::NotBased);
    }
    transcript.push(size);
    transcript.extend_from_slice(&leaf);
    transcript.extend_from_slice(&dyck);

    hash_varlen_words(&transcript)
}
