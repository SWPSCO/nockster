//! Pure-Rust jam signer + SLIP-10 derivation for Cheetah (no nockvm).
//! - Hoon parity for from-seed / child derivation (hardened & normal)
//! - RFC6979-HMAC-SHA256 deterministic nonce
//! - TIP5(e) challenge: TIP5(R || P || txid)
//! - Limb conventions match first-party (little-endian by limb)

use core::cmp::min;
use hmac::{Hmac, Mac};
use ibig::UBig;
use num_traits::Zero;
use sha2::{Sha256, Sha512};
use noun_serde::NounEncode;
use nockapp::noun::slab::NounSlab;
use nockapp::Bytes;

use tx_types::collections::ZMap;
use tx_types::transaction_types::*;
use zkvm_jetpack::form::math::base::bneg;
use zkvm_jetpack::form::math::bpoly::{bpegcd, bpscal};
use zkvm_jetpack::form::math::badd;
use zkvm_jetpack::form::math::tip5::permute as tip5_permute;
use zkvm_jetpack::form::poly::Belt;

// ===========================================================================
// Public API
// ===========================================================================

pub enum SecretSource<'a> {
    Seed(&'a [u8]),            // SLIP-10 master from seed
    PrivateKey([u8; 32]),      // raw 32-byte big-endian scalar
}

/// Sign a Transaction in-memory, return signed jam bytes.
pub fn sign_transaction(mut tx: Transaction, secret: SecretSource) -> Result<Bytes, String> {
    let tx_id: Hash = tx_types::tx_to_noun::generate_tx_id(tx.p.p.clone());

    // Master secret
    let (sk_be32, _cc) = match secret {
        SecretSource::PrivateKey(sk) => (sk, [0u8; 32]),
        SecretSource::Seed(seed) => master_from_seed(seed),
    };

    // Public key P = sk*G
    let pk_xy = cheetah_pub_from_sk(sk_be32);
    let my_pk = SchnorrPubkey {
        x: F6LT { values: pk_xy[0] },
        y: F6LT { values: pk_xy[1] },
        inf: false,
    };

    // (e, s)
    let (chal_t8, sig_t8) = schnorr_sign_txid(sk_be32, pk_xy, tx_id);

    // Attach our signature only to inputs whose signer-set contains our pk
    let mut new_inputs = ZMap::new();
    for (k, mut input) in tx.p.p.tap() {
        let lock = &input.note.lock;
        if !lock.pubkeys.has(&my_pk) {
            new_inputs.put(k, input);
            continue;
        }
        let entry = SchnorrSignature {
            chal: Chal { values: chal_t8.clone() },
            sig:  Sig  { values: sig_t8.clone()  },
        };
        match &mut input.spend.signature {
            Some(sigmap) => sigmap.map.put(my_pk.clone(), entry),
            None => {
                let mut map = ZMap::new();
                map.put(my_pk.clone(), entry);
                input.spend.signature = Some(Signature { map });
            }
        }
        new_inputs.put(k, input);
    }
    tx.p.p = new_inputs;

    // Jam
    let mut out_slab: NounSlab = NounSlab::new();
    let noun = tx.to_noun(&mut out_slab);
    out_slab.copy_into(noun);
    Ok(out_slab.jam())
}

// Convenience wrapper (kept for compatibility)
pub fn sign_unsigned_jam_using_tx(tx: Transaction, secret: SecretSource) -> Result<Bytes, String> {
    sign_transaction(tx, secret)
}

// ===========================================================================
// SLIP-10 master + child derivation (Hoon parity)
// ===========================================================================

type HmacSha512 = Hmac<Sha512>;
const NOCKCHAIN_SLIP10_KEY: &[u8] = b"Nockchain seed"; // 'dees niahckcoN' in Hoon text

/// Master from seed (Hoon `from-seed`):
/// I = HMAC-SHA512(key="Nockchain seed", data=seed)
/// left = sk, right = chain code; if left==0 or left>=n, **rehash the entire I**.
fn master_from_seed(seed: &[u8]) -> ([u8; 32], [u8; 32]) {
    let n = cheetah_order();

    // First derivation
    let mut mac = HmacSha512::new_from_slice(NOCKCHAIN_SLIP10_KEY).unwrap();
    mac.update(seed);
    let mut i = mac.finalize().into_bytes().to_vec();

    loop {
        let mut left = [0u8; 32];
        let mut right = [0u8; 32];
        left.copy_from_slice(&i[..32]);
        right.copy_from_slice(&i[32..]);

        let sk = UBig::from_be_bytes(&left);
        if !sk.is_zero() && sk < n {
            return (left, right);
        }

        // retry by rehashing the full 64-byte digest
        let mut mac = HmacSha512::new_from_slice(NOCKCHAIN_SLIP10_KEY).unwrap();
        mac.update(&i);
        i = mac.finalize().into_bytes().to_vec();
    }
}

