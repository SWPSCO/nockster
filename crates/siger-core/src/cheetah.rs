extern crate alloc;
use alloc::vec::Vec;
use core::cmp::Ordering;
use hmac::{Hmac, Mac};
use sha2::{Sha256, Sha512};

use crate::math::math::{Belt, tip5_permute, bpegcd};


// ---- Constants --------------------------------------------------------------

const F6_ZERO: F6lt = F6lt([Belt(0); 6]);
const F6_ONE:  F6lt = F6lt([Belt(1), Belt(0), Belt(0), Belt(0), Belt(0), Belt(0)]);

const GX: F6lt = F6lt([
    Belt(2_754_435_735_642_750_112), Belt(1_644_450_330_024_117_627), Belt(10_514_417_644_467_316_420),
    Belt(14_161_929_407_758_026_907), Belt(14_987_836_057_278_945_216), Belt(6_227_780_353_604_379_289),
]);
const GY: F6lt = F6lt([
    Belt(9_410_740_712_825_754_944), Belt(6_042_221_554_322_127_113), Belt(3_212_109_399_280_097_325),
    Belt(12_822_852_497_658_382_218), Belt(8_300_084_201_729_938_365), Belt(8_030_207_336_092_068_342),
]);

/// Domain tag for the seed
const MASTER_KEY_TAG: &[u8] = b"Nockchain seed";

/// Group order n for Cheetah as big-endian bytes.
const GROUP_ORDER_BE: [u8; 32] = [
    0x7a, 0xf2, 0x59, 0x9b, 0x3b, 0x3f, 0x22, 0xd0, 0x56, 0x3f, 0xbf, 0x0f, 0x99, 0x0a, 0x37, 0xb5,
    0x32, 0x7a, 0xa7, 0x23, 0x30, 0x15, 0x77, 0x22, 0xd4, 0x43, 0x62, 0x3e, 0xae, 0xd4, 0xac, 0xcf,
];

const G: CheetahPoint = CheetahPoint { x: GX, y: GY, inf: false };
const A_ID: CheetahPoint = CheetahPoint { x: F6_ZERO, y: F6_ONE, inf: true };

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hash {
    pub values: [u64; 5],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct T8 {
    pub values: [u64; 8],
}

// F_{p^6} tower elements as six Belt limbs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct F6lt([Belt; 6]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CheetahPoint {
    x: F6lt,
    y: F6lt,
    inf: bool,
}

// ---- SLIP master/child derivation ---------------------------------

type HmacSha512 = Hmac<Sha512>;

#[inline]
fn is_zero32(x: &[u8; 32]) -> bool {
    let mut z = 0u8;
    for b in x {
        z |= *b;
    }
    z == 0
}

#[inline]
fn cmp_be32(a: &[u8; 32], b: &[u8; 32]) -> Ordering {
    for i in 0..32 {
        match a[i].cmp(&b[i]) {
            Ordering::Equal => continue,
            non_eq => return non_eq,
        }
    }
    Ordering::Equal
}

#[inline]
fn sub_be32(a: &[u8; 32], b: &[u8; 32]) -> ([u8; 32], u8) {
    // returns (a-b, borrow) in big-endian
    let mut out = [0u8; 32];
    let mut borrow: u16 = 0;
    for i in (0..32).rev() {
        let ai = a[i] as u16;
        let bi = b[i] as u16;
        let t = 256 + ai - bi - borrow;
        out[i] = (t & 0xff) as u8;
        borrow = if t >= 256 { 0 } else { 1 };
    }
    (out, borrow as u8)
}

#[inline]
fn add_be32(a: &[u8; 32], b: &[u8; 32]) -> ([u8; 32], u8) {
    // returns (a+b, carry) in big-endian
    let mut out = [0u8; 32];
    let mut carry: u16 = 0;
    for i in (0..32).rev() {
        let t = a[i] as u16 + b[i] as u16 + carry;
        out[i] = (t & 0xff) as u8;
        carry = t >> 8;
    }
    (out, carry as u8)
}

