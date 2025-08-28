// nallux commentary: I don't know what ubig is and how it works, so I got gpt to spit this out for me.

/* ===== Internal 320-bit integer (no bigints) ===== */

use anyhow::{anyhow, Result};

const B58: &str =
    "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const P: u128 = (1u128 << 64) - (1u128 << 32) + 1; // 2^64 - 2^32 + 1
const MASK64: u128 = 0xffff_ffff_ffff_ffff;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct U320 {
    // Big-endian limbs, limbs[0] = most significant 64 bits
    limbs: [u64; 5],
}

impl U320 {
    fn zero() -> Self { Self { limbs: [0; 5] } }
    pub fn from_u64(x: u64) -> Self { Self { limbs: [0, 0, 0, 0, x] } }

    fn is_zero(&self) -> bool {
        self.limbs.iter().all(|&w| w == 0)
    }

    pub fn as_single_u64(&self) -> Result<u64> {
        if self.limbs[..4].iter().any(|&w| w != 0) {
            return Err(anyhow!("value does not fit in u64"));
        }
        Ok(self.limbs[4])
    }

    /* --- Base58 decode: N = N*58 + digit --- */
    pub fn from_base58(s: &str) -> Result<Self> {
        let mut n = U320::zero();
        for &b in s.as_bytes() {
            let d = B58
                .as_bytes()
                .iter()
                .position(|&c| c == b)
                .ok_or_else(|| anyhow!("invalid base58 char: {}", b as char))? as u64;
            n.mul_small_add(58, d);
        }
        Ok(n)
    }

    /* --- Base58 encode via repeated div by 58 --- */
    pub fn to_base58(&self) -> String {
        if self.is_zero() {
            return "1".to_string();
        }
        let mut tmp = *self;
        let mut out = Vec::new();
        while !tmp.is_zero() {
            let (q, r) = tmp.divrem_small(58);
            out.push(B58.as_bytes()[r as usize] as char);
            tmp = q;
        }
        out.iter().rev().collect()
    }

    /* --- core small ops --- */

    // self = self * m + add    (m <= 58)
    fn mul_small_add(&mut self, m: u64, add: u64) {
        let mut carry: u128 = add as u128;
        for i in (0..5).rev() {
            let t = (self.limbs[i] as u128) * (m as u128) + carry;
            self.limbs[i] = (t & MASK64) as u64;
            carry = t >> 64;
        }
        assert!(carry == 0, "overflow while parsing base58");
    }

    // (self / d, self % d)  where d <= 58
    fn divrem_small(&self, d: u64) -> (Self, u64) {
        let mut q = [0u64; 5];
        let mut rem: u128 = 0;
        for i in 0..5 {
            let cur = (rem << 64) | self.limbs[i] as u128;
            let qi = cur / d as u128;
            let ri = cur - qi * d as u128;
            q[i] = qi as u64;
            rem = ri;
        }
        (Self { limbs: q }, rem as u64)
    }

    // (self / p, self % p)
    pub fn divrem_p(&self) -> (Self, u64) {
        let mut q = [0u64; 5];
        let mut rem: u128 = 0;
        for i in 0..5 {
            let cur = (rem << 64) | self.limbs[i] as u128;
            let qi = cur / P;
            let ri = cur - qi * P;
            q[i] = qi as u64; // qi < 2^64
            rem = ri;         // < p
        }
        (Self { limbs: q }, rem as u64)
    }

    // self = self * p + add,  with p = 2^64 - 2^32 + 1
    // Implemented as: (self<<64) - (self<<32) + self + add
    pub fn mul_p_add_u64(&mut self, add: u64) {
        let mut acc = self.shl_64();
        let s32 = self.shl_32();
        acc.sub_assign(&s32);
        acc.add_assign(self);
        acc.add_small(add);
        *self = acc;
    }

    /* --- minimal helpers for the above --- */

    fn shl_64(&self) -> Self {
        let mut out = [0u64; 5];
        // left shift by one limb
        out[0] = self.limbs[1];
        out[1] = self.limbs[2];
        out[2] = self.limbs[3];
        out[3] = self.limbs[4];
        out[4] = 0;
        Self { limbs: out }
    }

    fn shl_32(&self) -> Self {
        let mut out = [0u64; 5];
        let mut prev_lo: u64 = 0;
        for i in (0..5).rev() {
            let v = self.limbs[i];
            out[i] = (v << 32) | (prev_lo >> 32);
            prev_lo = v;
        }
        Self { limbs: out }
    }

    fn add_small(&mut self, add: u64) {
        let mut carry = add as u128;
        for i in (0..5).rev() {
            let t = self.limbs[i] as u128 + carry;
            self.limbs[i] = (t & MASK64) as u64;
            carry = t >> 64;
        }
        assert!(carry == 0, "overflow in add_small");
    }

    fn add_assign(&mut self, other: &Self) {
        let mut carry: u128 = 0;
        for i in (0..5).rev() {
            let t = self.limbs[i] as u128 + other.limbs[i] as u128 + carry;
            self.limbs[i] = (t & MASK64) as u64;
            carry = t >> 64;
        }
        assert!(carry == 0, "overflow in add_assign");
    }

    fn sub_assign(&mut self, other: &Self) {
        let mut borrow: i128 = 0;
        for i in (0..5).rev() {
            let a = self.limbs[i] as i128;
            let b = other.limbs[i] as i128 + borrow;
            let diff = a - b;
            if diff < 0 {
                self.limbs[i] = (diff + (1i128 << 64)) as u64;
                borrow = 1;
            } else {
                self.limbs[i] = diff as u64;
                borrow = 0;
            }
        }
        assert!(borrow == 0, "underflow in sub_assign");
    }
}