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
    Belt(2_754_611_494_552_410_273),
    Belt(8_599_518_745_794_843_693),
    Belt(10_526_511_002_404_673_680),
    Belt(4_830_863_958_577_994_148),
    Belt(375_185_138_577_093_320),
    Belt(12_938_930_721_685_970_739),
]);
const GY: F6lt = F6lt([
    Belt(15_384_029_202_802_550_068),
    Belt(2_774_812_795_997_841_935),
    Belt(14_375_303_400_746_062_753),
    Belt(10_708_493_419_890_101_954),
    Belt(13_187_678_623_570_541_764),
    Belt(9_990_732_138_772_505_951),
]);


/// Domain tag for the seed
const MASTER_KEY_TAG: &[u8] = b"Nockchain seed";

/// Group order n for Cheetah as big-endian bytes.
const GROUP_ORDER_BE: [u8; 32] = [
    0x7a, 0xf2, 0x59, 0x9b, 0x3b, 0x3f, 0x22, 0xd0, 0x56, 0x3f, 0xbf, 0x0f, 0x99, 0x0a, 0x37, 0xb5,
    0x32, 0x7a, 0xa7, 0x23, 0x30, 0x15, 0x77, 0x22, 0xd4, 0x43, 0x62, 0x3e, 0xae, 0xd4, 0xac, 0xcf,
];

const NOCKCHAIN_SLIP10_KEY: &[u8] = b"Nockchain seed"; 

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

pub fn hmac_split_512(key: &[u8], data: &[u8]) -> ([u8; 32], [u8; 32]) {
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
    let mut mac = HmacSha512::new_from_slice(NOCKCHAIN_SLIP10_KEY).unwrap();
    mac.update(seed);
    let mut i = mac.finalize().into_bytes(); // 64 bytes

    loop {
        let mut left  = [0u8; 32];
        let mut right = [0u8; 32];
        left.copy_from_slice(&i[..32]);
        right.copy_from_slice(&i[32..]);

        if !be32_is_zero(&left) && be32_lt(&left, &CHEETAH_N) {
            return (left, right);
        }

        // rehash whole 64B per Hoon parity
        let mut mac = HmacSha512::new_from_slice(NOCKCHAIN_SLIP10_KEY).unwrap();
        mac.update(&i);
        i = mac.finalize().into_bytes();
    }
}

/// Serialize a 256-bit scalar (big-endian) to bytes.
fn ser256_be(x: &[u8; 32]) -> [u8; 32] { *x }

/// Serialize affine point limbs (x,y) into 96 little-endian bytes (12 u64s).
/// Serialize affine point into the exact Hoon/Tip5 parity format:
/// Treat [x0..x5, y0..y5, 1] as 13 little-endian 64-bit limbs of a single
/// big integer (Hoon `rep 6`), then emit that integer in canonical
/// **big-endian** bytes (104 bytes).
pub fn ser_a_pt(pk: &([u64; 6], [u64; 6])) -> [u8; 97] {
  let mut out = [0u8; 97];
  out[0] = 0x01;
  let mut off = 1;
  // Keygen: Y first, then X; limbs serialized little-endian
  for &w in pk.1.iter().chain(pk.0.iter()) {
      out[off..off + 8].copy_from_slice(&w.to_le_bytes());
      off += 8;
  }
  out
}