#[inline]
fn add_mod_n(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (sum, carry) = add_be32(a, b);
    // If carry set OR sum >= n, subtract n once.
    let need_sub = carry == 1 || cmp_be32(&sum, &GROUP_ORDER_BE) != Ordering::Less;
    if need_sub {
        let (out, _borrow) = sub_be32(&sum, &GROUP_ORDER_BE);
        out
    } else {
        sum
    }
}

#[inline]
fn mul_mod_n(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    // schoolbook 256x256 -> 512 then mod n using simple remainder
    let mut prod = [0u8; 64];
    for i in 0..32 {
        let mut carry: u32 = 0;
        for j in 0..32 {
            let ai = a[31 - i] as u32;
            let bj = b[31 - j] as u32;
            let k = 63 - (i + j);
            let t = ai * bj + prod[k] as u32 + carry;
            prod[k] = (t & 0xff) as u8;
            carry = t >> 8;
        }
        let k = 63 - (i + 32);
        // propagate remaining carry
        let mut kk = k;
        let mut c = carry;
        while c != 0 {
            let t = prod[kk] as u32 + c;
            prod[kk] = (t & 0xff) as u8;
            c = t >> 8;
            if kk == 0 { break; }
            kk -= 1;
        }
    }
    mod_n_from_be_bytes(&prod)
}

#[inline]
fn mod_n_from_be_bytes(bytes_be: &[u8]) -> [u8; 32] {
    // remainder by iterative (rem*256 + byte) % n
    let mut rem = [0u8; 32];
    for &b in bytes_be {
        // rem <<= 8
        let mut carry = b as u16;
        for i in (0..32).rev() {
            let t = ((rem[i] as u16) << 8) | carry;
            rem[i] = (t & 0xff) as u8;
            carry = (t >> 8) as u16; // always <= 0xff
        }
        // reduce while rem >= n (this at most 1–2 times thanks to left-shift-by-8)
        if cmp_be32(&rem, &GROUP_ORDER_BE) != Ordering::Less {
            let (tmp, _) = sub_be32(&rem, &GROUP_ORDER_BE);
            rem = tmp;
            if cmp_be32(&rem, &GROUP_ORDER_BE) != Ordering::Less {
                let (tmp2, _) = sub_be32(&rem, &GROUP_ORDER_BE);
                rem = tmp2;
            }
        }
    }
    rem
}

fn hmac_split_512(key: &[u8], data: &[u8]) -> ([u8; 32], [u8; 32]) {
    let mut mac = HmacSha512::new_from_slice(key).expect("HMAC key");
    mac.update(data);
    let out = mac.finalize().into_bytes();
    let mut left = [0u8; 32];
    let mut right = [0u8; 32];
    left.copy_from_slice(&out[..32]);
    right.copy_from_slice(&out[32..]);
    (left, right)
}

/// Create master (sk, chain_code) from a seed.
pub fn master_from_seed(seed: &[u8]) -> ([u8; 32], [u8; 32]) {
    // Loop until left is 0 < left < n
    let mut sk = [0u8; 32];
    let mut cc = [0u8; 32];
    let mut i = 0u8;
    loop {
        let mut msg = Vec::with_capacity(seed.len() + 1);
        msg.extend_from_slice(seed);
        msg.push(i);
        let (left, right) = hmac_split_512(MASTER_KEY_TAG, &msg);
        if !is_zero32(&left) && cmp_be32(&left, &GROUP_ORDER_BE) == Ordering::Less {
            sk = left;
            cc = right;
            break;
        }
        i = i.wrapping_add(1);
    }
    (sk, cc)
}

/// Serialize a 256-bit scalar (big-endian) to bytes.
fn ser256_be(x: &[u8; 32]) -> [u8; 32] { *x }

fn ser32_be(i: u32) -> [u8; 4] { i.to_be_bytes() }

