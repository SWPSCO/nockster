
#![allow(clippy::len_without_is_empty)]
use core::slice::Iter;
use alloc::vec::Vec;
use alloc::vec;
pub use crate::noun::belt::{Belt, Felt};
use core::ops::{Add, Mul, Neg, Sub};

pub const PRIME: u64 = 18446744069414584321;
pub const PRIME_128: u128 = 18446744069414584321;
pub const STATE_SIZE: usize = 16;
pub const NUM_SPLIT_AND_LOOKUP: usize = 4;
pub const NUM_ROUNDS: usize = 7;
pub const R: u128 = 18446744073709551616;
pub const P: u64 = 0xffffffff00000001;

const LOOKUP_TABLE: [u8; 256] = [
    0, 7, 26, 63, 124, 215, 85, 254, 214, 228, 45, 185, 140, 173, 33, 240, 29, 177, 176, 32, 8,
    110, 87, 202, 204, 99, 150, 106, 230, 14, 235, 128, 213, 239, 212, 138, 23, 130, 208, 6, 44,
    71, 93, 116, 146, 189, 251, 81, 199, 97, 38, 28, 73, 179, 95, 84, 152, 48, 35, 119, 49, 88,
    242, 3, 148, 169, 72, 120, 62, 161, 166, 83, 175, 191, 137, 19, 100, 129, 112, 55, 221, 102,
    218, 61, 151, 237, 68, 164, 17, 147, 46, 234, 203, 216, 22, 141, 65, 57, 123, 12, 244, 54, 219,
    231, 96, 77, 180, 154, 5, 253, 133, 165, 98, 195, 205, 134, 245, 30, 9, 188, 59, 142, 186, 197,
    181, 144, 92, 31, 224, 163, 111, 74, 58, 69, 113, 196, 67, 246, 225, 10, 121, 50, 60, 157, 90,
    122, 2, 250, 101, 75, 178, 159, 24, 36, 201, 11, 243, 132, 198, 190, 114, 233, 39, 52, 21, 209,
    108, 238, 91, 187, 18, 104, 194, 37, 153, 34, 200, 143, 126, 155, 236, 118, 64, 80, 172, 89,
    94, 193, 135, 183, 86, 107, 252, 13, 167, 206, 136, 220, 207, 103, 171, 160, 76, 182, 227, 217,
    158, 56, 174, 4, 66, 109, 139, 162, 184, 211, 249, 47, 125, 232, 117, 43, 16, 42, 127, 20, 241,
    25, 149, 105, 156, 51, 53, 168, 145, 247, 223, 79, 78, 226, 15, 222, 82, 115, 70, 210, 27, 41,
    1, 170, 40, 131, 192, 229, 248, 255,
];