/// Extended key minimal representation for derivation.
#[derive(Clone)]
pub struct XKey {
    pub sk: Option<[u8; 32]>,      // None for public-only nodes
    pub pk_xy: [[u64; 6]; 2],      // affine point
    pub cc: [u8; 32],              // chain code
    pub depth: u8,
    pub index: u32,                // raw, with hardening bit possibly set
    pub parent_fp: [u8; 4],        // fingerprint (optional for now)
}

pub fn xkey_from_seed(seed: &[u8]) -> XKey {
    let (sk, cc) = master_from_seed(seed);
    let pk_xy = cheetah_pub_from_sk(sk);
    XKey { sk: Some(sk), pk_xy, cc, depth: 0, index: 0, parent_fp: [0u8; 4] }
}

#[inline]
fn is_hardened(i: u32) -> bool { i >= (1 << 31) }

#[inline]
fn ser32_be(i: u32) -> [u8; 4] { i.to_be_bytes() }

#[inline]
fn ser256_be(sk: &[u8; 32]) -> [u8; 32] { *sk }

/// Serialize affine point to 97 bytes (hoon `ser-a-pt`).
/// 6×u64 limbs for x (48 bytes) || 6×u64 limbs for y (48 bytes) || 1 byte inf.
/// Limbs are written **little-endian** per limb by default (flip const below if needed).
const SER_LIMBS_BIG_ENDIAN: bool = false;
fn ser_a_pt(pk_xy: &[[u64; 6]; 2]) -> [u8; 97] {
    let mut out = [0u8; 97];
    let mut off = 0;
    for limb in pk_xy[0].iter().chain(pk_xy[1].iter()) {
        let b = if SER_LIMBS_BIG_ENDIAN {
            limb.to_be_bytes()
        } else {
            limb.to_le_bytes()
        };
        out[off..off + 8].copy_from_slice(&b);
        off += 8;
    }
    out[96] = 0; // inf = false for affine points
    out
}

/// hoon `derive-private`
/// - If `i` is hardened (i>=2^31): data = 0x00 || ser256(prv) || ser32(i)
/// - Else: data = serP(pub) || ser32(i)
/// - I = HMAC-SHA512(key=cc, data)
/// - left,right = split(I), key' = (left + prv) mod n
/// - invalid if left>=n or key'==0 → rehash with data' = 0x01 || right || ser32(i)
pub fn xprv_derive_child(parent: &XKey, i: u32) -> XKey {
    let n = cheetah_order();

    let (prv, cc) = (parent.sk.expect("need private key"), parent.cc);
    let prv_int = UBig::from_be_bytes(&prv); // <-- add this

    let pub_ser = ser_a_pt(&parent.pk_xy);

    // initial data ...
    let mut data = Vec::with_capacity(1 + 32 + 4 + 97);
    if is_hardened(i) {
        data.push(0u8);
        data.extend_from_slice(&ser256_be(&prv));
        data.extend_from_slice(&ser32_be(i));
    } else {
        data.extend_from_slice(&pub_ser);
        data.extend_from_slice(&ser32_be(i));
    }

    // run HMAC and (if needed) the invalid-key retry path
    let (left, right) = hmac_split_512(&cc, &data);
    let mut left_int = UBig::from_be_bytes(&left);
    let mut child_sk_int = (&left_int + &prv_int) % &n; // <-- borrow, don’t move

    if !(left_int < n) || child_sk_int.is_zero() {
        // retry with 0x01 || right || ser32(i)
        let mut red = Vec::with_capacity(1 + 32 + 4);
        red.push(0x01);
        red.extend_from_slice(&right);
        red.extend_from_slice(&ser32_be(i));

        let (left2, right2) = hmac_split_512(&cc, &red);
        left_int = UBig::from_be_bytes(&left2);
        child_sk_int = (&left_int + &prv_int) % &n; // <-- borrow again

        assert!(!child_sk_int.is_zero() && left_int < n, "invalid after retry");
        return xkey_from_child_int(child_sk_int, right2, parent, i);
    }

    xkey_from_child_int(child_sk_int, right, parent, i)
}

