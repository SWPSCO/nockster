//! cheetah.rs — consolidated, cleaned, no_std-friendly

#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::vec::Vec;
use core::cmp::Ordering;
use hmac::{Hmac, Mac};
use sha2::{Sha256, Sha512};

use crate::math::math::{Belt, tip5_permute, bpegcd_full};

// ---- Constants --------------------------------------------------------------

const F6_ZERO: F6lt = F6lt([Belt(0); 6]);
const F6_ONE:  F6lt = F6lt([Belt(1), Belt(0), Belt(0), Belt(0), Belt(0), Belt(0)]);

const A: F6lt = F6lt([Belt(1), Belt(0), Belt(0), Belt(0), Belt(0), Belt(0)]);
const B: F6lt = F6lt([Belt(395), Belt(1), Belt(0), Belt(0), Belt(0), Belt(0)]);

pub const GX: F6lt = F6lt([
    Belt(2_754_611_494_552_410_273),
    Belt(8_599_518_745_794_843_693),
    Belt(10_526_511_002_404_673_680),
    Belt(4_830_863_958_577_994_148),
    Belt(375_185_138_577_093_320),
    Belt(12_938_930_721_685_970_739),
]);
pub const GY: F6lt = F6lt([
    Belt(15_384_029_202_802_550_068),
    Belt(2_774_812_795_997_841_935),
    Belt(14_375_303_400_746_062_753),
    Belt(10_708_493_419_890_101_954),
    Belt(13_187_678_623_570_541_764),
    Belt(9_990_732_138_772_505_951),
]);

pub const G: CheetahPoint = CheetahPoint { x: GX, y: GY, inf: false };

/// Group order n as 32-byte big-endian
const CHEETAH_N: [u8; 32] = [
    0x7a,0xf2,0x59,0x9b,0x3b,0x3f,0x22,0xd0,0x56,0x3f,0xbf,0x0f,0x99,0x0a,0x37,0xb5,
    0x32,0x7a,0xa7,0x23,0x30,0x15,0x77,0x22,0xd4,0x43,0x62,0x3e,0xae,0xd4,0xac,0xcf,
];

const A_ID: CheetahPoint = CheetahPoint { x: F6_ZERO, y: F6_ONE, inf: true };
const NOCKCHAIN_SLIP10_KEY: &[u8] = b"Nockchain seed";

// ---- Types ------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hash {
    pub values: [u64; 5],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct T8 {
    pub values: [u64; 8],
}

// F_{p^6} element (tower) as six Belt limbs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct F6lt([Belt; 6]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheetahPoint {
    pub x: F6lt,
    pub y: F6lt,
    pub inf: bool,
}

// ---- Field ops over F_{p^6} (with u^6 ≡ +7) --------------------------------

#[inline]
fn f6_mul(a: &F6lt, b: &F6lt) -> F6lt {
    let mut h = [Belt(0); 11];
    for i in 0..6 {
        for j in 0..6 {
            h[i + j] = h[i + j] + (a.0[i] * b.0[j]);
        }
    }
    // u^6 ≡ +7  → fold highs down with +7
    for k in 6..=10 {
        h[k - 6] = h[k - 6] + (Belt(7) * h[k]);
    }
    F6lt([h[0], h[1], h[2], h[3], h[4], h[5]])
}

#[inline] fn f6_square(a: &F6lt) -> F6lt { f6_mul(a, a) }

#[inline] fn f6_add(a: &F6lt, b: &F6lt) -> F6lt {
    F6lt([
        a.0[0] + b.0[0], a.0[1] + b.0[1], a.0[2] + b.0[2],
        a.0[3] + b.0[3], a.0[4] + b.0[4], a.0[5] + b.0[5],
    ])
}
#[inline] fn f6_neg(a: &F6lt) -> F6lt {
    F6lt([-a.0[0], -a.0[1], -a.0[2], -a.0[3], -a.0[4], -a.0[5]])
}
#[inline] fn f6_sub(a: &F6lt, b: &F6lt) -> F6lt { f6_add(a, &f6_neg(b)) }
#[inline] fn f6_scal(a: &F6lt, s: Belt) -> F6lt {
    F6lt([ a.0[0]*s, a.0[1]*s, a.0[2]*s, a.0[3]*s, a.0[4]*s, a.0[5]*s ])
}