pub fn ser_a_pt_rep104(pk: &([u64; 6], [u64; 6])) -> [u8; 104] {
    let mut limbs = [0u64; 13];
    limbs[0] = 1;                           // LS limb = sentinel
    // Y first, then X (each y0/x0 is the LS limb within that coordinate)
    limbs[1..7].copy_from_slice(&pk.1);     // y0..y5
    limbs[7..13].copy_from_slice(&pk.0);    // x0..x5

    // Write MS→LS limbs as big-endian bytes.
    let mut out = [0u8; 104];
    let mut off = 0;
    for i in (0..13).rev() {
        out[off..off + 8].copy_from_slice(&limbs[i].to_be_bytes());
        off += 8;
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

#[inline]
fn karat3(a0: Belt, a1: Belt, a2: Belt, b0: Belt, b1: Belt, b2: Belt)
-> (Belt,Belt,Belt,Belt,Belt) {
    let m0 = a0*b0;
    let m1 = a1*b1;
    let m2 = a2*b2;
    let t0 = (a0+a1)*(b0+b1) - (m0+m1);
    let t1 = (a0+a2)*(b0+b2) - (m0+m2);
    let t2 = (a1+a2)*(b1+b2) - (m1+m2);
    (m0, t0, t1 - m1, t2, m2)
}

#[inline]
fn karat3_square(a0: Belt, a1: Belt, a2: Belt)
-> (Belt,Belt,Belt,Belt,Belt) {
    let m0  = a0*a0;
    let m2  = a2*a2;
    let n01 = (a0*a1)*Belt(2);
    let n12 = (a1*a2)*Belt(2);
    let tri = (a0 + a1 + a2);
    let tri2 = tri*tri;
    let coeff2 = tri2 - (m0 + m2 + n01 + n12);
    (m0, n01, coeff2, n12, m2)
}

#[inline]
fn f6_mul(a: &F6lt, b: &F6lt) -> F6lt {
    let (a0,a1,a2,a3,a4,a5) = (a.0[0],a.0[1],a.0[2],a.0[3],a.0[4],a.0[5]);
    let (b0,b1,b2,b3,b4,b5) = (b.0[0],b.0[1],b.0[2],b.0[3],b.0[4],b.0[5]);

    let (f00,f01,f02,f03,f04) = karat3(a0,a1,a2, b0,b1,b2);
    let (f10,f11,f12,f13,f14) = karat3(a3,a4,a5, b3,b4,b5);
    let (g00,g01,g02,g03,g04) = karat3(a0+a3,a1+a4,a2+a5, b0+b3,b1+b4,b2+b5);

    // cross = foil - (f0g0 + f1g1)
    let c0 = g00 - (f00 + f10);
    let c1 = g01 - (f01 + f11);
    let c2 = g02 - (f02 + f12);
    let c3 = g03 - (f03 + f13);
    let c4 = g04 - (f04 + f14);

    // reduction mod x^6 - 7 (wrap with factor 7)
    F6lt([
        f00 + Belt(7)*(c3 + f10),
        f01 + Belt(7)*(c4 + f11),
        f02 + Belt(7)*f12,
        f03 + c0 + Belt(7)*f13,
        f04 + c1 + Belt(7)*f14,
        c2,
    ])
}

#[inline]
fn f6_square(a: &F6lt) -> F6lt {
    let (a0,a1,a2,a3,a4,a5) = (a.0[0],a.0[1],a.0[2],a.0[3],a.0[4],a.0[5]);
    let (lo0,lo1,lo2,lo3,lo4) = karat3_square(a0,a1,a2);
    let (hi0,hi1,hi2,hi3,hi4) = karat3_square(a3,a4,a5);
    let (fd0,fd1,fd2,fd3,fd4) = karat3_square(a0+a3, a1+a4, a2+a5);

    // cross = folded2 - (lo2 + hi2)
    let c0 = fd0 - (lo0 + hi0);
    let c1 = fd1 - (lo1 + hi1);
    let c2 = fd2 - (lo2 + hi2);
    let c3 = fd3 - (lo3 + hi3);
    let c4 = fd4 - (lo4 + hi4);

    F6lt([
        lo0 + Belt(7)*c3 + Belt(7)*hi0,
        lo1 + Belt(7)*c4 + Belt(7)*hi1,
        lo2 + Belt(7)*hi2,
        lo3 + c0 + Belt(7)*hi3,
        lo4 + c1 + Belt(7)*hi4,
        c2,
    ])
}

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

    let three = Belt(3);
    let two   = Belt(2);

    // slope = (3*x^2 + 1) / (2*y)
    let num  = f6_add(&f6_scal(&f6_square(&x), three), &F6_ONE);
    let den  = f6_scal(&y, two);
    let s    = f6_div(&num, &den);

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
    let p_ser = ser_a_pt_rep104(&pk);
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
  let prv = parent.sk.expect("need private key");
  let cc  = parent.chain_code;

  const APT_SER_LEN: usize = 104;

  let (mut left, mut right) = if is_hardened(i) {
      // data = 0x00 || ser256(prv) || ser32(i)
      let mut data = [0u8; 1 + 32 + 4];
      data[0] = 0;
      data[1..33].copy_from_slice(&prv);
      data[33..].copy_from_slice(&ser32_be(i));
      hmac_split_512(&cc, &data)
  } else {
      // data = ser_a_pt(P) || ser32(i)
      let pk_xy   = parent.pk.unwrap_or_else(|| cheetah_pub_from_sk(prv));
      let pub_ser = ser_a_pt_rep104(&pk_xy);              // 97 bytes (X||Y||inf)
      let mut data = [0u8; APT_SER_LEN + 4];
      data[..APT_SER_LEN].copy_from_slice(&pub_ser);
      data[APT_SER_LEN..].copy_from_slice(&ser32_be(i));
      hmac_split_512(&cc, &data)
  };


  // Retry until 0 < left < n and child != 0 using 0x01 || right || i
  for _ in 0..1024 {
      if !be32_is_zero(&left) && be32_lt(&left, &CHEETAH_N) {
          let child_sk = be32_add_mod_n(&left, &prv);
          if !be32_is_zero(&child_sk) {
              let child_pk = cheetah_pub_from_sk(child_sk);
              return XKey {
                  sk: Some(child_sk),
                  pk: Some(child_pk),
                  chain_code: right,
                  depth: parent.depth + 1,
                  index: i,
                  parent_fingerprint: parent.parent_fingerprint, // or recompute if you use it
              };
          }
      }

      // Next attempt seed: 0x01 || right || ser32(i)
      let mut red = [0u8; 1 + 32 + 4];
      red[0] = 0x01;
      red[1..33].copy_from_slice(&right);
      red[33..].copy_from_slice(&ser32_be(i));
      let (l2, r2) = hmac_split_512(&cc, &red);
      left = l2;
      right = r2;
  }

  panic!("xprv_derive_child: too many retries at index {}", i);
}


fn xkey_from_child_bytes(child_sk: [u8; 32], cc: [u8; 32], parent: &XKey, i: u32) -> XKey {
  let pk_xy = cheetah_pub_from_sk(child_sk);
  XKey {
      sk: Some(child_sk),
      pk: Some(pk_xy),
      chain_code: cc,
      depth: parent.depth + 1,
      index: i,
      parent_fingerprint: [0u8; 4],
  }
}

//////////
// --- Scalar math over 32-byte big-endian arrays (no_std friendly) ------------

/// Cheetah group order n as 32-byte big-endian
const CHEETAH_N: [u8; 32] = [
    0x7a,0xf2,0x59,0x9b,0x3b,0x3f,0x22,0xd0,0x56,0x3f,0xbf,0x0f,0x99,0x0a,0x37,0xb5,
    0x32,0x7a,0xa7,0x23,0x30,0x15,0x77,0x22,0xd4,0x43,0x62,0x3e,0xae,0xd4,0xac,0xcf,
];

#[inline] fn is_hardened(i: u32) -> bool { i & 0x8000_0000 != 0 }
#[inline] fn ser32_be(i: u32) -> [u8; 4] { i.to_be_bytes() }

#[inline] fn be32_is_zero(a: &[u8; 32]) -> bool { a.iter().fold(0u8, |acc, &b| acc | b) == 0 }

#[inline]
fn be32_lt(a: &[u8; 32], b: &[u8; 32]) -> bool {
    for k in 0..32 { if a[k] != b[k] { return a[k] < b[k]; } }
    false
}

#[inline]
fn be32_add(a: &[u8; 32], b: &[u8; 32]) -> ([u8; 32], u8) {
    let mut out = [0u8; 32];
    let mut c: u16 = 0;
    for k in (0..32).rev() {
        let s = a[k] as u16 + b[k] as u16 + c;
        out[k] = (s & 0xff) as u8;
        c = s >> 8;
    }
    (out, c as u8)
}

#[inline]
fn be32_sub_inplace(a: &mut [u8; 32], b: &[u8; 32]) {
    let mut brw: i16 = 0;
    for k in (0..32).rev() {
        let v = a[k] as i16 - b[k] as i16 - brw;
        if v < 0 { a[k] = (v + 256) as u8; brw = 1; } else { a[k] = v as u8; brw = 0; }
    }
}

#[inline]
fn be32_add_mod_n(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (mut sum, carry) = be32_add(a, b);
    if carry == 1 || !be32_lt(&sum, &CHEETAH_N) {
        be32_sub_inplace(&mut sum, &CHEETAH_N);
    }
    sum
}

pub fn xpub_derive_child(parent: &XKey, i: u32) -> XKey {
    assert_eq!(i & 0x8000_0000, 0);

    let mut data = Vec::with_capacity(104 + 4);
    data.extend_from_slice(&ser_a_pt(parent.pk.as_ref().unwrap()));
    data.extend_from_slice(&ser32_be(i));

    let (mut left, mut right) = hmac_split_512(&parent.chain_code, &data);

    loop {
        if !is_zero32(&left) && cmp_be32(&left, &GROUP_ORDER_BE) == Ordering::Less {
            // child P = (left * G) + parent P
            let q  = ch_scal_big(&left, &G);
            let pp = parent.pk.unwrap();
            let p  = CheetahPoint { x: F6lt(pp.0.map(Belt)), y: F6lt(pp.1.map(Belt)), inf: false };
            let r  = ch_add(&q, &p);
            let child_pk = {
                let mut x = [0u64; 6]; let mut y = [0u64; 6];
                for j in 0..6 { x[j] = r.x.0[j].0 as u64; y[j] = r.y.0[j].0 as u64; }
                (x, y)
            };
            return XKey {
                depth: parent.depth.saturating_add(1),
                index: i,
                chain_code: right,
                sk: None,
                pk: Some(child_pk),
                parent_fingerprint: fingerprint_from_pk(&pp),
            };
        }

        // retry seed: 0x01 || right || ser32(i)
        let mut red = [0u8; 1 + 32 + 4];
        red[0] = 0x01;
        red[1..33].copy_from_slice(&right);
        red[33..].copy_from_slice(&ser32_be(i));
        (left, right) = hmac_split_512(&parent.chain_code, &red);
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


pub fn xprv_derive_child_traced(parent: &XKey, i: u32, sink: &mut impl TraceSink) -> XKey {
    let prv = parent.sk.expect("need private key");
    let cc  = parent.chain_code;
    const APT_SER_LEN: usize = 104;

    let hardened = is_hardened(i);
    let (mut left, mut right) = if hardened {
        // data = 0x00 || ser256(prv) || ser32(i)
        let mut data = [0u8; 1 + 32 + 4];
        data[0] = 0x00;
        data[1..33].copy_from_slice(&prv);
        data[33..].copy_from_slice(&ser32_be(i));
        sink.log(&format!("i={} mode=hardened HMAC_msg = {}", i, hex(&data)));
        hmac_split_512(&cc, &data)
    } else {
        // data = ser_a_pt(P) || ser32(i)
        let pk_xy   = parent.pk.unwrap_or_else(|| cheetah_pub_from_sk(prv));
        let pub_ser = ser_a_pt_rep104(&pk_xy); // 97 bytes (X||Y||0x01)
        let mut data = [0u8; APT_SER_LEN + 4];
        data[..APT_SER_LEN].copy_from_slice(&pub_ser);
        data[APT_SER_LEN..].copy_from_slice(&ser32_be(i));
        sink.log(&format!("i={} mode=normal   HMAC_msg = {}", i, hex(&data)));
        hmac_split_512(&cc, &data)
    };

    loop {
        sink.log(&format!("i={} IL = {}", i, hex(&left)));
        sink.log(&format!("i={} IR = {}", i, hex(&right)));

        if !be32_is_zero(&left) && be32_lt(&left, &CHEETAH_N) {
            let child_sk = be32_add_mod_n(&left, &prv);
            if !be32_is_zero(&child_sk) {
                let child_pk = cheetah_pub_from_sk(child_sk);
                let ser = ser_a_pt(&child_pk);
                sink.log(&format!("i={} child_sk = {}", i, hex(&child_sk)));
                sink.log(&format!("i={} child_P_ser97 = {}", i, hex(&ser)));
                return XKey {
                    sk: Some(child_sk),
                    pk: Some(child_pk),
                    chain_code: right,
                    depth: parent.depth.saturating_add(1),
                    index: i,
                    parent_fingerprint: parent.parent_fingerprint,
                };
            }
        }

        // retry: 0x01 || right || ser32(i)
        let mut red = [0u8; 1 + 32 + 4];
        red[0] = 0x01;
        red[1..33].copy_from_slice(&right);
        red[33..].copy_from_slice(&ser32_be(i));
        sink.log(&format!("i={} retry_seed = {}", i, hex(&red)));
        (left, right) = hmac_split_512(&cc, &red);
    }
}

pub fn xpub_derive_child_traced(parent: &XKey, i: u32, sink: &mut impl TraceSink) -> XKey {
    assert_eq!(i & 0x8000_0000, 0);
    const APT_SER_LEN: usize = 104;

    // data = ser_a_pt(P) || ser32(i)
    let pub_ser = ser_a_pt_rep104(parent.pk.as_ref().unwrap());
    let mut data = [0u8; APT_SER_LEN + 4];
    data[..APT_SER_LEN].copy_from_slice(&pub_ser);
    data[APT_SER_LEN..].copy_from_slice(&ser32_be(i));
    sink.log(&format!("i={} mode=normal   HMAC_msg = {}", i, hex(&data)));

    let mut cc = parent.chain_code;
    let (mut left, mut right) = hmac_split_512(&cc, &data);

    loop {
        sink.log(&format!("i={} IL = {}", i, hex(&left)));
        sink.log(&format!("i={} IR = {}", i, hex(&right)));

        if !is_zero32(&left) && cmp_be32(&left, &GROUP_ORDER_BE) == core::cmp::Ordering::Less {
            // child P = (IL * G) + parent P
            let q = ch_scal_big(&left, &G);
            let pp = parent.pk.unwrap();
            let p = CheetahPoint {
                x: F6lt(pp.0.map(Belt)),
                y: F6lt(pp.1.map(Belt)),
                inf: false,
            };
            let r = ch_add(&q, &p);
            let mut x = [0u64; 6]; let mut y = [0u64; 6];
            for j in 0..6 { x[j] = r.x.0[j].0 as u64; y[j] = r.y.0[j].0 as u64; }
            let child_pk = (x, y);
            let ser = ser_a_pt(&child_pk);
            sink.log(&format!("i={} child_P_ser97 = {}", i, hex(&ser)));

            return XKey {
                depth: parent.depth.saturating_add(1),
                index: i,
                chain_code: right,
                sk: None,
                pk: Some(child_pk),
                parent_fingerprint: fingerprint_from_pk(&parent.pk.unwrap()),
            };
        }

        // retry with 0x01 || IR || ser32(i)
        let mut red = [0u8; 1 + 32 + 4];
        red[0] = 0x01;
        red[1..33].copy_from_slice(&right);
        red[33..].copy_from_slice(&ser32_be(i));
        sink.log(&format!("i={} retry_seed = {}", i, hex(&red)));
        let out = hmac_split_512(&cc, &red);
        left = out.0; right = out.1;
    }
}

pub fn derive_path_with_trace(seed: &[u8], path: &[u32], sink: &mut impl TraceSink)
-> (XKey, String /*base58 of final ser_a_pt*/) {
    let (sk, cc) = master_from_seed_traced(seed, sink);
    let mut xk = XKey::from_master(sk, cc);

    for &i in path {
        xk = if is_hardened(i) {
            xprv_derive_child_traced(&xk, i, sink)
        } else if xk.sk.is_some() {
            // have xprv → stay in private space
            xprv_derive_child_traced(&xk, i, sink)
        } else {
            // xpub only
            xpub_derive_child_traced(&xk, i, sink)
        };
    }

    let pk = xk.pk.expect("final key must have pk");
    let ser = ser_a_pt(&pk);
    sink.log(&format!("FINAL ser_a_pt(97B) = {}", hex(&ser)));
    let b58 = base58_encode(&ser);
    sink.log(&format!("FINAL Base58 = {}", b58));
    (xk, b58)
}


////////// test below// --- Add this block ----------------------------------------------------------
use alloc::string::String;
use alloc::format;

// Simple tracing that doesn't require std I/O.
pub trait TraceSink { fn log(&mut self, s: &str); }

pub struct StringSink { buf: String }
impl StringSink {
    pub fn new() -> Self { Self { buf: String::new() } }
    pub fn into_string(self) -> String { self.buf }
}
impl TraceSink for StringSink {
    fn log(&mut self, s: &str) {
        self.buf.push_str(s);
        self.buf.push('\n');
    }
}

// Hex helper (alloc-only).
const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";
pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX_CHARS[(b >> 4) as usize] as char);
        out.push(HEX_CHARS[(b & 0x0f) as usize] as char);
    }
    out
}