/// hoon `derive-public` (only -non-hardened)
/// - data = serP(pub) || ser32(i)
/// - I = HMAC-SHA512(key=cc, data), left,right = split(I)
/// - pub' = (left*G) + pub
/// - invalid if left>=n or pub' == identity → retry with 0x01 || right || ser32(i)
pub fn xpub_derive_child(parent: &XKey, i: u32) -> XKey {
    assert!(!is_hardened(i), "cannot derive hardened from public key");
    let n = cheetah_order();

    let cc = parent.cc;
    let pub_ser = ser_a_pt(&parent.pk_xy);

    let mut data = Vec::with_capacity(97 + 4);
    data.extend_from_slice(&pub_ser);
    data.extend_from_slice(&ser32_be(i));

    let (left, right) = hmac_split_512(&cc, &data);
    let mut left_int = UBig::from_be_bytes(&left);

    // pub' = left*G + pub
    let mut child_pk = add_scalar_times_g_to_point(&left_int, &parent.pk_xy);

    if !(left_int < n) || is_identity(&child_pk) {
        // retry with 0x01 || right || ser32(i)
        let mut red = Vec::with_capacity(1 + 32 + 4);
        red.push(0x01);
        red.extend_from_slice(&right);
        red.extend_from_slice(&ser32_be(i));
        let (left2, right2) = hmac_split_512(&cc, &red);
        left_int = UBig::from_be_bytes(&left2);
        child_pk = add_scalar_times_g_to_point(&left_int, &parent.pk_xy);
        assert!(left_int < n && !is_identity(&child_pk), "invalid after retry");
        return XKey {
            sk: None,
            pk_xy: child_pk,
            cc: right2,
            depth: parent.depth + 1,
            index: i,
            parent_fp: [0u8; 4],
        };
    }

    XKey {
        sk: None,
        pk_xy: child_pk,
        cc: right,
        depth: parent.depth + 1,
        index: i,
        parent_fp: [0u8; 4],
    }
}

fn xkey_from_child_int(child_sk: UBig, cc: [u8; 32], parent: &XKey, i: u32) -> XKey {
    let mut be = child_sk.to_be_bytes();
    if be.len() < 32 {
        let mut pad = vec![0u8; 32 - be.len()];
        pad.extend_from_slice(&be);
        be = pad;
    }
    let mut sk = [0u8; 32];
    sk.copy_from_slice(&be[be.len() - 32..]);
    let pk_xy = cheetah_pub_from_sk(sk);
    XKey {
        sk: Some(sk),
        pk_xy,
        cc,
        depth: parent.depth + 1,
        index: i,
        parent_fp: [0u8; 4],
    }
}

#[inline]
fn hmac_split_512(key: &[u8; 32], data: &[u8]) -> ([u8; 32], [u8; 32]) {
    let mut mac = HmacSha512::new_from_slice(key).unwrap();
    mac.update(data);
    let i = mac.finalize().into_bytes();
    let mut left = [0u8; 32];
    let mut right = [0u8; 32];
    left.copy_from_slice(&i[..32]);
    right.copy_from_slice(&i[32..]);
    (left, right)
}

// ===========================================================================
// TIP5
// ===========================================================================

const DIGEST_LENGTH: usize = 5;
const STATE_SIZE: usize = 16;
const RATE: usize = 10;

fn tip5_hash_words(words: &[u64]) -> [u64; DIGEST_LENGTH] {
    let mut st = [0u64; STATE_SIZE];

    // absorb
    let mut i = 0;
    while i < words.len() {
        let take = min(RATE, words.len() - i);
        for j in 0..take {
            st[j] = badd(st[j], words[i + j]);
        }
        i += take;
        tip5_permute(&mut st);
    }

    // domain sep
    st[words.len() % RATE] = badd(st[words.len() % RATE], 1);
    st[STATE_SIZE - 1] = badd(st[STATE_SIZE - 1], 1 << 63);
    tip5_permute(&mut st);

    let mut out = [0u64; DIGEST_LENGTH];
    out.copy_from_slice(&st[..DIGEST_LENGTH]);
    out
}

fn pack_point_words(pt: &[[u64; 6]; 2]) -> [u64; 12] {
    let mut out = [0u64; 12];
    out[..6].copy_from_slice(&pt[0]);
    out[6..].copy_from_slice(&pt[1]);
    out
}

// ===========================================================================
// Schnorr over Cheetah (RFC6979 nonce + TIP5 challenge)
// ===========================================================================

