extern crate alloc;

use super::noun_serde::{Noun, NounAllocator, NounDecode, NounDecodeError, NounEncode};
use crate::math::math::{badd, based_check, binv, bmul, bneg, bsub, PRIME};
use alloc::vec;
use alloc::vec::Vec;
use core::ops::{Add, Mul, Neg, Sub};
use core::slice::Iter;

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
pub struct Belt(pub u64);

impl NounEncode for Belt {
    fn to_noun<A: NounAllocator>(&self, allocator: &mut A) -> Noun {
        allocator.alloc_atom(self.0)
    }
}

impl NounDecode for Belt {
    fn from_noun(noun: &Noun) -> Result<Self, NounDecodeError> {
        let value = noun.as_atom().map_err(|_| NounDecodeError::ExpectedAtom)?;

        if !based_check(value) {
            return Err(NounDecodeError::Custom("Belt value not based"));
        }
        Ok(Belt(value))
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
pub struct Felt(pub [Belt; 3]);

pub trait Element: Clone {
    fn is_zero(&self) -> bool;
    fn zero() -> Self;
    fn len() -> usize;
    fn one() -> Self;
}

impl From<u64> for Belt {
    #[inline(always)]
    fn from(x: u64) -> Self {
        Belt(if x >= PRIME { x % PRIME } else { x })
    }
}

impl From<Belt> for u64 {
    #[inline(always)]
    fn from(b: Belt) -> u64 {
        b.0
    }
}

impl Element for Felt {
    #[inline(always)]
    fn is_zero(&self) -> bool {
        self.0.iter().all(|b| b.0 == 0)
    }
    #[inline(always)]
    fn zero() -> Self {
        Felt([Belt(0); 3])
    }
    #[inline(always)]
    fn len() -> usize {
        3
    }
    #[inline(always)]
    fn one() -> Self {
        let mut a = [Belt(0); 3];
        a[0] = Belt(1);
        Felt(a)
    }
}

impl Element for Belt {
    #[inline(always)]
    fn is_zero(&self) -> bool {
        self.is_zero()
    }
    #[inline(always)]
    fn zero() -> Self {
        Belt::zero()
    }
    #[inline(always)]
    fn len() -> usize {
        1
    }
    #[inline(always)]
    fn one() -> Self {
        Belt::one()
    }
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

impl<T> Poly for Vec<T>
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
pub struct PolyVec<T>(pub Vec<T>);

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

// For testing BPolyVec funcs
impl From<Vec<u64>> for BPolyVec {
    fn from(b: Vec<u64>) -> Self {
        let belts: Vec<Belt> = b.into_iter().map(|item| item.into()).collect();
        PolyVec(belts)
    }
}

impl From<BPolyVec> for Vec<u64> {
    fn from(b: BPolyVec) -> Self {
        let belts: Vec<u64> = b.0.into_iter().map(|item| item.into()).collect();
        belts
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

impl core::ops::Div for Belt {
    type Output = Belt;

    fn div(self, rhs: Belt) -> Belt {
        Belt(bmul(self.0, binv(rhs.0)))
    }
}

impl Belt {
    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }

    pub fn zero() -> Self {
        Belt(0)
    }

    pub fn one() -> Self {
        Belt(1)
    }
}

impl Add for Belt {
    type Output = Belt;

    fn add(self, rhs: Belt) -> Belt {
        Belt(badd(self.0, rhs.0))
    }
}

impl Sub for Belt {
    type Output = Belt;

    fn sub(self, rhs: Belt) -> Belt {
        Belt(bsub(self.0, rhs.0))
    }
}

impl Mul for Belt {
    type Output = Belt;

    fn mul(self, rhs: Belt) -> Belt {
        Belt(bmul(self.0, rhs.0))
    }
}

impl Neg for Belt {
    type Output = Belt;

    fn neg(self) -> Belt {
        Belt(bneg(self.0))
    }
}

impl Belt {
    pub fn inv(self) -> Belt {
        Belt(binv(self.0))
    }
}