/// Serialize affine point limbs (x,y) into 96 little-endian bytes (12 u64s).
fn ser_a_pt(pt: &([u64; 6], [u64; 6])) -> [u8; 96] {
  let mut out = [0u8; 96];
  for (li, limb) in pt.0.iter().enumerate() {
      out[li * 8..li * 8 + 8].copy_from_slice(&limb.to_le_bytes());
  }
  for (li, limb) in pt.1.iter().enumerate() {
      let o = 48 + li * 8;
      out[o..o + 8].copy_from_slice(&limb.to_le_bytes());
  }
  out
}

// Deterministic k (RFC6979)

fn rfc6979_k(sk_be: &[u8; 32], personalization: &[u8]) -> [u8; 32] {
    // k = HMAC-SHA256(sk, personalization) mod n, reject 0
    let mut mac = Hmac::<Sha256>::new_from_slice(sk_be).expect("hmac key");
    mac.update(personalization);
    let digest = mac.finalize().into_bytes(); // 32
    let mut k = [0u8; 32];
    k.copy_from_slice(&digest);
    let k = mod_n_from_be_bytes(&k);
    // ensure non-zero
    if is_zero32(&k) { [1u8; 32] } else { k }
}

// TIP5 helpers
const RATE: usize = 8;            // 8 x u64 words absorbed per block
const DIGEST_LENGTH: usize = 5;   // 5 x u64 output

fn tip5_hash_words(words: &[u64]) -> [u64; DIGEST_LENGTH] {
    // sponge over 64-bit words
    let mut state = [0u64; 16];
    let mut offset = 0usize;
    while offset < words.len() {
        let block = &words[offset..core::cmp::min(offset + RATE, words.len())];
        for (i, &w) in block.iter().enumerate() {
            state[i] ^= w;
        }
        tip5_permute(&mut state);
        offset += RATE;
    }
    [state[0], state[1], state[2], state[3], state[4]]
}

fn pack_point_words(pt: &([u64; 6], [u64; 6])) -> [u64; 12] {
  let mut out = [0u64; 12];
  out[..6].copy_from_slice(&pt.0);
  out[6..].copy_from_slice(&pt.1);
  out
}


// Turn 40-byte (5 words) big-endian into hex ASCII for ibig-less mod; but we
// already reduce via byte math, so we just keep it as bytes.

// Curve math over F_{p^6}
// Field helpers
#[inline] fn f6_add(a: &F6lt, b: &F6lt) -> F6lt {
    F6lt([
        a.0[0] + b.0[0], a.0[1] + b.0[1], a.0[2] + b.0[2],
        a.0[3] + b.0[3], a.0[4] + b.0[4], a.0[5] + b.0[5],
    ])
}
#[inline] fn f6_neg(a: &F6lt) -> F6lt {
    F6lt([ -a.0[0], -a.0[1], -a.0[2], -a.0[3], -a.0[4], -a.0[5] ])
}
#[inline] fn f6_sub(a: &F6lt, b: &F6lt) -> F6lt { f6_add(a, &f6_neg(b)) }

#[inline] fn f6_scal(a: &F6lt, s: Belt) -> F6lt {
    F6lt([ a.0[0]*s, a.0[1]*s, a.0[2]*s, a.0[3]*s, a.0[4]*s, a.0[5]*s ])
}

#[inline] fn f6_mul(a: &F6lt, b: &F6lt) -> F6lt {
    // Karatsuba-3 per the original
    let t0 = a.0[0]*b.0[0] + a.0[2]*b.0[4] + a.0[4]*b.0[2];
    let t1 = a.0[1]*b.0[1] + a.0[3]*b.0[5] + a.0[5]*b.0[3];
    let t2 = a.0[0]*b.0[2] + a.0[2]*b.0[0] + a.0[4]*b.0[4];
    let t3 = a.0[1]*b.0[3] + a.0[3]*b.0[1] + a.0[5]*b.0[5];
    let t4 = a.0[0]*b.0[4] + a.0[2]*b.0[2] + a.0[4]*b.0[0];
    let t5 = a.0[1]*b.0[5] + a.0[3]*b.0[3] + a.0[5]*b.0[1];
    F6lt([t0,t1,t2,t3,t4,t5])
}