/// RFC6979-HMAC-SHA256 deterministic nonce.
fn rfc6979_k(sk_be32: &[u8; 32], msg: &[u8], n: &UBig) -> UBig {
    // Section 3.2 of RFC6979 (qlen≈255, we use 32-byte HMAC)
    let mut V = [0x01u8; 32];
    let mut K = [0x00u8; 32];

    // K = HMAC(K, V || 0x00 || x || m)
    let mut h = Hmac::<Sha256>::new_from_slice(&K).unwrap();
    h.update(&V);
    h.update(&[0x00]);
    h.update(sk_be32);
    h.update(msg);
    K.copy_from_slice(&h.finalize().into_bytes());

    // V = HMAC(K, V)
    let mut h = Hmac::<Sha256>::new_from_slice(&K).unwrap();
    h.update(&V);
    V.copy_from_slice(&h.finalize().into_bytes());

    // K = HMAC(K, V || 0x01 || x || m)
    let mut h = Hmac::<Sha256>::new_from_slice(&K).unwrap();
    h.update(&V);
    h.update(&[0x01]);
    h.update(sk_be32);
    h.update(msg);
    K.copy_from_slice(&h.finalize().into_bytes());

    // V = HMAC(K, V)
    let mut h = Hmac::<Sha256>::new_from_slice(&K).unwrap();
    h.update(&V);
    V.copy_from_slice(&h.finalize().into_bytes());

    loop {
        // T = HMAC(K, V)
        let mut h = Hmac::<Sha256>::new_from_slice(&K).unwrap();
        h.update(&V);
        let T = h.finalize().into_bytes(); // 32 bytes

        let mut k = UBig::from_be_bytes(&T) % n;
        if !k.is_zero() {
            return k;
        }

        // K = HMAC(K, V || 0x00); V = HMAC(K, V)
        let mut h = Hmac::<Sha256>::new_from_slice(&K).unwrap();
        h.update(&V);
        h.update(&[0x00]);
        K.copy_from_slice(&h.finalize().into_bytes());

        let mut h = Hmac::<Sha256>::new_from_slice(&K).unwrap();
        h.update(&V);
        V.copy_from_slice(&h.finalize().into_bytes());
    }
}

fn ubig_to_t8(v: &UBig) -> T8 {
    // 8 limbs, limb[0] = least-significant 64 bits (LE by limb)
    let mut be = v.to_be_bytes();
    if be.len() < 64 {
        let mut pad = vec![0u8; 64 - be.len()];
        pad.extend_from_slice(&be);
        be = pad;
    } else if be.len() > 64 {
        be = be[be.len() - 64..].to_vec();
    }
    let mut limbs = [0u64; 8];
    for i in 0..8 {
        let start = 64 - (i + 1) * 8;
        limbs[i] = u64::from_be_bytes(be[start..start + 8].try_into().unwrap());
    }
    T8 { values: limbs }
}

fn schnorr_sign_txid(sk_be32: [u8; 32], pk: [[u64; 6]; 2], txid: Hash) -> (T8, T8) {
    let n = cheetah_order();

    // RFC6979 nonce over txid bytes (40 bytes)
    let mut msg = Vec::with_capacity(5 * 8);
    for w in txid.values { msg.extend_from_slice(&w.to_be_bytes()); }
    let k = rfc6979_k(&sk_be32, &msg, &n);

    // R = k*G
    let mut kb = k.to_be_bytes();
    if kb.len() < 32 {
        let mut pad = vec![0u8; 32 - kb.len()];
        pad.extend_from_slice(&kb);
        kb = pad;
    }
    let mut k32 = [0u8; 32];
    k32.copy_from_slice(&kb[kb.len() - 32..]);
    let r_pt = cheetah_pub_from_sk(k32);

    // e = TIP5( R || P || txid )
    let mut words = Vec::<u64>::with_capacity(12 + 12 + 5);
    words.extend_from_slice(&pack_point_words(&r_pt));
    words.extend_from_slice(&pack_point_words(&pk));
    words.extend_from_slice(&txid.values);
    let e_words = tip5_hash_words(&words);

    let mut e_be = [0u8; 40];
    for (i, w) in e_words.iter().enumerate() {
        e_be[i * 8..(i + 1) * 8].copy_from_slice(&w.to_be_bytes());
    }
    let e = UBig::from_be_bytes(&e_be) % &n;

    // s = k + e*x mod n
    let x = UBig::from_be_bytes(&sk_be32);
    let s = (k + e.clone() * x) % &n;

    (ubig_to_t8(&e), ubig_to_t8(&s))
}

