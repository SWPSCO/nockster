extern crate alloc;

use alloc::vec::Vec;
use core::fmt;

pub const PRIME: u64 = 18446744069414584321;

#[derive(Debug)]
pub enum NounDecodeError {
    ExpectedAtom,
    ExpectedCell,
    FieldError(&'static str, &'static str),
    InvalidEnumVariant,
    InvalidEnumData,
    InvalidTag,
    Custom(&'static str),
}

impl fmt::Display for NounDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedAtom => write!(f, "Expected atom, found cell"),
            Self::ExpectedCell => write!(f, "Expected cell, found atom"),
            Self::FieldError(field, msg) => write!(f, "Failed to decode field {}: {}", field, msg),
            Self::InvalidEnumVariant => write!(f, "Invalid enum variant"),
            Self::InvalidEnumData => write!(f, "Invalid enum data"),
            Self::InvalidTag => write!(f, "Invalid tag"),
            Self::Custom(msg) => write!(f, "Custom error: {}", msg),
        }
    }
}

pub trait NounEncode {
    fn to_noun<A: NounAllocator>(&self, allocator: &mut A) -> Noun;
}

pub trait NounDecode: Sized {
    fn from_noun(noun: &Noun) -> Result<Self, NounDecodeError>;
}

// Minimal Noun/Atom types for no_std
#[derive(Clone, Copy, Debug)]
pub enum Noun {
    Atom(u64),
    Cell { head: *const Noun, tail: *const Noun },
}

impl Noun {
    pub fn as_atom(&self) -> Result<u64, ()> {
        match self {
            Noun::Atom(val) => Ok(*val),
            _ => Err(()),
        }
    }
    
    pub fn as_cell(&self) -> Result<(*const Noun, *const Noun), ()> {
        match self {
            Noun::Cell { head, tail } => Ok((*head, *tail)),
            _ => Err(()),
        }
    }
    
    pub fn is_atom(&self) -> bool {
        matches!(self, Noun::Atom(_))
    }
    
    pub fn is_cell(&self) -> bool {
        matches!(self, Noun::Cell { .. })
    }
}

pub trait NounAllocator {
    fn alloc_atom(&mut self, val: u64) -> Noun;
    fn alloc_cell(&mut self, head: Noun, tail: Noun) -> Noun;
}

// Simple bump allocator for ESP32
pub struct BumpAllocator<'a> {
    buffer: &'a mut [u8],
    offset: usize,
}

impl<'a> BumpAllocator<'a> {
    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self { buffer, offset: 0 }
    }
    
    fn alloc<T>(&mut self, val: T) -> *const T {
        let size = core::mem::size_of::<T>();
        let align = core::mem::align_of::<T>();
        let padding = (align - (self.offset % align)) % align;
        self.offset += padding;
        
        if self.offset + size > self.buffer.len() {
            panic!("BumpAllocator out of memory");
        }
        
        let ptr = &mut self.buffer[self.offset] as *mut u8 as *mut T;
        unsafe {
            ptr.write(val);
        }
        self.offset += size;
        ptr as *const T
    }
}

impl<'a> NounAllocator for BumpAllocator<'a> {
    fn alloc_atom(&mut self, val: u64) -> Noun {
        Noun::Atom(val)
    }
    
    fn alloc_cell(&mut self, head: Noun, tail: Noun) -> Noun {
        let head_ptr = self.alloc(head);
        let tail_ptr = self.alloc(tail);
        Noun::Cell { head: head_ptr, tail: tail_ptr }
    }
}

// Helper functions
pub fn D(val: u64) -> Noun {
    Noun::Atom(val)
}

pub fn T<A: NounAllocator>(allocator: &mut A, items: &[Noun]) -> Noun {
    if items.is_empty() {
        return D(0);
    }
    if items.len() == 1 {
        return items[0];
    }
    
    let mut result = items[items.len() - 1];
    for i in (0..items.len() - 1).rev() {
        result = allocator.alloc_cell(items[i], result);
    }
    result
}