#[inline] fn f6_square(a: &F6lt) -> F6lt { f6_mul(a, a) }

fn f6_inv(a: &F6lt) -> F6lt {
    // Inverse via extended GCD in the tower (ported, depends on zkvm_jetpack math)
    // Zero has “inverse” zero by convention.
    if a.0.iter().all(|&x| x == Belt(0)) { return *a; }

    // Build polynomial for a and mod polynomial μ; run extended GCD; extract inverse.
    let mut u = [Belt(0); 7];
    u[..6].copy_from_slice(&a.0);
    let mu = [
        Belt(1), Belt(0), Belt(0), Belt(0), Belt(0), Belt(0), Belt(1)
    ];

    // Extended GCD over polynomials in Belt
    let (s, v) = bpegcd(&u, &mu);
    // Scalar inverses
    let inv_v0 = v[0].inv();
    let inv_v1 = v[1].inv();
    let s0 = f6_scal(&F6lt([s[0], s[1], s[2], s[3], s[4], s[5]]), inv_v0);
    let s1 = f6_scal(&F6lt([s[6], s[7], s[8], s[9], s[10], s[11]]), inv_v1);
    f6_sub(&s0, &s1)
}

#[inline] fn f6_div(a: &F6lt, b: &F6lt) -> F6lt { f6_mul(a, &f6_inv(b)) }

// Curve ops
fn ch_add_unsafe(p: &CheetahPoint, q: &CheetahPoint) -> CheetahPoint {
    let x1 = p.x; let y1 = p.y; let x2 = q.x; let y2 = q.y;

    if p.inf { return *q; }
    if q.inf { return *p; }

    if x1 == x2 {
        if y1 == f6_neg(&y2) { return A_ID; } else { return ch_double_unsafe(p); }
    }

    let s = f6_div(&f6_sub(&y2, &y1), &f6_sub(&x2, &x1));
    let s2 = f6_square(&s);
    let x3 = f6_sub(&f6_sub(&s2, &x1), &x2);
    let y3 = f6_sub(&f6_mul(&s, &f6_sub(&x1, &x3)), &y1);

    CheetahPoint { x: x3, y: y3, inf: false }
}
fn ch_add(p: &CheetahPoint, q: &CheetahPoint) -> CheetahPoint { ch_add_unsafe(p, q) }

fn ch_double_unsafe(p: &CheetahPoint) -> CheetahPoint {
    if p.inf { return *p; }
    let x = p.x; let y = p.y;

    if y == F6_ZERO { return A_ID; }

    // λ = (3x^2 + a) / (2y), but a = 0 here per curve form used
    let three = Belt(3);
    let two = Belt(2);

    let s = f6_div(&f6_scal(&f6_square(&x), three), &f6_scal(&y, two));
    let s2 = f6_square(&s);
    let x3 = f6_sub(&f6_sub(&s2, &x), &x);
    let y3 = f6_sub(&f6_mul(&s, &f6_sub(&x, &x3)), &y);

    CheetahPoint { x: x3, y: y3, inf: false }
}
fn ch_double(p: &CheetahPoint) -> CheetahPoint { ch_double_unsafe(p) }

fn ch_scal_big(k_be: &[u8; 32], p: &CheetahPoint) -> CheetahPoint {
    let mut acc = A_ID;
    let mut base = *p;
    // LSB-first bit walk: iterate over 256 bits from LSB
    for bit in 0..256 {
        let byte = k_be[31 - (bit / 8)];
        if ((byte >> (bit % 8)) & 1) == 1 {
            acc = ch_add(&acc, &base);
        }
        base = ch_double(&base);
    }
    acc
}