// ===========================================================================
// Cheetah over F^6 (pure)
// ===========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct F6lt(pub [Belt; 6]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CheetahPoint { x: F6lt, y: F6lt, inf: bool }

const F6_ZERO: F6lt = F6lt([Belt(0); 6]);
const F6_ONE:  F6lt = F6lt([Belt(1), Belt(0), Belt(0), Belt(0), Belt(0), Belt(0)]);
const A_ID: CheetahPoint = CheetahPoint { x: F6_ZERO, y: F6_ONE, inf: true };

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

fn g() -> CheetahPoint { CheetahPoint { x: GX, y: GY, inf: false } }

const GROUP_ORDER_HEX: &str =
    "7af2599b3b3f22d0563fbf0f990a37b5327aa72330157722d443623eaed4accf";
fn cheetah_order() -> UBig {
    UBig::from_str_radix(GROUP_ORDER_HEX, 16).expect("valid group order")
}

fn f6_add(a: &F6lt, b: &F6lt) -> F6lt {
    F6lt([
        a.0[0]+b.0[0], a.0[1]+b.0[1], a.0[2]+b.0[2],
        a.0[3]+b.0[3], a.0[4]+b.0[4], a.0[5]+b.0[5],
    ])
}
fn f6_neg(f: &F6lt) -> F6lt { F6lt([-f.0[0], -f.0[1], -f.0[2], -f.0[3], -f.0[4], -f.0[5]]) }
fn f6_sub(a: &F6lt, b: &F6lt) -> F6lt { f6_add(a, &f6_neg(b)) }
fn f6_scal(s: Belt, f: &F6lt) -> F6lt {
    F6lt([f.0[0]*s, f.0[1]*s, f.0[2]*s, f.0[3]*s, f.0[4]*s, f.0[5]*s])
}
fn karat3(a: &[Belt;3], b: &[Belt;3]) -> [Belt;5] {
    let m = [a[0]*b[0], a[1]*b[1], a[2]*b[2]];
    [
        m[0],
        (a[0]+a[1])*(b[0]+b[1]) - (m[0]+m[1]),
        (a[0]+a[2])*(b[0]+b[2]) - (m[0]+m[2]) + m[1],
        (a[1]+a[2])*(b[1]+b[2]) - (m[1]+m[2]),
        m[2],
    ]
}
fn f6_mul(f: &F6lt, g: &F6lt) -> F6lt {
    let f0g0 = karat3(&[f.0[0], f.0[1], f.0[2]], &[g.0[0], g.0[1], g.0[2]]);
    let f1g1 = karat3(&[f.0[3], f.0[4], f.0[5]], &[g.0[3], g.0[4], g.0[5]]);
    let foil  = karat3(
        &[f.0[0]+f.0[3], f.0[1]+f.0[4], f.0[2]+f.0[5]],
        &[g.0[0]+g.0[3], g.0[1]+g.0[4], g.0[2]+g.0[5]],
    );
    let cross = [
        foil[0] - (f0g0[0]+f1g1[0]),
        foil[1] - (f0g0[1]+f1g1[1]),
        foil[2] - (f0g0[2]+f1g1[2]),
        foil[3] - (f0g0[3]+f1g1[3]),
        foil[4] - (f0g0[4]+f1g1[4]),
    ];
    F6lt([
        f0g0[0] + Belt(7)*(cross[3] + f1g1[0]),
        f0g0[1] + Belt(7)*(cross[4] + f1g1[1]),
        f0g0[2] + Belt(7)* f1g1[2],
        f0g0[3] + cross[0] + Belt(7)*f1g1[3],
        f0g0[4] + cross[1] + Belt(7)*f1g1[4],
        cross[2],
    ])
}
fn f6_square(f: &F6lt) -> F6lt { f6_mul(f, f) }
fn f6_inv(f: &F6lt) -> F6lt {
    let mut res = [Belt(0); 6];
    let mut d = [Belt(0); 7];
    let mut u = [Belt(0); 7];
    let mut v = [Belt(0); 6];
    bpegcd(
        &f.0,
        &[Belt(bneg(7)), Belt(0), Belt(0), Belt(0), Belt(0), Belt(0), Belt(1)],
        &mut d, &mut u, &mut v,
    );
    let inv = d[0].inv();
    bpscal(inv, &u, &mut res);
    F6lt(res)
}
fn f6_div(a: &F6lt, b: &F6lt) -> F6lt { f6_mul(a, &f6_inv(b)) }