// inverse via extended GCD wrt μ(t) = t^6 - 7 (matches u^6 ≡ +7)
fn f6_inv(a: &F6lt) -> F6lt {
    if a.0.iter().all(|&x| x == Belt(0)) { return *a; }

    let mut u = [Belt(0); 7];
    u[..6].copy_from_slice(&a.0);

    // μ(t) = t^6 - 7
    let mu = [-Belt(7), Belt(0), Belt(0), Belt(0), Belt(0), Belt(0), Belt(1)];

    let (s, _t, d0) = bpegcd_full(&u, &mu);
    let inv_d0 = d0.inv();
    F6lt([ s[0]*inv_d0, s[1]*inv_d0, s[2]*inv_d0, s[3]*inv_d0, s[4]*inv_d0, s[5]*inv_d0 ])
}

#[inline] fn f6_div(a: &F6lt, b: &F6lt) -> F6lt { f6_mul(a, &f6_inv(b)) }

// ---- Curve ops --------------------------------------------------------------

#[inline]
fn is_on_curve(p: &CheetahPoint) -> bool {
    let y2 = f6_square(&p.y);
    let x2 = f6_square(&p.x);
    let x3 = f6_mul(&x2, &p.x);
    let ax = f6_mul(&A, &p.x);
    let rhs = f6_add(&f6_add(&x3, &ax), &B); // y^2 = x^3 + A*x + B
    y2 == rhs
}

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
#[inline] fn ch_add(p: &CheetahPoint, q: &CheetahPoint) -> CheetahPoint { ch_add_unsafe(p, q) }

fn ch_double_unsafe(p: &CheetahPoint) -> CheetahPoint {
    if p.inf { return *p; }
    let x = p.x; let y = p.y;
    if y == F6_ZERO { return A_ID; }

    let three = Belt(3);
    let two   = Belt(2);

    // a = 1  → slope = (3*x^2 + A) / (2*y)
    let num  = f6_add(&f6_scal(&f6_square(&x), three), &A);
    let den  = f6_scal(&y, two);
    let s    = f6_div(&num, &den);

    let s2 = f6_square(&s);
    let x3 = f6_sub(&f6_sub(&s2, &x), &x);
    let y3 = f6_sub(&f6_mul(&s, &f6_sub(&x, &x3)), &y);

    CheetahPoint { x: x3, y: y3, inf: false }
}
#[inline] fn ch_double(p: &CheetahPoint) -> CheetahPoint { ch_double_unsafe(p) }

fn ch_scal_big(k_be: &[u8; 32], p: &CheetahPoint) -> CheetahPoint {
    let mut acc = A_ID;
    let mut base = *p;
    // LSB-first bit walk
    for bit in 0..256 {
        let byte = k_be[31 - (bit / 8)];
        if ((byte >> (bit % 8)) & 1) == 1 {
            acc = ch_add(&acc, &base);
        }
        base = ch_double(&base);
    }
    acc
}

// ---- Basepoint --------------------------------------------------------------

#[inline]
fn basepoint() -> CheetahPoint {
    // cheap in prod; thorough in debug
    debug_assert!(is_on_curve(&G), "G not on curve");
    #[cfg(debug_assertions)]
    {
        let nG = ch_scal_big(&CHEETAH_N, &G);
        debug_assert!(nG.inf, "n·G != ∞");
        let a = F6lt([Belt(5), Belt(123), Belt(0), Belt(77), Belt(2), Belt(9999)]);
        let ainv = f6_inv(&a);
        debug_assert_eq!(f6_mul(&a, &ainv), F6_ONE, "f6_inv is wrong");
    }
    G
}

// ---- TIP-5 helpers ----------------------------------------------------------

const RATE: usize = 8;            // 8 x u64 words absorbed per block
const DIGEST_LENGTH: usize = 5;   // 5 x u64 output

fn tip5_hash_words(words: &[u64]) -> [u64; DIGEST_LENGTH] {
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
    // MSW..LSW per limb order, X then Y
    let mut out = [0u64; 12];
    out[..6].copy_from_slice(&pt.0);
    out[6..].copy_from_slice(&pt.1);
    out
}

// ---- Big-endian scalar helpers (mod n) -------------------------------------

type HmacSha512 = Hmac<Sha512>;

#[inline] fn is_zero32(x: &[u8; 32]) -> bool { x.iter().fold(0u8, |z, &b| z | b) == 0 }

#[inline]
fn be32_lt(a: &[u8; 32], b: &[u8; 32]) -> bool {
    for k in 0..32 { if a[k] != b[k] { return a[k] < b[k]; } }
    false
}