const ROUND_CONSTANTS: [u64; NUM_ROUNDS * STATE_SIZE] = [
    // 1st round constants
    1332676891236936200, 16607633045354064669, 12746538998793080786, 15240351333789289931,
    10333439796058208418, 986873372968378050, 153505017314310505, 703086547770691416,
    8522628845961587962, 1727254290898686320, 199492491401196126, 2969174933639985366,
    1607536590362293391, 16971515075282501568, 15401316942841283351, 14178982151025681389,
    // 2nd round constants
    2916963588744282587, 5474267501391258599, 5350367839445462659, 7436373192934779388,
    12563531800071493891, 12265318129758141428, 6524649031155262053, 1388069597090660214,
    3049665785814990091, 5225141380721656276, 10399487208361035835, 6576713996114457203,
    12913805829885867278, 10299910245954679423, 12980779960345402499, 593670858850716490,
    // 3rd round constants
    12184128243723146967, 1315341360419235257, 9107195871057030023, 4354141752578294067,
    8824457881527486794, 14811586928506712910, 7768837314956434138, 2807636171572954860,
    9487703495117094125, 13452575580428891895, 14689488045617615844, 16144091782672017853,
    15471922440568867245, 17295382518415944107, 15054306047726632486, 5708955503115886019,
    // 4th round constants
    9596017237020520842, 16520851172964236909, 8513472793890943175, 8503326067026609602,
    9402483918549940854, 8614816312698982446, 7744830563717871780, 14419404818700162041,
    8090742384565069824, 15547662568163517559, 17314710073626307254, 10008393716631058961,
    14480243402290327574, 13569194973291808551, 10573516815088946209, 15120483436559336219,
    // 5th round constants
    3515151310595301563, 1095382462248757907, 5323307938514209350, 14204542692543834582,
    12448773944668684656, 13967843398310696452, 14838288394107326806, 13718313940616442191,
    15032565440414177483, 13769903572116157488, 17074377440395071208, 16931086385239297738,
    8723550055169003617, 590842605971518043, 16642348030861036090, 10708719298241282592,
    // 6th round constants
    12766914315707517909, 11780889552403245587, 113183285481780712, 9019899125655375514,
    3300264967390964820, 12802381622653377935, 891063765000023873, 15939045541699412539,
    3240223189948727743, 4087221142360949772, 10980466041788253952, 18199914337033135244,
    7168108392363190150, 16860278046098150740, 13088202265571714855, 4712275036097525581,
    // 7th round constants
    16338034078141228133, 1455012125527134274, 5024057780895012002, 9289161311673217186,
    9401110072402537104, 11919498251456187748, 4173156070774045271, 15647643457869530627,
    15642078237964257476, 1405048341078324037, 3059193199283698832, 1605012781983592984,
    7134876918849821827, 5796994175286958720, 7251651436095127661, 4565856221886323991,
];

const MDS_MATRIX_I64: [[i64; STATE_SIZE]; STATE_SIZE] = [
    [
        61402, 17845, 26798, 59689, 12021, 40901, 41351, 27521, 56951, 12034, 53865, 43244, 7454,
        33823, 28750, 1108,
    ],
    [
        1108, 61402, 17845, 26798, 59689, 12021, 40901, 41351, 27521, 56951, 12034, 53865, 43244,
        7454, 33823, 28750,
    ],
    [
        28750, 1108, 61402, 17845, 26798, 59689, 12021, 40901, 41351, 27521, 56951, 12034, 53865,
        43244, 7454, 33823,
    ],
    [
        33823, 28750, 1108, 61402, 17845, 26798, 59689, 12021, 40901, 41351, 27521, 56951, 12034,
        53865, 43244, 7454,
    ],
    [
        7454, 33823, 28750, 1108, 61402, 17845, 26798, 59689, 12021, 40901, 41351, 27521, 56951,
        12034, 53865, 43244,
    ],
    [
        43244, 7454, 33823, 28750, 1108, 61402, 17845, 26798, 59689, 12021, 40901, 41351, 27521,
        56951, 12034, 53865,
    ],
    [
        53865, 43244, 7454, 33823, 28750, 1108, 61402, 17845, 26798, 59689, 12021, 40901, 41351,
        27521, 56951, 12034,
    ],
    [
        12034, 53865, 43244, 7454, 33823, 28750, 1108, 61402, 17845, 26798, 59689, 12021, 40901,
        41351, 27521, 56951,
    ],
    [
        56951, 12034, 53865, 43244, 7454, 33823, 28750, 1108, 61402, 17845, 26798, 59689, 12021,
        40901, 41351, 27521,
    ],
    [
        27521, 56951, 12034, 53865, 43244, 7454, 33823, 28750, 1108, 61402, 17845, 26798, 59689,
        12021, 40901, 41351,
    ],
    [
        41351, 27521, 56951, 12034, 53865, 43244, 7454, 33823, 28750, 1108, 61402, 17845, 26798,
        59689, 12021, 40901,
    ],
    [
        40901, 41351, 27521, 56951, 12034, 53865, 43244, 7454, 33823, 28750, 1108, 61402, 17845,
        26798, 59689, 12021,
    ],
    [
        12021, 40901, 41351, 27521, 56951, 12034, 53865, 43244, 7454, 33823, 28750, 1108, 61402,
        17845, 26798, 59689,
    ],
    [
        59689, 12021, 40901, 41351, 27521, 56951, 12034, 53865, 43244, 7454, 33823, 28750, 1108,
        61402, 17845, 26798,
    ],
    [
        26798, 59689, 12021, 40901, 41351, 27521, 56951, 12034, 53865, 43244, 7454, 33823, 28750,
        1108, 61402, 17845,
    ],
    [
        17845, 26798, 59689, 12021, 40901, 41351, 27521, 56951, 12034, 53865, 43244, 7454, 33823,
        28750, 1108, 61402,
    ],
];