// Public Cheetah API 
//
/// Compute affine (x,y) for the secret scalar `sk_be` (big-endian).
pub fn cheetah_pub_from_sk(sk_be: [u8; 32]) -> ([u64; 6], [u64; 6]) {
    let p = ch_scal_big(&sk_be, &G);
    let mut x = [0u64; 6];
    let mut y = [0u64; 6];
    for i in 0..6 {
        x[i] = p.x.0[i].0 as u64;
        y[i] = p.y.0[i].0 as u64;
    }
    (x, y)
}

/// Schnorr signature (challenge, response) tuple over Cheetah
/// - `sk_be`: 32-byte big-endian secret
/// - `pk`:   affine pubkey ([x;6], [y;6]) from `cheetah_pub_from_sk`
/// - `txid`: TIP-5 hash (5 words)
pub fn schnorr_sign_txid(sk_be: [u8; 32], pk: ([u64; 6], [u64; 6]), txid: Hash) -> (T8, T8) {
    // Deterministic nonce personalization = ser(P) || txid (bytes)
    let p_words = pack_point_words(&pk);
    let p_ser = ser_a_pt(&pk);
    let mut personalization = Vec::with_capacity(96 + 40);
    personalization.extend_from_slice(&p_ser);
    {
        let mut tmp = [0u8; 40];
        // txid words are u64 little-endian each
        for (i, w) in txid.values.iter().enumerate() {
            tmp[i * 8..i * 8 + 8].copy_from_slice(&w.to_le_bytes());
        }
        personalization.extend_from_slice(&tmp);
    }

    let k_be = rfc6979_k(&sk_be, &personalization);
    let r_pt = cheetah_pub_from_sk(k_be);
    let r_words = pack_point_words(&r_pt);

    // e = TIP5( R || P || txid )
    let mut words = Vec::with_capacity(24 + 24 + 5);
    words.extend_from_slice(&r_words);
    words.extend_from_slice(&p_words);
    words.extend_from_slice(&txid.values);
    let e_words = tip5_hash_words(&words);

    // Turn 5-word digest -> 40 bytes big-endian -> reduce mod n to 32-bytes
    let mut e_be40 = [0u8; 40];
    for (i, w) in e_words.iter().enumerate() {
        e_be40[i * 8..i * 8 + 8].copy_from_slice(&w.to_be_bytes());
    }
    let e_be = mod_n_from_be_bytes(&e_be40);

    // s = (k + e*sk) mod n
    let e_times_sk = mul_mod_n(&e_be, &sk_be);
    let s_be = add_mod_n(&k_be, &e_times_sk);

    // Pack outputs:
    // challenge T8 := [e_words (5 u64)] || ZERO (pad to 8)
    // response  T8 := big-endian s into 4 u64 (top-padded) || ZERO (pad to 8)
    let chal = {
        let mut v = [0u64; 8];
        v[..5].copy_from_slice(&e_words);
        T8 { values: v }
    };
    let sig = {
        // write s_be into 4 u64 big-endian limbs
        let mut v = [0u64; 8];
        for i in 0..4 {
            let off = i * 8;
            v[i] = u64::from_be_bytes([
                s_be[off], s_be[off + 1], s_be[off + 2], s_be[off + 3],
                s_be[off + 4], s_be[off + 5], s_be[off + 6], s_be[off + 7],
            ]);
        }
        T8 { values: v }
    };

    (chal, sig)
}

// ---- SLIP-10 child derivation (hardened + non-hardened) --------------------

#[derive(Clone, Debug)]
pub struct XKey {
    pub depth: u8,
    pub index: u32,
    pub chain_code: [u8; 32],
    pub sk: Option<[u8; 32]>,           // None for xpub
    pub pk: Option<([u64; 6], [u64; 6])>, // present for xpub or when sk present
    pub parent_fingerprint: [u8; 4],
}

fn fingerprint_from_pk(pk: &([u64; 6], [u64; 6])) -> [u8; 4] {
    // TIP-5 over P, take first 4 bytes of digest[0]
    let packed = pack_point_words(pk);
    let digest = tip5_hash_words(&packed);
    digest[0].to_be_bytes()[..4].try_into().unwrap()
}