// Belt implementation
impl NounEncode for Belt {
    fn to_noun<A: NounAllocator>(&self, allocator: &mut A) -> Noun {
        allocator.alloc_atom(self.0)
    }
}

impl NounDecode for Belt {
    fn from_noun(noun: &Noun) -> Result<Self, NounDecodeError> {
        let value = noun.as_atom()
            .map_err(|_| NounDecodeError::ExpectedAtom)?;
        
        if !based_check(value) {
            return Err(NounDecodeError::Custom("Belt value not based"));
        }
        Ok(Belt(value))
    }
}

// u64 implementation
impl NounEncode for u64 {
    fn to_noun<A: NounAllocator>(&self, allocator: &mut A) -> Noun {
        allocator.alloc_atom(*self)
    }
}

impl NounDecode for u64 {
    fn from_noun(noun: &Noun) -> Result<Self, NounDecodeError> {
        noun.as_atom().map_err(|_| NounDecodeError::ExpectedAtom)
    }
}

// Array implementations
impl<T: NounEncode, const N: usize> NounEncode for [T; N] {
    fn to_noun<A: NounAllocator>(&self, allocator: &mut A) -> Noun {
        if N == 0 {
            return D(0);
        }
        
        let mut result = self[N - 1].to_noun(allocator);
        for i in (0..N - 1).rev() {
            let elem = self[i].to_noun(allocator);
            result = allocator.alloc_cell(elem, result);
        }
        result
    }
}

impl<T: NounDecode, const N: usize> NounDecode for [T; N] {
    fn from_noun(noun: &Noun) -> Result<Self, NounDecodeError> {
        if N == 0 {
            return Ok(unsafe { core::mem::zeroed() });
        }
        
        let mut result = Vec::with_capacity(N);
        let mut current = *noun;
        
        for _ in 0..N - 1 {
            let (head_ptr, tail_ptr) = current.as_cell()
                .map_err(|_| NounDecodeError::ExpectedCell)?;
            let head = unsafe { *head_ptr };
            let item = T::from_noun(&head)?;
            result.push(item);
            current = unsafe { *tail_ptr };
        }
        
        let last = T::from_noun(&current)?;
        result.push(last);
        
        result.try_into()
            .map_err(|_| NounDecodeError::Custom("Array conversion failed"))
    }
}

// Tuple implementations
impl<A: NounEncode, B: NounEncode> NounEncode for (A, B) {
    fn to_noun<Alloc: NounAllocator>(&self, allocator: &mut Alloc) -> Noun {
        let a = self.0.to_noun(allocator);
        let b = self.1.to_noun(allocator);
        T(allocator, &[a, b])
    }
}

impl<A: NounDecode, B: NounDecode> NounDecode for (A, B) {
    fn from_noun(noun: &Noun) -> Result<Self, NounDecodeError> {
        let (head_ptr, tail_ptr) = noun.as_cell()
            .map_err(|_| NounDecodeError::ExpectedCell)?;
        let a = A::from_noun(unsafe { &*head_ptr })?;
        let b = B::from_noun(unsafe { &*tail_ptr })?;
        Ok((a, b))
    }
}

// Vec implementation
impl<T: NounEncode> NounEncode for Vec<T> {
    fn to_noun<A: NounAllocator>(&self, allocator: &mut A) -> Noun {
        self.iter().rev().fold(D(0), |acc, item| {
            let item_noun = item.to_noun(allocator);
            allocator.alloc_cell(item_noun, acc)
        })
    }
}

impl<T: NounDecode> NounDecode for Vec<T> {
    fn from_noun(noun: &Noun) -> Result<Self, NounDecodeError> {
        let mut result = Vec::new();
        let mut current = *noun;
        
        while let Ok((head_ptr, tail_ptr)) = current.as_cell() {
            let item = T::from_noun(unsafe { &*head_ptr })?;
            result.push(item);
            current = unsafe { *tail_ptr };
        }
        
        if current.as_atom() != Ok(0) {
            return Err(NounDecodeError::Custom("Invalid list termination"));
        }
        
        Ok(result)
    }
}

fn based_check(value: u64) -> bool {
    return value < PRIME
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
pub struct Belt(pub u64);