pub trait Element: Clone {
    fn is_zero(&self) -> bool;
    fn zero() -> Self;
    fn len() -> usize;
    fn one() -> Self;
}

impl Element for Felt {
    #[inline(always)] fn is_zero(&self) -> bool { self.0.iter().all(|b| b.0 == 0) }
    #[inline(always)] fn zero() -> Self { Felt([Belt(0); 3]) }
    #[inline(always)] fn len() -> usize { 3 }
    #[inline(always)] fn one() -> Self { let mut a=[Belt(0);3]; a[0]=Belt(1); Felt(a) }
}

impl Element for Belt {
    #[inline(always)] fn is_zero(&self) -> bool { self.0 == 0 }
    #[inline(always)] fn zero() -> Self { Belt(0) }
    #[inline(always)] fn len() -> usize { 1 }
    #[inline(always)] fn one() -> Self { Belt(1) }
}

impl Element for u64 {
    #[inline(always)]
    fn is_zero(&self) -> bool {
        *self == 0
    }
    #[inline(always)]
    fn zero() -> Self {
        0
    }
    #[inline(always)]
    fn len() -> usize {
        1
    }
    #[inline(always)]
    fn one() -> Self {
        1
    }
}

pub trait Poly {
    type Element: Element;

    fn data(&self) -> &[Self::Element];

    #[inline(always)]
    fn degree(&self) -> u32 {
        self.data()
            .iter()
            .rposition(|x| !Element::is_zero(x))
            .map_or(0, |i| i as u32)
    }
    #[inline(always)]
    fn leading_coeff(&self) -> &Self::Element {
        &self.data()[self.degree() as usize]
    }
    #[inline(always)]
    fn is_zero(&self) -> bool {
        let len = self.len();
        let data = self.data();
        if len == 0 || (len == 1 && data[0].is_zero()) {
            return true;
        }
        data.iter().all(|x| x.is_zero())
    }
    #[inline(always)]
    fn len(&self) -> usize {
        self.data().len()
    }
    #[inline(always)]
    fn iter(&self) -> Iter<'_, Self::Element> {
        self.data().iter()
    }
}

impl<T> Poly for &[T]
where
    T: Element,
{
    type Element = T;
    #[inline(always)]
    fn data(&self) -> &[T] {
        self
    }
}

impl<T> Poly for alloc::vec::Vec<T>
where
    T: Element,
{
    type Element = T;
    #[inline(always)]
    fn data(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T> Poly for &mut [T]
where
    T: Element,
{
    type Element = T;
    #[inline(always)]
    fn data(&self) -> &[T] {
        self
    }
}

// Wrapper types for Polys to convert from Cell. Only called from top level jet wrapper or in tests.
// Note that form/math functions will always use slice primitives like &[Felt] and &mut [Felt]
#[derive(Clone, Debug)]
#[repr(transparent)]
pub struct PolyVec<T>(pub alloc::vec::Vec<T>);

impl<T> PolyVec<T> {
    #[inline(always)]
    pub fn as_slice(&self) -> &[T] {
        self.0.as_slice()
    }
    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self.0.as_mut_slice()
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
pub struct PolySlice<'a, T>(pub &'a [T]);

#[repr(transparent)]
pub struct PolySliceMut<'a, T>(pub &'a mut [T]);

pub type BPolyVec = PolyVec<Belt>;
pub type BPolySlice<'a> = PolySlice<'a, Belt>;
pub type BPolySliceMut<'a> = PolySliceMut<'a, Belt>;

pub type FPolyVec = PolyVec<Felt>;
pub type FPolySlice<'a> = PolySlice<'a, Felt>;
pub type FPolySliceMut<'a> = PolySliceMut<'a, Felt>;

impl<T> Poly for PolyVec<T>
where
    T: Element,
{
    type Element = T;
    #[inline(always)]
    fn data(&self) -> &[Self::Element] {
        &self.0
    }
}

impl<T: Element> Poly for PolySlice<'_, T> {
    type Element = T;

    #[inline(always)]
    fn data(&self) -> &[T] {
        self.0
    }
}