// Base58 (Bitcoin alphabet), alloc-only, no std.
const B58_ALPHABET: &[u8] =
    b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
pub fn base58_encode(bytes: &[u8]) -> String {
    // Count leading zeros
    let mut zeros = 0;
    for &b in bytes {
        if b == 0 { zeros += 1; } else { break; }
    }

    // Make a mutable copy for in-place base256 -> base58 conversion
    let mut input = bytes.to_vec();
    let mut encoded: Vec<u8> = Vec::new();

    let mut start = zeros;
    while start < input.len() {
        let mut carry: u32 = 0;
        for i in start..input.len() {
            let val = (carry << 8) + input[i] as u32;
            input[i] = (val / 58) as u8;
            carry = val % 58;
        }
        encoded.push(B58_ALPHABET[carry as usize]);
        while start < input.len() && input[start] == 0 {
            start += 1;
        }
    }

    let mut out = String::with_capacity(zeros + encoded.len());
    for _ in 0..zeros { out.push('1'); }           // leading zero -> '1'
    for ch in encoded.iter().rev() { out.push(*ch as char); }
    out
}

// Traced master-from-seed (alloc-only logging).
pub fn master_from_seed_traced(seed: &[u8], sink: &mut impl TraceSink)
-> ([u8; 32], [u8; 32]) {
    let mut mac = HmacSha512::new_from_slice(NOCKCHAIN_SLIP10_KEY).unwrap();
    mac.update(seed);
    let mut i = mac.finalize().into_bytes(); // 64 bytes
    sink.log(&format!(
        "MASTER: I = HMAC-SHA512(key='Nockchain seed', seed) = {}",
        hex(&i)
    ));

    loop {
        let mut left  = [0u8; 32];
        let mut right = [0u8; 32];
        left.copy_from_slice(&i[..32]);
        right.copy_from_slice(&i[32..]);

        sink.log(&format!("MASTER: IL = {}", hex(&left)));
        sink.log(&format!("MASTER: IR = {}", hex(&right)));

        if !be32_is_zero(&left) && be32_lt(&left, &CHEETAH_N) {
            return (left, right);
        }

        // rehash whole 64B per spec
        let mut mac = HmacSha512::new_from_slice(NOCKCHAIN_SLIP10_KEY).unwrap();
        mac.update(&i);
        i = mac.finalize().into_bytes();
        sink.log("MASTER: invalid IL (zero or ≥n), rehashing I.");
    }
}