#[inline]
fn sub_be32(a: &[u8; 32], b: &[u8; 32]) -> ([u8; 32], u8) {
    // (a-b, borrow)
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
    // (a+b, carry)
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
fn be32_sub_inplace(a: &mut [u8; 32], b: &[u8; 32]) {
    let mut brw: i16 = 0;
    for k in (0..32).rev() {
        let v = a[k] as i16 - b[k] as i16 - brw;
        if v < 0 { a[k] = (v + 256) as u8; brw = 1; } else { a[k] = v as u8; brw = 0; }
    }
}

#[inline]
fn add_mod_n(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (mut sum, carry) = add_be32(a, b);
    if carry == 1 || !be32_lt(&sum, &CHEETAH_N) {
        be32_sub_inplace(&mut sum, &CHEETAH_N);
    }
    sum
}

#[inline]
fn mul_mod_n(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    // schoolbook 256x256 -> 512 then mod n
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
        let mut kk = 63 - (i + 32);
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
        // rem <<= 8; add b
        let mut carry = b as u16;
        for i in (0..32).rev() {
            let t = ((rem[i] as u16) << 8) | carry;
            rem[i] = (t & 0xff) as u8;
            carry = (t >> 8) as u16;
        }
        // reduce while rem >= n (usually at most once)
        while !be32_lt(&rem, &CHEETAH_N) {
            be32_sub_inplace(&mut rem, &CHEETAH_N);
        }
    }
    rem
}

// ---- RFC6979-ish deterministic k (HMAC-SHA256) -----------------------------

fn rfc6979_k(sk_be: &[u8; 32], personalization: &[u8]) -> [u8; 32] {
    // k = HMAC-SHA256(sk, personalization) mod n, reject 0
    let mut mac = Hmac::<Sha256>::new_from_slice(sk_be).expect("hmac key");
    mac.update(personalization);
    let digest = mac.finalize().into_bytes(); // 32
    let mut k = [0u8; 32];
    k.copy_from_slice(&digest);
    let k = mod_n_from_be_bytes(&k);
    if is_zero32(&k) { [1u8; 32] } else { k }
}

// ---- HMAC split (for SLIP-10) ----------------------------------------------

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

// ---- Master-from-seed (rehash-until-valid) ---------------------------------

pub fn master_from_seed(seed: &[u8]) -> ([u8; 32], [u8; 32]) {
    let mut mac = HmacSha512::new_from_slice(NOCKCHAIN_SLIP10_KEY).unwrap();
    mac.update(seed);
    let mut i = mac.finalize().into_bytes(); // 64 bytes

    loop {
        let mut left  = [0u8; 32];
        let mut right = [0u8; 32];
        left.copy_from_slice(&i[..32]);
        right.copy_from_slice(&i[32..]);

        if !is_zero32(&left) && be32_lt(&left, &CHEETAH_N) {
            return (left, right);
        }

        // rehash whole 64B per spec
        let mut mac = HmacSha512::new_from_slice(NOCKCHAIN_SLIP10_KEY).unwrap();
        mac.update(&i);
        i = mac.finalize().into_bytes();
    }
}

// ---- Serialization ----------------------------------------------------------

/// Serialize affine point limbs (x,y) into 97 bytes: 0x01 || Y(MSW..LSW) || X(MSW..LSW)
pub fn ser_a_pt(pk: &([u64; 6], [u64; 6])) -> [u8; 97] {
    let (x, y) = pk;
    let mut out = [0u8; 97];
    out[0] = 0x01;
    let mut off = 1;

    for &w in y.iter() { out[off..off+8].copy_from_slice(&w.to_be_bytes()); off += 8; }
    for &w in x.iter() { out[off..off+8].copy_from_slice(&w.to_be_bytes()); off += 8; }
    out
}

/// Serialize as 8-byte sentinel(=1) || X(MSW..LSW) || Y(MSW..LSW)  (104 bytes)
pub fn ser_a_pt_rep104(pk: &([u64; 6], [u64; 6])) -> [u8; 104] {
    let (x, y) = pk;
    let mut out = [0u8; 104];
    out[0..8].copy_from_slice(&1u64.to_be_bytes()); // 8-byte sentinel
    let mut off = 8;
    for &w in x.iter() { out[off..off+8].copy_from_slice(&w.to_be_bytes()); off += 8; }
    for &w in y.iter() { out[off..off+8].copy_from_slice(&w.to_be_bytes()); off += 8; }
    out
}