fn ch_double_unsafe(x: &F6lt, y: &F6lt) -> CheetahPoint {
    let slope = f6_div(
        &f6_add(&f6_scal(Belt(3), &f6_square(x)), &F6_ONE),
        &f6_scal(Belt(2), y),
    );
    let x_out = f6_sub(&f6_square(&slope), &f6_scal(Belt(2), x));
    let y_out = f6_sub(&f6_mul(&slope, &f6_sub(x, &x_out)), y);
    CheetahPoint { x: x_out, y: y_out, inf: false }
}
fn ch_double(p: CheetahPoint) -> CheetahPoint {
    if p.inf || p.y == F6_ZERO { return A_ID; }
    ch_double_unsafe(&p.x, &p.y)
}
fn ch_neg(p: &CheetahPoint) -> CheetahPoint {
    CheetahPoint { x: p.x, y: f6_neg(&p.y), inf: p.inf }
}
fn ch_add_unsafe(p: CheetahPoint, q: CheetahPoint) -> CheetahPoint {
    let slope = f6_div(&f6_sub(&p.y, &q.y), &f6_sub(&p.x, &q.x));
    let x_out = f6_sub(&f6_square(&slope), &f6_add(&p.x, &q.x));
    let y_out = f6_sub(&f6_mul(&slope, &f6_sub(&p.x, &x_out)), &p.y);
    CheetahPoint { x: x_out, y: y_out, inf: false }
}
fn ch_add(p: &CheetahPoint, q: &CheetahPoint) -> CheetahPoint {
    if p.inf { return *q; }
    if q.inf { return *p; }
    if *p == ch_neg(q) { return A_ID; }
    if p == q { return ch_double(*p); }
    ch_add_unsafe(*p, *q)
}

fn ch_scal_big(n: &UBig, p: &CheetahPoint) -> CheetahPoint {
    let mut n = n.clone();
    let mut q = *p;
    let mut acc = A_ID;
    while n > UBig::from(0u8) {
        if n.bit(0) { acc = ch_add(&acc, &q); }
        q = ch_double(q);
        n >>= 1;
    }
    acc
}

fn scalar_mul_g(s: &UBig) -> [[u64; 6]; 2] {
    let p = ch_scal_big(s, &g());
    let mut out = [[0u64; 6]; 2];
    for i in 0..6 {
        out[0][i] = p.x.0[i].0;
        out[1][i] = p.y.0[i].0;
    }
    out
}

fn add_scalar_times_g_to_point(k: &UBig, pk_xy: &[[u64; 6]; 2]) -> [[u64; 6]; 2] {
    let kG = scalar_mul_g(k);
    // point add kG + pk_xy (both affine, not inf)
    let P = CheetahPoint {
        x: F6lt([Belt(pk_xy[0][0]),Belt(pk_xy[0][1]),Belt(pk_xy[0][2]),Belt(pk_xy[0][3]),Belt(pk_xy[0][4]),Belt(pk_xy[0][5])]),
        y: F6lt([Belt(pk_xy[1][0]),Belt(pk_xy[1][1]),Belt(pk_xy[1][2]),Belt(pk_xy[1][3]),Belt(pk_xy[1][4]),Belt(pk_xy[1][5])]),
        inf: false,
    };
    let Q = CheetahPoint {
        x: F6lt([Belt(kG[0][0]),Belt(kG[0][1]),Belt(kG[0][2]),Belt(kG[0][3]),Belt(kG[0][4]),Belt(kG[0][5])]),
        y: F6lt([Belt(kG[1][0]),Belt(kG[1][1]),Belt(kG[1][2]),Belt(kG[1][3]),Belt(kG[1][4]),Belt(kG[1][5])]),
        inf: false,
    };
    let R = ch_add(&P, &Q);
    let mut out = [[0u64; 6]; 2];
    for i in 0..6 {
        out[0][i] = R.x.0[i].0;
        out[1][i] = R.y.0[i].0;
    }
    out
}

fn is_identity(_pk_xy: &[[u64; 6]; 2]) -> bool {
    // we’ll just say it’s never inf here.
    false
}

/// Public key from 32-byte BE secret
fn cheetah_pub_from_sk(sk_be32: [u8; 32]) -> [[u64; 6]; 2] {
    let s = UBig::from_be_bytes(&sk_be32) % cheetah_order();
    scalar_mul_g(&s)
}