// Pretty printer for an XKey you can show in CLI.
pub fn format_xkey(xk: &XKey) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "depth={} index={}{}",
        xk.depth,
        xk.index & 0x7fff_ffff,
        if (xk.index & 0x8000_0000) != 0 { " (hardened)" } else { "" }
    ));
    s.push('\n');

    s.push_str("chain_code = ");
    s.push_str(&hex(&xk.chain_code));
    s.push('\n');

    if let Some(sk) = xk.sk {
        s.push_str("sk         = ");
        s.push_str(&hex(&sk));
        s.push('\n');
    } else {
        s.push_str("sk         = <none>\n");
    }

    if let Some((x, y)) = xk.pk {
        s.push_str("pk.x       = [");
        for (i, w) in x.iter().enumerate() {
            if i > 0 { s.push_str(", "); }
            s.push_str(&format!("{}", w));
        }
        s.push_str("]\n");
        s.push_str("pk.y       = [");
        for (i, w) in y.iter().enumerate() {
            if i > 0 { s.push_str(", "); }
            s.push_str(&format!("{}", w));
        }
        s.push_str("]\n");
    } else {
        s.push_str("pk         = <none>\n");
    }

    s
}

// Convenience: run the traced derivation and return a single printable blob.
pub fn derive_path_transcript(seed: &[u8], path: &[u32]) -> (XKey, String) {
    let mut sink = StringSink::new();
    let (xk, b58) = derive_path_with_trace(seed, path, &mut sink);
    let mut out = sink.into_string();
    out.push_str("----- SUMMARY -----\n");
    out.push_str(&format_xkey(&xk));
    out.push_str("base58(P)  = ");
    out.push_str(&b58);
    out.push('\n');
    (xk, out)
}
// ---------------------------------------------------------------------------