// ---- Keys & Schnorr ---------------------------------------------------------

/// Compute affine (x,y) for the secret scalar `sk_be` (big-endian).
/// Internal limbs are LSW..MSW; wire/serialized limbs must be MSW..LSW.
pub fn cheetah_pub_from_sk(sk_be: [u8; 32]) -> ([u64; 6], [u64; 6]) {
    let p = ch_scal_big(&sk_be, &basepoint());
    let mut x = [0u64; 6];
    let mut y = [0u64; 6];

    // internal limbs are LSW..MSW, wire format must be MSW..LSW
    for i in 0..6 {
        x[5 - i] = p.x.0[i].0 as u64;
        y[5 - i] = p.y.0[i].0 as u64;
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

// ---- SLIP-10 child derivation (xprv/xpub) ----------------------------------

#[derive(Clone, Debug)]
pub struct XKey {
    pub depth: u8,
    pub index: u32,
    pub chain_code: [u8; 32],
    pub sk: Option<[u8; 32]>,              // None for xpub
    pub pk: Option<([u64; 6], [u64; 6])>,  // present for xpub or when sk present
    pub parent_fingerprint: [u8; 4],
}

#[inline] fn is_hardened(i: u32) -> bool { i & 0x8000_0000 != 0 }
#[inline] fn ser32_be(i: u32) -> [u8; 4] { i.to_be_bytes() }

fn fingerprint_from_pk(pk: &([u64; 6], [u64; 6])) -> [u8; 4] {
    // TIP-5 over P, take first 4 bytes of digest[0]
    let packed = pack_point_words(pk);
    let digest = tip5_hash_words(&packed);
    digest[0].to_be_bytes()[..4].try_into().unwrap()
}

pub fn xprv_derive_child(parent: &XKey, i: u32) -> XKey {
    let prv = parent.sk.expect("need private key");
    let cc  = parent.chain_code;

    const APT_SER_LEN: usize = 97;

    let (mut left, mut right) = if is_hardened(i) {
        // data = 0x00 || ser256(prv) || ser32(i)
        let mut data = [0u8; 1 + 32 + 4];
        data[0] = 0x00;
        data[1..33].copy_from_slice(&prv);
        data[33..].copy_from_slice(&ser32_be(i));
        hmac_split_512(&cc, &data)
    } else {
        // data = ser_a_pt(P) || ser32(i)
        let pk_xy   = parent.pk.unwrap_or_else(|| cheetah_pub_from_sk(prv));
        let pub_ser = ser_a_pt(&pk_xy); // 97 bytes
        let mut data = [0u8; APT_SER_LEN + 4];
        data[..APT_SER_LEN].copy_from_slice(&pub_ser);
        data[APT_SER_LEN..].copy_from_slice(&ser32_be(i));
        hmac_split_512(&cc, &data)
    };

    // Retry until 0 < left < n and child != 0 using 0x01 || right || i
    for _ in 0..1024 {
        if !is_zero32(&left) && be32_lt(&left, &CHEETAH_N) {
            let child_sk = be32_add_mod_n(&left, &prv);
            if !is_zero32(&child_sk) {
                let child_pk = cheetah_pub_from_sk(child_sk);
                return XKey {
                    sk: Some(child_sk),
                    pk: Some(child_pk),
                    chain_code: right,
                    depth: parent.depth + 1,
                    index: i,
                    parent_fingerprint: parent.parent_fingerprint,
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

pub fn xpub_derive_child(parent: &XKey, i: u32) -> XKey {
    assert_eq!(i & 0x8000_0000, 0);

    let mut data = Vec::with_capacity(104 + 4);
    data.extend_from_slice(&ser_a_pt_rep104(parent.pk.as_ref().unwrap()));
    data.extend_from_slice(&ser32_be(i));

    let (mut left, mut right) = hmac_split_512(&parent.chain_code, &data);

    loop {
        if !is_zero32(&left) && be32_lt(&left, &CHEETAH_N) {
            // child P = (left * G) + parent P
            let q  = ch_scal_big(&left, &basepoint());
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

// ---- 32-byte arithmetic helpers (mod n) ------------------------------------

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
fn be32_add_mod_n(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (mut sum, carry) = be32_add(a, b);
    if carry == 1 || !be32_lt(&sum, &CHEETAH_N) {
        be32_sub_inplace(&mut sum, &CHEETAH_N);
    }
    sum
}