pub fn xprv_derive_child(parent: &XKey, i: u32) -> XKey {
    let hardened = (i & 0x8000_0000) != 0;
    let mut data = Vec::with_capacity(1 + 32 + 4 + 96);
    if hardened {
        data.push(0);
        data.extend_from_slice(&ser256_be(&parent.sk.unwrap()));
    } else {
        let pk = parent.pk.as_ref().expect("parent pk");
        data.extend_from_slice(&ser_a_pt(pk));
    }
    data.extend_from_slice(&ser32_be(i));

    let (il, ir) = hmac_split_512(&parent.chain_code, &data);

    // child sk = (il + parent_sk) mod n; reject il==0 or child==0
    if is_zero32(&il) || cmp_be32(&il, &GROUP_ORDER_BE) != Ordering::Less {
        return xprv_derive_child(parent, i.wrapping_add(1)); // rare retry
    }
    let child_sk = add_mod_n(&il, &parent.sk.unwrap());
    if is_zero32(&child_sk) {
        return xprv_derive_child(parent, i.wrapping_add(1));
    }

    let child_pk = cheetah_pub_from_sk(child_sk);
    XKey {
        depth: parent.depth.saturating_add(1),
        index: i,
        chain_code: ir,
        sk: Some(child_sk),
        pk: Some(child_pk),
        parent_fingerprint: fingerprint_from_pk(&parent.pk.unwrap()),
    }
}

pub fn xpub_derive_child(parent: &XKey, i: u32) -> XKey {
    // Non-hardened only (per SLIP-10 rules)
    assert_eq!(i & 0x8000_0000, 0);
    let mut data = Vec::with_capacity(96 + 4);
    data.extend_from_slice(&ser_a_pt(parent.pk.as_ref().unwrap()));
    data.extend_from_slice(&ser32_be(i));
    let (il, ir) = hmac_split_512(&parent.chain_code, &data);

    if is_zero32(&il) || cmp_be32(&il, &GROUP_ORDER_BE) != Ordering::Less {
        return xpub_derive_child(parent, i.wrapping_add(1));
    }

    // child P = (il * G) + parent P
    let q = ch_scal_big(&il, &G);
    let p = CheetahPoint {
        x: F6lt(parent.pk.unwrap().0.map(|w| Belt(w))),
        y: F6lt(parent.pk.unwrap().1.map(|w| Belt(w))),
        inf: false,
    };
    let r = ch_add(&q, &p);
    let child_pk = {
        let mut x = [0u64; 6];
        let mut y = [0u64; 6];
        for j in 0..6 {
            x[j] = r.x.0[j].0 as u64;
            y[j] = r.y.0[j].0 as u64;
        }
        (x, y)
    };

    XKey {
        depth: parent.depth.saturating_add(1),
        index: i,
        chain_code: ir,
        sk: None,
        pk: Some(child_pk),
        parent_fingerprint: fingerprint_from_pk(&parent.pk.unwrap()),
    }
}

impl XKey {
    /// Construct a master extended private key (depth=0, index=0)
    /// Parent fingerprint for the master is 0x00000000
    pub fn from_master(sk: [u8; 32], chain_code: [u8; 32]) -> Self {
        let pk = cheetah_pub_from_sk(sk);
        XKey {
            depth: 0,
            index: 0,
            chain_code,
            sk: Some(sk),
            pk: Some(pk),
            parent_fingerprint: [0u8; 4],
        }
    }

    /// convenience for xpub-only view
    #[allow(dead_code)]
    pub fn to_xpub(&self) -> Self {
        let pk = self.pk.expect("xkey must have pk");
        XKey {
            depth: self.depth,
            index: self.index,
            chain_code: self.chain_code,
            sk: None,
            pk: Some(pk),
            parent_fingerprint: self.parent_fingerprint,
        }
    }
}