impl<T: Element> Poly for PolySliceMut<'_, T> {
    type Element = T;

    #[inline(always)]
    fn data(&self) -> &[T] {
        self.0
    }
}

impl From<alloc::vec::Vec<u64>> for BPolyVec {
    fn from(b: alloc::vec::Vec<u64>) -> Self {
        PolyVec(b.into_iter().map(Belt::from).collect())
    }
}
impl From<BPolyVec> for alloc::vec::Vec<u64> {
    fn from(b: BPolyVec) -> Self {
        b.0.into_iter().map(u64::from).collect()
    }
}

impl From<Felt> for BPolyVec {
    fn from(b: Felt) -> Self {
        PolyVec(vec![b.0[0], b.0[1], b.0[2]])
    }
}

impl<'a> From<&'a Felt> for BPolySlice<'a> {
    fn from(f: &'a Felt) -> Self {
        PolySlice(&f.0)
    }
}

impl<'a> From<&'a BPolySliceMut<'_>> for BPolySlice<'a> {
    fn from(p: &'a BPolySliceMut) -> Self {
        Self(p.0)
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for BPolyVec {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        PolyVec(Vec::<Belt>::arbitrary(g))
    }
}

pub fn based_check(a: u64) -> bool {
  a < PRIME
}

pub fn bpegcd(a: &[Belt], b: &[Belt]) -> (Vec<Belt>, Vec<Belt>) {
  let mut d = vec![Belt(0); core::cmp::max(a.len(), b.len())];
  let mut u = vec![Belt(0); a.len() + b.len()];
  let mut v = vec![Belt(0); a.len() + b.len()];
  
  bpegcd_impl(a, b, &mut d, &mut u, &mut v);
  
  // Return s and v from the extended GCD
  // s is stored in first 12 elements of u, v in first 2 elements of v
  let s = u[..12].to_vec();
  let v_out = v[..2].to_vec();
  (s, v_out)
}

#[inline(always)]
pub fn bpegcd_impl(a: &[Belt], b: &[Belt], d: &mut [Belt], u: &mut [Belt], v: &mut [Belt]) {
    let mut m1_u = vec![Belt(0)];
    let mut m2_u = vec![Belt(1)];
    let mut m1_v = vec![Belt(1)];
    let mut m2_v = vec![Belt(0)];

    d.fill(Belt(0));
    u.fill(Belt(0));
    v.fill(Belt(0));

    let mut a = a.to_vec();
    let mut b = b.to_vec();

    while !b.is_zero() {
        let deg_a = a.degree();
        let deg_b = b.degree();
        let deg_q = deg_a.saturating_sub(deg_b);
        let len_q = deg_q + 1;
        let len_r = deg_b + 1;

        let mut q = vec![Belt(0); len_q as usize];
        let mut r = vec![Belt(0); len_r as usize];

        bpdvr(
            a.as_slice(),
            b.as_slice(),
            q.as_mut_slice(),
            r.as_mut_slice(),
        );

        a = b;
        b = r;

        let q_len = q.len();
        let m1_u_len = m1_u.len() as usize;

        let mut res1_len = q_len + m1_u_len - 1;
        let mut res1 = vec![Belt(0); res1_len as usize];
        bpmul(q.as_slice(), m1_u.as_slice(), res1.as_mut_slice());

        let m2_u_len = m2_u.len();

        let len_res2 = core::cmp::max(m2_u_len, res1_len);
        let mut res2 = vec![Belt(0); len_res2 as usize];
        bpsub(m2_u.as_slice(), res1.as_slice(), res2.as_mut_slice());

        m2_u = m1_u;
        m1_u = res2;

        let m1_v_len = m1_v.len() as usize;

        res1.fill(Belt(0));
        res1_len = q_len + m1_v_len - 1;

        bpmul(q.as_slice(), m1_v.as_slice(), res1.as_mut_slice());

        let m2_v_len = m2_v.len();

        let len_res3 = core::cmp::max(m2_v_len, res1_len);
        let mut res3 = vec![Belt(0); len_res3 as usize];

        bpsub(m2_v.as_slice(), res1.as_slice(), res3.as_mut_slice());

        m2_v = m1_v;
        m1_v = res3;
    }

    let a_len = a.len();
    d[0..a_len].copy_from_slice(&a[0..a_len]);

    let m2_u_len = m2_u.len();
    let m2_v_len = m2_v.len();

    u[0..(m2_u_len as usize)].copy_from_slice(&m2_u[0..(m2_u_len as usize)]);
    v[0..(m2_v_len as usize)].copy_from_slice(&m2_v[0..(m2_v_len as usize)]);
}

#[inline(always)]
pub fn bpdvr(a: &[Belt], b: &[Belt], q: &mut [Belt], res: &mut [Belt]) {
    if a.is_zero() {
        q.fill(Belt(0));
        res.fill(Belt(0));
        return;
    } else if b.is_zero() {
        panic!("divide by zero\r");
    };

    q.fill(Belt(0));
    res.fill(Belt(0));

    let a_end = a.degree() as usize;
    let mut r = a[0..(a_end + 1)].to_vec();

    let deg_b = b.degree();

    let mut i = a_end;
    let end_b = deg_b as usize;
    let mut deg_r = a.degree();
    let mut q_index = deg_r.saturating_sub(deg_b);

    while deg_r >= deg_b {
        let coeff = r[i] / b[end_b];
        q[q_index as usize] = coeff;
        for k in 0..(deg_b + 1) {
            let index = k as usize;
            if k <= a_end as u32 && k < b.len() as u32 && k <= (i as u32) {
                r[i - index] = r[i - index] - coeff * b[end_b - index];
            }
        }
        deg_r = deg_r.saturating_sub(1);
        q_index = q_index.saturating_sub(1);
        if deg_r == 0 && r[0] == Belt(0) {
            break;
        }
        i -= 1;
    }

    let r_len = deg_r + 1;
    res[0..(r_len as usize)].copy_from_slice(&r[0..(r_len as usize)]);
}


pub fn tip5_permute(sponge: &mut [u64; 16]) {
  for i in 0..NUM_ROUNDS {
      // old: let a = sbox_layer(array_ref![sponge, 0, STATE_SIZE]);
      let a = sbox_layer(&*sponge);
      let b = linear_layer(&a);

      for j in 0..STATE_SIZE {
          let r_cons = (((ROUND_CONSTANTS[i * STATE_SIZE + j] as u128) * R) % PRIME_128) as u64;
          sponge[j] = badd(r_cons, b[j]);
      }
  }
}

fn sbox_layer(state: &[u64; STATE_SIZE]) -> [u64; STATE_SIZE] {
    let mut res: [u64; STATE_SIZE] = [0; STATE_SIZE];

    for i in 0..NUM_SPLIT_AND_LOOKUP {
        let mut bytes = state[i].to_le_bytes();
        for i in 0..8 {
            bytes[i] = LOOKUP_TABLE[bytes[i] as usize];
        }
        res[i] = u64::from_le_bytes(bytes);
    }

    for j in NUM_SPLIT_AND_LOOKUP..STATE_SIZE {
        res[j] = bpow(state[j], 7);
    }
    res
}

fn linear_layer(state: &[u64; 16]) -> [u64; 16] {
    let mut result = [0u64; 16];

    for i in 0..16 {
        for j in 0..16 {
            let matrix_element = MDS_MATRIX_I64[i][j] as u64;
            let product = bmul(matrix_element, state[j]);
            result[i] = badd(result[i], product);
        }
    }

    result
}

#[macro_export]
macro_rules! based {
    ( $( $x:expr ),* ) => {{
        $(
            debug_assert!(
                $crate::math::math::based_check($x),
                "element must be inside the field"
            );
        )*
    }};
}

#[inline(always)]
pub fn badd(a: u64, b: u64) -> u64 {
    based!(a);
    based!(b);

    let b = PRIME.wrapping_sub(b);
    let (r, c) = a.overflowing_sub(b);
    let adj = 0u32.wrapping_sub(c as u32);
    r.wrapping_sub(adj as u64)
}

#[inline(always)]
pub fn bmul(a: u64, b: u64) -> u64 {
    based!(a);
    based!(b);
    reduce((a as u128) * (b as u128))
}


#[inline(always)]
pub fn bpow(mut a: u64, mut b: u64) -> u64 {
    based!(a);
    based!(b);

    let mut c: u64 = 1;
    if b == 0 {
        return c;
    }

    while b > 1 {
        if b & 1 == 0 {
            a = reduce((a as u128) * (a as u128));
            b /= 2;
        } else {
            c = reduce((c as u128) * (a as u128));
            a = reduce((a as u128) * (a as u128));
            b = (b - 1) / 2;
        }
    }
    reduce((c as u128) * (a as u128))
}

#[inline(always)]
pub fn bpsub(a: &[Belt], b: &[Belt], res: &mut [Belt]) {
    let a_len = a.len();
    let b_len = b.len();

    let res_len = core::cmp::max(a_len, b_len);

    for i in 0..res_len {
        let n = i;
        if i < a_len && i < b_len {
            res[n] = a[n] - b[n];
        } else if i < a_len {
            res[n] = a[n];
        } else {
            res[n] = -b[n];
        }
    }
}

#[inline(always)]
pub fn bpmul(a: &[Belt], b: &[Belt], res: &mut [Belt]) {
    if a.is_zero() || b.is_zero() {
        res.fill(Belt(0));
        return;
    }

    res.fill(Belt(0));

    let a_len = a.len();
    let b_len = b.len();

    for i in 0..a_len {
        if a[i] == Belt(0) {
            continue;
        }
        for j in 0..b_len {
            res[i + j] = res[i + j] + a[i] * b[j];
        }
    }
}


/// Reduce a 128 bit number
#[inline(always)]
pub fn reduce(n: u128) -> u64 {
    reduce_159(n as u64, (n >> 64) as u32, (n >> 96) as u64)
}

/// Reduce a 159 bit number
/// See <https://cp4space.hatsya.com/2021/09/01/an-efficient-prime-for-number-theoretic-transforms/>
/// See <https://github.com/mir-protocol/plonky2/blob/3a6d693f3ffe5aa1636e0066a4ea4885a10b5cdf/field/src/goldilocks_field.rs#L340-L356>
#[inline(always)]
pub fn reduce_159(low: u64, mid: u32, high: u64) -> u64 {
    let (mut low2, carry) = low.overflowing_sub(high);
    if carry {
        low2 = low2.wrapping_add(PRIME);
    }

    let mut product = (mid as u64) << 32;
    product -= product >> 32;

    let (mut result, carry) = product.overflowing_add(low2);
    if carry {
        result = result.wrapping_sub(PRIME);
    }

    if result >= PRIME {
        result -= PRIME;
    }
    result
}

#[inline(always)]
pub fn bsub(a: u64, b: u64) -> u64 {
    based!(a, b);
    let (r, borrow) = a.overflowing_sub(b);
    if borrow {
        r.wrapping_add(PRIME)
    } else {
        r
    }
}

#[inline(always)]
pub fn bneg(a: u64) -> u64 {
    based!(a);
    if a == 0 {
        0
    } else {
        PRIME - a
    }
}

#[inline(always)]
pub fn binv(a: u64) -> u64 {
    based!(a);
    // Fermat's little theorem: a^(p-2) = a^-1 mod p
    bpow(a, PRIME - 2)
}
