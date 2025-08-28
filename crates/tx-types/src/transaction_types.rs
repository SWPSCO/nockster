use nockapp::Noun;
use nockapp::noun::slab::NounSlab;
use nockapp::noun::AtomExt;
use noun_serde::{NounDecode, NounDecodeError, NounEncode};
use nockvm::noun::{Atom, NounAllocator, D, T};
use std::collections::HashMap;
use crate::collections::{ZSet, ZMap};
use crate::hashing::hashable::Hashable;
use crate::hashing::hasher::hash_hashable;
use crate::hashing::tip5::Tip5Hasher;
use crate::u320::U320;

// Coin name structure  
#[derive(Debug, Clone, NounEncode, NounDecode, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Coins {
   pub value: u64
}

// page number name structure  
#[derive(Debug, Clone, NounEncode, NounDecode, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PageNumber {
   pub value: u64
}

// Hash wrapper for transaction IDs and other hashes
#[derive(Debug, Clone, NounEncode, NounDecode, PartialEq, Eq, Hash)]
pub struct Hash {
    pub values: [u64; 5],
}

impl PartialOrd for Hash {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Hash {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Compare as UBig integers, not lexicographically
        // This ensures hash comparisons match the gor-tip behavior
        self.to_ubig().cmp(&other.to_ubig())
    }
}

impl Hash {
    /// Convert the Hash to a UBig integer for comparison
    /// This treats the hash as a 320-bit integer (5 * 64 bits)
    /// values[0] is least significant, values[4] is most significant
    pub fn to_ubig(&self) -> ibig::UBig {
        use ibig::UBig;
        
        // Build the UBig from bytes in little-endian order
        let mut bytes = Vec::with_capacity(40); // 5 * 8 bytes
        
        // Add each u64 in little-endian byte order
        for value in &self.values {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        
        // Create UBig from little-endian bytes
        UBig::from_le_bytes(&bytes)
    }
    
    /// Create a Hash from a UBig integer (inverse of to_ubig)
    /// Returns None if the UBig is too large to fit in 320 bits
    pub fn from_ubig(big: &ibig::UBig) -> Option<Self> {
        // Convert to little-endian bytes
        let bytes = big.to_le_bytes();
        
        // Check if it fits in 320 bits (40 bytes)
        if bytes.len() > 40 {
            return None;
        }
        
        let mut values = [0u64; 5];
        
        // Parse each u64 from the bytes
        for i in 0..5 {
            let start = i * 8;
            let end = (i + 1) * 8;
            
            if start < bytes.len() {
                // Get up to 8 bytes for this u64
                let mut u64_bytes = [0u8; 8];
                let available = (bytes.len() - start).min(8);
                u64_bytes[..available].copy_from_slice(&bytes[start..start + available]);
                values[i] = u64::from_le_bytes(u64_bytes);
            }
        }
        
        Some(Hash { values })
    }
    
    /// Convert base58 encoded string to Hash
    /// Implements the Hoon from-b58 function:
    /// ++  from-b58  |=(=cord `form`(atom-to-digest:tip5 (de-base58 (trip cord))))
    pub fn from_b58(base58: &str) -> Result<Self, String> {
        use bs58;
        use num_bigint::BigUint;
        
        // Decode base58 to bytes (de-base58 in Hoon)
        let bytes = bs58::decode(base58)
            .into_vec()
            .map_err(|e| format!("Invalid base58: {}", e))?;
        
        // Convert to BigUint (the atom in Hoon)
        let big_num = BigUint::from_bytes_be(&bytes);
        
        // The Goldilocks prime used in TIP5 hashing
        const GOLDILOCKS_PRIME: u64 = 0xFFFF_FFFF_0000_0001;
        let p = BigUint::from(GOLDILOCKS_PRIME);
        
        // atom-to-digest:tip5 converts the atom using the formula:
        // digest = [a, b, c, d, e] where atom = a + b*p + c*p² + d*p³ + e*p⁴
        let mut values = [0u64; 5];
        let mut remainder = big_num;
        
        // Extract coefficients in order (a, b, c, d, e)
        for i in 0..5 {
            if i < 4 {
                // For a, b, c, d: take remainder mod p
                let coeff = &remainder % &p;
                values[i] = coeff.try_into().unwrap_or(0);
                remainder = remainder / &p;
            } else {
                // For e: just take what's left
                values[i] = (&remainder).try_into().unwrap_or(0);
            }
        }
        
        Ok(Hash { values })
    }
    
    /// Convert Hash to base58 encoded string
    /// Implements the Hoon to-b58 function:
    /// ++  to-b58  |=(has=form `cord`(crip (en-base58 (digest-to-atom:tip5 has))))
    pub fn to_b58(&self) -> String {
        use num_bigint::BigUint;
        use bs58;
        
        // The Goldilocks prime
        const GOLDILOCKS_PRIME: u64 = 0xFFFF_FFFF_0000_0001;
        let p = BigUint::from(GOLDILOCKS_PRIME);
        
        // digest-to-atom:tip5 uses formula: a + b*p + c*p² + d*p³ + e*p⁴
        let mut result = BigUint::from(self.values[0]);
        
        for i in 1..5 {
            let power = p.pow(i as u32);
            result += BigUint::from(self.values[i]) * power;
        }
        
        bs58::encode(result.to_bytes_be()).into_string()
    }
    pub fn from_base58(b58_str: &str) -> anyhow::Result<Self> {
        let n = U320::from_base58(b58_str)?;
        // Four divmods by p: collect remainders a..d; final quotient e
        let (q1, a) = n.divrem_p();
        let (q2, b) = q1.divrem_p();
        let (q3, c) = q2.divrem_p();
        let (e_q, d) = q3.divrem_p();
        let e = e_q.as_single_u64()?;
        Ok(Hash { values: [a, b, c, d, e] })
    }
    /// Convert Hash to base58 encoded string
    pub fn to_base58(&self) -> String {
        // N = ((((e*p + d)*p + c)*p + b)*p + a)
        let [a, b, c, d, e] = self.values;
        let mut n = U320::from_u64(e);
        n.mul_p_add_u64(d);
        n.mul_p_add_u64(c);
        n.mul_p_add_u64(b);
        n.mul_p_add_u64(a);
        n.to_base58()
    }
}

// Note name structure  
#[derive(Debug, Clone, NounEncode, NounDecode, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NName {
    pub p: Vec<Hash>
}

impl NName {
    pub fn to_hashable(&self) -> Hashable {
        // NName hashable from Hoon (found in tx-engine.hoon):
        // ++  hashable
        //   |=  =form
        //   ^-  hashable:tip5
        //   [[%hash -.form] [%hash +<.form] [%leaf +>.form]]
        //
        // This creates a nested cell structure:
        // - First hash wrapped with %hash tag
        // - Second hash wrapped with %hash tag
        // - The nil (~) wrapped with %leaf tag
        
        if self.p.len() >= 2 {
            // Create the structure: [[%hash hash1] [%hash hash2] [%leaf 0]]
            // This is a triple where each hash is wrapped with Hash variant
            // and the terminator is wrapped with Leaf variant
            Hashable::triple(
                Hashable::Hash(self.p[0].clone()),
                Hashable::Hash(self.p[1].clone()),
                Hashable::null()  // null() returns Leaf(0)
            )
        } else if self.p.len() == 1 {
            // If only one hash, still need proper structure
            // Assuming it would be [%hash hash1] [%leaf 0] [%leaf 0]
            Hashable::triple(
                Hashable::Hash(self.p[0].clone()),
                Hashable::null(),
                Hashable::null()
            )
        } else {
            // Empty NName
            Hashable::null()
        }
    }
    
    pub fn to_hash(&self) -> Hash {
        hash_hashable(&self.to_hashable())
    }
}

// TimelockIntent: Option<(absolute, relative)> where both are TimelockRange
// ~ means "no intent", Some((absolute, relative)) means there is intent
pub type TimelockIntent = Option<(TimelockRange, TimelockRange)>;

impl Timelock {
    pub fn to_hashable(&self) -> Hashable {
        // Timelock just delegates to its TimelockIntent
        to_hashable_timelock_intent(&self.intent)
    }
    
    pub fn to_hash(&self) -> Hash {
        hash_hashable(&self.to_hashable())
    }
}

// Helper function for TimelockIntent since it's a type alias
pub fn to_hashable_timelock_intent(intent: &TimelockIntent) -> Hashable {
    // TimelockIntent hashable from Hoon:
    // ?~  form  leaf+~
    // :+  leaf+~
    //   (hashable:timelock-range absolute.u.form)
    // (hashable:timelock-range relative.u.form)
    
    match intent {
        None => Hashable::null(),
        Some((absolute, relative)) => {
            Hashable::triple(
                Hashable::null(),
                absolute.to_hashable(),
                relative.to_hashable(),
            )
        }
    }
}

// Timelock: A TimelockIntent that cannot be Some with both ranges empty
// This wraps TimelockIntent with validation that it's not [~ ~ ~]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Timelock {
    pub intent: TimelockIntent,
}

impl Timelock {
    /// Create a new Timelock, validating it's not [~ ~ ~]
    pub fn new(intent: TimelockIntent) -> Result<Self, String> {
        if let Some((ref absolute, ref relative)) = intent {
            // Check if both ranges are empty (equivalent to [~ ~ ~])
            if absolute.min.is_none() && absolute.max.is_none() &&
               relative.min.is_none() && relative.max.is_none() {
                return Err("Timelock cannot be [~ ~ ~] (both ranges empty)".to_string());
            }
        }
        Ok(Timelock { intent })
    }
    
    /// Create a Timelock that allows any intent (used for testing/construction)
    pub fn new_unchecked(intent: TimelockIntent) -> Self {
        Timelock { intent }
    }
}

impl NounEncode for Timelock {
    fn to_noun<A: NounAllocator>(&self, allocator: &mut A) -> Noun {
        self.intent.to_noun(allocator)
    }
}

impl NounDecode for Timelock {
    fn from_noun<A: NounAllocator>(allocator: &mut A, noun: &Noun) -> Result<Self, NounDecodeError> {
        let intent = TimelockIntent::from_noun(allocator, noun)?;
        Timelock::new(intent)
            .map_err(|e| NounDecodeError::Custom(e))
    }
}

// Timelock range structure
#[derive(Debug, Clone, NounEncode, NounDecode, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TimelockRange {
    pub min: Option<PageNumber>,
    pub max: Option<PageNumber>,
}

impl TimelockRange {
    pub fn to_hashable(&self) -> Hashable {
        // Following the Hoon pattern:
        // :-  ?~(min.form %leaf^~ [%leaf^~ leaf+u.min.form])
        // ?~(max.form %leaf^~ [%leaf^~ leaf+u.max.form])
        
        let min_hashable = match &self.min {
            None => Hashable::null(),
            Some(val) => Hashable::cell(
                Hashable::null(),
                Hashable::leaf_u64(val.value)
            ),
        };
        
        let max_hashable = match &self.max {
            None => Hashable::null(),
            Some(val) => Hashable::cell(
                Hashable::null(),
                Hashable::leaf_u64(val.value)
            ),
        };
        
        Hashable::cell(min_hashable, max_hashable)
    }
    
    pub fn to_hash(&self) -> Hash {
        hash_hashable(&self.to_hashable())
    }
}
// F6LT is a 6-element field type (a0-a5 in Hoon)
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct F6LT {
    pub values: [u64; 6],
}

// Manual NounEncode/NounDecode implementations for F6LT
impl NounEncode for F6LT {
    fn to_noun<A: nockvm::noun::NounAllocator>(&self, allocator: &mut A) -> nockvm::noun::Noun {
        use nockvm::noun::{Atom, T};
        let atoms = self.values.map(|v| Atom::new(allocator, v).as_noun());
        T(allocator, &atoms)
    }
}

impl NounDecode for F6LT {
    fn from_noun<A: NounAllocator>(_: &mut A, noun: &Noun) -> Result<Self, NounDecodeError> {
        // Extract 6 values from nested cell structure
        let mut values = [0u64; 6];
        let mut current = *noun;
        
        for i in 0..5 {
            let cell = current.as_cell()
                .map_err(|_| NounDecodeError::ExpectedCell)?;
            values[i] = cell.head().as_atom()
                .map_err(|_| NounDecodeError::ExpectedAtom)?
                .as_u64()?;
            current = cell.tail();
        }
        
        // Last element
        values[5] = current.as_atom()
            .map_err(|_| NounDecodeError::ExpectedAtom)?
            .as_u64()?;
        
        Ok(F6LT { values })
    }
}

// Schnorr signature structure a a-pt:cheta
#[derive(Debug, Clone, NounEncode, NounDecode, PartialEq, Eq, Hash)]
pub struct SchnorrPubkey {
    pub x: F6LT,
    pub y: F6LT,
    pub inf: bool,
}

use num_bigint::BigUint;
use num_traits::{Zero};

impl SchnorrPubkey {
    pub fn to_hashable(&self) -> Hashable {
        // In Hoon, this is [%leaf form] where form is the pubkey noun.
        // Since our Hashable::Leaf only supports bytes (not arbitrary nouns),
        // and since the Hoon implementation ultimately hashes the noun anyway,
        // we pre-hash the noun and return Hashable::Hash.
        // This is semantically equivalent to Hoon's (hash [%leaf form]).
        let mut slab: NounSlab = NounSlab::new();
        let noun = self.to_noun(&mut slab);
        let hash = Tip5Hasher::hash_noun(noun)
            .unwrap_or_else(|_| Hash { values: [0; 5] });
        Hashable::Hash(hash)
    }
    
    pub fn to_hash(&self) -> Hash {
        // Since to_hashable returns Hash, extract it directly
        match self.to_hashable() {
            Hashable::Hash(h) => h,
            _ => hash_hashable(&self.to_hashable())
        }
    }
    
    /// Convert base58 encoded string to SchnorrPubkey
    /// Implements the Hoon from-b58 function:
    /// ++  from-b58  |=(=cord `form`(base58-to-a-pt:cheetah cord))
    /// 
    /// The base58 encoding represents a compressed elliptic curve point
    pub fn from_b58(base58: &str) -> Result<Self, String> {
        use bs58;
        use num_bigint::BigUint;
        
        // Decode base58 to bytes
        let bytes = bs58::decode(base58)
            .into_vec()
            .map_err(|e| format!("Invalid base58: {}", e))?;
        
        // Convert to BigUint
        let big_num = BigUint::from_bytes_be(&bytes);
        
        // For elliptic curve points in Nockchain/cheetah:
        // The encoding is a 384-bit value (2 * 192 bits for x and y coordinates)
        // Each coordinate has 6 field elements (6 * 64 bits = 384 bits)
        // Plus 1 bit for the infinity flag
        
        // Extract the infinity flag (least significant bit)
        let inf = (&big_num & BigUint::from(1u32)) == BigUint::from(1u32);
        let mut point_data = big_num >> 1; // Remove the infinity bit
        
        // Extract y coordinate (next 384 bits)
        let mut y_values = [0u64; 6];
        for i in 0..6 {
            y_values[i] = (&point_data & BigUint::from(u64::MAX)).try_into().unwrap_or(0);
            point_data >>= 64;
        }
        
        // Extract x coordinate (next 384 bits)
        let mut x_values = [0u64; 6];
        for i in 0..6 {
            x_values[i] = (&point_data & BigUint::from(u64::MAX)).try_into().unwrap_or(0);
            point_data >>= 64;
        }

        Ok(SchnorrPubkey {
            x: F6LT { values: x_values },
            y: F6LT { values: y_values },
            inf,
        })
    }
    
    /// Convert SchnorrPubkey to base58 encoded string
    /// Implements the Hoon to-b58 function:
    /// ++  to-b58  |=(sop=form `cord`(a-pt-to-base58:cheetah sop))
    pub fn to_b58(&self) -> String {
        use num_bigint::BigUint;
        use bs58;
        
        // Build the encoding: x coordinates + y coordinates + infinity flag
        let mut result = BigUint::from(0u32);
        
        // Add x coordinate (most significant)
        for i in (0..6).rev() {
            result <<= 64;
            result |= BigUint::from(self.x.values[i]);
        }
        
        // Add y coordinate
        for i in (0..6).rev() {
            result <<= 64;
            result |= BigUint::from(self.y.values[i]);
        }
        
        // Add infinity flag (least significant bit)
        result <<= 1;
        if self.inf {
            result |= BigUint::from(1u32);
        }
        
        bs58::encode(result.to_bytes_be()).into_string()
    }

    pub fn from_base58(s: &str) -> Self {
        let pk_vec = Self::rip64(&Self::de_base58(s));
        let x = F6LT { values: [pk_vec[0], pk_vec[1], pk_vec[2], pk_vec[3], pk_vec[4], pk_vec[5]] };
        let y = F6LT { values: [pk_vec[6], pk_vec[7], pk_vec[8], pk_vec[9], pk_vec[10], pk_vec[11]] };
        SchnorrPubkey { x, y, inf: false }
    }
    fn de_base58(s: &str) -> BigUint {
        const ALPH: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
        let mut n = BigUint::zero();
        for ch in s.chars() {
            let d = ALPH.find(ch).expect("invalid base58 char") as u32;
            n = n * 58u32 + d;
        }
        n
    }
    fn rip64(n: &BigUint) -> Vec<u64> {
        let mut x = n.clone();
        let mut out = Vec::new();
        let mask = BigUint::from(u128::from(u64::MAX));
        let sixty_four = 64u32;
        while !x.is_zero() {
            let w = (&x & &mask).to_u64_digits()[0];
            out.push(w);
            x >>= sixty_four;
        }
        if out.is_empty() { out.push(0); }
        out
    }
}

// Implement PartialOrd and Ord manually to avoid conflict with DorTip
impl PartialOrd for SchnorrPubkey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SchnorrPubkey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        
        // First try to compare based on hash (gor-tip style)
        // Since we don't have the hash computation here, fall back to dor comparison
        
        // Compare inf flag first
        match self.inf.cmp(&other.inf) {
            Ordering::Equal => {}
            other => return other,
        }
        
        // Compare x coordinates
        match self.x.values.cmp(&other.x.values) {
            Ordering::Equal => {}
            other => return other,
        }
        
        // Compare y coordinates
        self.y.values.cmp(&other.y.values)
    }
}

#[derive(Debug, Clone, NounEncode, NounDecode, PartialEq, Eq, PartialOrd, Ord)]
pub struct Lock {
    pub m: u64,
    pub pubkeys: ZSet<SchnorrPubkey>,
}

impl Lock {
    pub fn to_hashable(&self) -> Hashable {
        // Lock hashable from Hoon:
        // [leaf+m.form (hashable-pubkeys pubkeys.form)]
        // Where hashable-pubkeys is recursive on z-set:
        // ?~  pubkeys  leaf+pubkeys
        // :+  hash+(hash:schnorr-pubkey n.pubkeys)
        //   $(pubkeys l.pubkeys)
        // $(pubkeys r.pubkeys)
        
        // Use ZSet's to_hashable method which properly traverses the tree
        let pubkeys_hashable = self.pubkeys.to_hashable(|pubkey| {
            // Hash each pubkey
            let mut slab: NounSlab = NounSlab::new();
            let pubkey_noun = pubkey.to_noun(&mut slab);
            let pubkey_hash = Tip5Hasher::hash_noun(pubkey_noun)
                .unwrap_or_else(|_| Hash { values: [0; 5] });
            Hashable::Hash(pubkey_hash)
        });
        
        // Create the cell [leaf+m hashable-pubkeys]
        Hashable::cell(
            Hashable::leaf_u64(self.m),
            pubkeys_hashable
        )
    }
    
    pub fn to_hash(&self) -> Hash {
        hash_hashable(&self.to_hashable())
    }
    
    /// Convert base58 encoded multisig parameters to Lock
    /// Implements the Hoon from-b58 function for Lock:
    /// ++  from-b58
    ///   |=  [m=@ pks=(list @t)]
    ///   ^-  form
    ///   %-  check
    ///   %+  m-of-n:new  m
    ///   %-  ~(gas z-in *(z-set schnorr-pubkey))
    ///   %+  turn  pks
    ///   |=  pk=@t
    ///   (from-b58:schnorr-pubkey pk)
    pub fn from_b58(m: u64, pubkeys_b58: Vec<String>) -> Result<Self, String> {
        let mut pubkeys = ZSet::new();
        
        // Convert each base58 pubkey string to SchnorrPubkey and add to set
        for pk_str in pubkeys_b58 {
            let pubkey = SchnorrPubkey::from_b58(&pk_str)?;
            pubkeys.put(pubkey);
        }
        
        // The Hoon code calls 'check' which validates the lock
        // We should do the same validation here
        if m == 0 {
            return Err("Lock m value cannot be 0".to_string());
        }
        
        let pubkeys_len = pubkeys.len() as u64;
        if m > pubkeys_len {
            return Err(format!("Lock m value {} exceeds number of pubkeys {}", m, pubkeys_len));
        }
        
        // Create the lock with m-of-n multisig
        let lock = Lock { m, pubkeys };
        
        Ok(lock)
    }
    
    /// Convert Lock to base58 encoded representation
    /// Implements the Hoon to-b58 function:
    /// ++  to-b58
    ///   |=  loc=form
    ///   ^-  [m=@udD pks=(list @t)]
    ///   :-  m.loc
    ///   (turn ~(tap z-in pubkeys.loc) to-b58:schnorr-pubkey)
    pub fn to_b58(&self) -> (u64, Vec<String>) {
        let pubkeys_b58: Vec<String> = self.pubkeys
            .iter()
            .map(|pk| pk.to_b58())
            .collect();
        
        (self.m, pubkeys_b58)
    }
}

impl std::hash::Hash for Lock {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.m.hash(state);
        // Sort the pubkeys to ensure consistent hashing
        let mut sorted_pubkeys: Vec<_> = self.pubkeys.iter().collect();
        sorted_pubkeys.sort_by(|a, b| {
            a.x.values.cmp(&b.x.values)
                .then_with(|| a.y.values.cmp(&b.y.values))
                .then_with(|| a.inf.cmp(&b.inf))
        });
        for pubkey in sorted_pubkeys {
            pubkey.hash(state);
        }
    }
}

#[derive(Debug, Clone, NounEncode, NounDecode, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Source {
    pub p: Hash,
    pub is_coinbase: bool,
}

impl Source {
    pub fn to_hashable(&self) -> Hashable {
        // Source hashable based on Hoon implementation (tx-engine.hoon lines 275-279)
        // Source is [hash, is_coinbase]
        // Note: In Hoon, %.y (true) = 0 and %.n (false) = 1
        Hashable::cell(
            Hashable::Hash(self.p.clone()),
            Hashable::leaf_u64(if self.is_coinbase { 0 } else { 1 }),
        )
    }
    
    pub fn to_hash(&self) -> Hash {
        hash_hashable(&self.to_hashable())
    }
}

// Note structure
#[derive(Debug, Clone, NounEncode, NounDecode)]
pub struct NNote {
    pub meta: NNoteHead,
    pub name: NName,
    pub lock: Lock,
    pub source: Source,
    pub assets: Coins,
}

impl NNote {
    /// Convert the NNote to a hashable structure and hash it
    pub fn to_hash(&self) -> Hash {
        // Convert to hashable structure
        let hashable = self.to_hashable();
        
        // Hash the structure
        hash_hashable(&hashable)
    }
    
    pub fn to_hashable(&self) -> Hashable {
        // NNote hashable matches Hoon implementation in tx-engine.hoon lines 1462-1472
        // Structure: [[version origin-page timelock-hash] [name-hash lock-hash source-hash assets]]
        Hashable::cell(
            // First part: [version origin-page timelock-hash]
            Hashable::triple(
                Hashable::leaf_u64(self.meta.version),
                Hashable::leaf_u64(self.meta.origin_page.value),
                Hashable::Hash(hash_hashable(&self.meta.timelock.to_hashable())),
            ),
            // Second part: [name-hash lock-hash source-hash assets]
            // Note: In Hoon this is a quad (4-element structure)
            Hashable::cell(
                Hashable::Hash(hash_hashable(&self.name.to_hashable())),
                Hashable::cell(
                    Hashable::Hash(hash_hashable(&self.lock.to_hashable())),
                    Hashable::cell(
                        Hashable::Hash(hash_hashable(&self.source.to_hashable())),
                        Hashable::leaf_u64(self.assets.value),
                    ),
                ),
            ),
        )
    }
}

// Note structure
#[derive(Debug, Clone, NounEncode, NounDecode)]
pub struct NNoteHead {
    pub version: u64,
    pub origin_page: PageNumber,
    pub timelock: Timelock,
}

#[derive(Debug, Clone)]
pub struct T8 {
    pub values: [u64; 8],
}

#[derive(Debug, Clone, NounEncode, NounDecode)]
pub struct Chal {
    pub values: T8,
}

#[derive(Debug, Clone, NounEncode, NounDecode)]
pub struct Sig {
    pub values: T8,
}

// Schnorr signature components
#[derive(Debug, Clone, NounEncode, NounDecode)]
pub struct SchnorrSignature {
    pub chal: Chal,
    pub sig: Sig,
}

impl SchnorrSignature {
    pub fn to_hashable(&self) -> Hashable {
        // In Hoon, this is [%leaf form] where form is the signature noun.
        // Since our Hashable::Leaf only supports bytes (not arbitrary nouns),
        // and since the Hoon implementation ultimately hashes the noun anyway,
        // we pre-hash the noun and return Hashable::Hash.
        // This is semantically equivalent to Hoon's (hash [%leaf form]).
        let mut slab: NounSlab = NounSlab::new();
        let noun = self.to_noun(&mut slab);
        let hash = Tip5Hasher::hash_noun(noun)
            .unwrap_or_else(|_| Hash { values: [0; 5] });
        Hashable::Hash(hash)
    }
    
    pub fn to_hash(&self) -> Hash {
        // Since to_hashable returns Hash, extract it directly
        match self.to_hashable() {
            Hashable::Hash(h) => h,
            _ => hash_hashable(&self.to_hashable())
        }
    }
}
// signature structure
#[derive(Debug, Clone, NounEncode, NounDecode)]
pub struct Signature {
    pub map: ZMap<SchnorrPubkey, SchnorrSignature>,
}

impl Signature {
    pub fn to_hashable(&self) -> Hashable {
        // Signature is just a ZMap, so delegate to its to_hashable method
        // The ZMap will handle the tree traversal and structure
        self.map.to_hashable(
            // Function for keys (pubkeys) - convert to hashable
            |pubkey| pubkey.to_hashable(),
            // Function for values (signatures) - convert to hashable
            |sig| sig.to_hashable()
        )
    }
    
    pub fn to_hash(&self) -> Hash {
        // Since to_hashable already returns Hashable::Hash, we can optimize
        match self.to_hashable() {
            Hashable::Hash(h) => h,
            _ => hash_hashable(&self.to_hashable())
        }
    }
}

// Seed structure
#[derive(Debug, Clone, NounEncode, NounDecode, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Seed {
    pub output_source: Option<Source>,
    pub recipient: Lock,
    pub timelock_intent: TimelockIntent,
    pub gift: Coins,
    pub parent_hash: Hash,
}

impl Seed {
    pub fn to_hashable(&self) -> Hashable {
        // Seed hashable from Hoon (excluding output-source):
        // :^    (hashable:lock recipient.sed)
        //     (hashable:timelock-intent timelock-intent.sed)
        //   leaf+gift.sed
        // hash+parent-hash.sed
        
        // This is a 4-element structure (quad)
        // Using nested cells to represent it
        Hashable::cell(
            self.recipient.to_hashable(),
            Hashable::cell(
                to_hashable_timelock_intent(&self.timelock_intent),
                Hashable::cell(
                    Hashable::leaf_u64(self.gift.value),
                    Hashable::Hash(self.parent_hash.clone())
                )
            )
        )
    }
    
    pub fn to_hash(&self) -> Hash {
        hash_hashable(&self.to_hashable())
    }
}

// Seeds structure
#[derive(Debug, Clone, NounEncode, NounDecode)]
pub struct Seeds {
    pub set: ZSet<Seed>,
}

impl Seeds {
    pub fn to_hashable(&self) -> Hashable {
        // Seeds is a z-set of Seed
        // From Hoon:
        // ?~  form  leaf+form
        // :+  (hashable:seed n.form)
        //   $(form l.form)
        // $(form r.form)
        
        // Use ZSet's to_hashable method which properly traverses the tree
        self.set.to_hashable(|seed| seed.to_hashable())
    }
    
    pub fn to_hash(&self) -> Hash {
        hash_hashable(&self.to_hashable())
    }
}

// Spend structure
#[derive(Debug, Clone, NounEncode, NounDecode)]
pub struct Spend {
    pub signature: Option<Signature>,
    pub seeds: Seeds,
    pub fee: Coins,
}

impl Spend {
    pub fn to_hashable(&self) -> Hashable {
        // Spend hashable matches Hoon implementation in tx-engine.hoon lines 1987-1992
        // Structure: [signature-hashable seeds-hashable leaf+fee]
        Hashable::triple(
            // Signature handling: if None -> leaf+~, if Some -> [leaf+~ signature-hashable]
            match &self.signature {
                None => Hashable::null(),
                Some(sig) => Hashable::cell(
                    Hashable::null(),
                    sig.to_hashable(),
                ),
            },
            self.seeds.to_hashable(),
            Hashable::leaf_u64(self.fee.value),
        )
    }
    
    pub fn to_hash(&self) -> Hash {
        hash_hashable(&self.to_hashable())
    }
}

// Input structure  
#[derive(Debug, Clone, NounEncode, NounDecode)]
pub struct Input {
    pub note: NNote,
    pub spend: Spend,
}

impl Input {
    pub fn to_hashable(&self) -> Hashable {
        // Input hashable is a cell of [nnote-hashable, spend-hashable]
        // This matches the Hoon implementation in tx-engine.hoon lines 2105-2109
        Hashable::cell(
            self.note.to_hashable(),
            self.spend.to_hashable(),
        )
    }
    
    pub fn to_hash(&self) -> Hash {
        hash_hashable(&self.to_hashable())
    }
    
    /// Calculate the effective timelock range for this input
    /// 
    /// Combines absolute and relative timelocks from the note with the note's origin page.
    /// Returns (min, max) as Option<u64> values representing the valid spending window.
    pub fn calculate_timelock_range(&self) -> (Option<u64>, Option<u64>) {
        let origin_page = self.note.meta.origin_page.value;
        let timelock_intent = &self.note.meta.timelock.intent;
        
        if let Some((absolute, relative)) = timelock_intent {
            self.calculate_input_timelock_range(
                origin_page,
                &Some(absolute.clone()),
                &Some(relative.clone()),
            )
        } else {
            (None, None)
        }
    }
    
    /// Helper method to calculate timelock range from absolute and relative timelocks
    fn calculate_input_timelock_range(
        &self,
        origin_page: u64,
        absolute: &Option<TimelockRange>,
        relative: &Option<TimelockRange>,
    ) -> (Option<u64>, Option<u64>) {
        let mut min = None;
        let mut max = None;
        
        // Process absolute timelock
        if let Some(abs) = absolute {
            min = abs.min.as_ref().map(|p| p.value);
            max = abs.max.as_ref().map(|p| p.value);
        }
        
        // Process relative timelock (add to origin_page)
        if let Some(rel) = relative {
            let rel_min = rel.min.as_ref().map(|p| origin_page + p.value);
            let rel_max = rel.max.as_ref().map(|p| origin_page + p.value);
            
            // Combine with absolute (intersection - most restrictive)
            min = match (min, rel_min) {
                (None, rm) => rm,
                (am, None) => am,
                (Some(am), Some(rm)) => Some(am.max(rm)), // Most restrictive min
            };
            
            max = match (max, rel_max) {
                (None, rm) => rm,
                (am, None) => am,
                (Some(am), Some(rm)) => Some(am.min(rm)), // Most restrictive max
            };
        }
        
        (min, max)
    }
}

// Inputs structure using ZMap to match Hoon's z-map
#[derive(Debug, Clone, NounEncode, NounDecode)]
pub struct Inputs {
    pub p: ZMap<NName, Input>,
}

impl Inputs {
    /// Convert Inputs to a Hashable representation
    /// This delegates to the ZMap's to_hashable method for proper tree traversal
    pub fn to_hashable(&self) -> Hashable {
        self.p.to_hashable(
            |name| name.to_hashable(),
            |input| input.to_hashable(),
        )
    }
    
    /// Hash the Inputs structure
    pub fn to_hash(&self) -> Hash {
        hash_hashable(&self.to_hashable())
    }
}

// Hash wrapper for transaction IDs and other hashes
#[derive(Debug, Clone, NounEncode, NounDecode)]
pub struct Transaction {
    pub name: String,
    pub p: Inputs
}

// Raw transaction structure matching Hoon raw-tx form
// ++  raw-tx
//   $:  id=tx-id  :: hash of +.raw-tx
//       =inputs
//       =timelock-range
//       total-fees=coins
//   ==
#[derive(Debug, Clone, NounEncode, NounDecode)]
pub struct RawTransaction {
    pub id: Hash,                       // tx-id: hash of the transaction
    pub inputs: Inputs,                 // inputs map
    pub timelock_range: TimelockRange,  // union of valid page-number ranges
    pub total_fees: Coins,              // sum of all fees paid by all inputs
}

impl NounDecode for T8 {
    fn from_noun<A: NounAllocator>(_: &mut A, noun: &Noun) -> Result<Self, NounDecodeError> {
        let mut ret: [u64; 8] = [0; 8];
        let mut cur = *noun;
        for i in 0..7 {
            let cur_cell = cur.as_cell().map_err(|_| NounDecodeError::ExpectedCell)?;
            ret[i] = cur_cell
                .head()
                .as_atom()
                .map_err(|_| NounDecodeError::ExpectedAtom)?
                .as_u64()?;
            cur = cur_cell.tail();
        }
        ret[7] = cur
            .as_atom()
            .map_err(|_| NounDecodeError::ExpectedAtom)?
            .as_u64()?;
        Ok(T8 { values: ret })
    }
}

impl NounEncode for T8 {
    fn to_noun<A: NounAllocator>(&self, alloc: &mut A) -> Noun {
        let mut res_cell = Atom::new(alloc, self.values[7]).as_noun();
        for i in (0..=6).rev() {
            let b = Atom::new(alloc, self.values[i]).as_noun();
            res_cell = T(alloc, &[b, res_cell]);
        }
        res_cell
    }
}

#[cfg(test)]
mod tests {
    use nockvm::{mem::NockStack, noun::FullDebugCell};
    use crate::collections::ZSet;

    use super::*;

    #[test]
    fn test_hash_encoding(){
        let mut stack = NockStack::new(8 << 10 << 10, 0);

        let hash = Hash { values: [0x1234; 5] };
        let encoded = hash.to_noun(&mut stack);
        //let decoded : Hash = Hash::from_noun(&mut stack, &encoded).unwrap();
        println!("Encoded: {:?}", FullDebugCell(&encoded.as_cell().unwrap()));
    }

    #[test]
    fn test_hash_to_ubig() {
        use ibig::UBig;
        
        // Test with a simple hash
        let hash1 = Hash { values: [1, 2, 3, 4, 5] };
        let big1 = hash1.to_ubig();
        
        // Convert back and verify
        let hash1_back = Hash::from_ubig(&big1).unwrap();
        assert_eq!(hash1.values, hash1_back.values);
        
        // Test with larger values
        let hash2 = Hash { values: [u64::MAX, u64::MAX-1, u64::MAX-2, u64::MAX-3, u64::MAX-4] };
        let big2 = hash2.to_ubig();
        let hash2_back = Hash::from_ubig(&big2).unwrap();
        assert_eq!(hash2.values, hash2_back.values);
        
        // Test with zeros
        let hash3 = Hash { values: [0, 0, 0, 0, 0] };
        let big3 = hash3.to_ubig();
        assert_eq!(big3, UBig::from(0u64));
        let hash3_back = Hash::from_ubig(&big3).unwrap();
        assert_eq!(hash3.values, hash3_back.values);
        
        // Test comparison using UBig
        let hash_a = Hash { values: [100, 0, 0, 0, 0] };
        let hash_b = Hash { values: [99, 0, 0, 0, 0] };
        let big_a = hash_a.to_ubig();
        let big_b = hash_b.to_ubig();
        assert!(big_a > big_b);
        
        // Test with most significant bit differences
        let hash_c = Hash { values: [0, 0, 0, 0, 1] };
        let hash_d = Hash { values: [u64::MAX, u64::MAX, u64::MAX, u64::MAX, 0] };
        let big_c = hash_c.to_ubig();
        let big_d = hash_d.to_ubig();
        assert!(big_c > big_d); // MSB difference should dominate
        
        println!("Hash to UBig conversion tests passed!");
    }

    #[test]
    fn test_raw_transaction_encoding() {
        let mut stack = NockStack::new(8 << 10 << 10, 0);

        // Create test data for RawTransaction
        let tx_id = Hash { values: [0x1111, 0x2222, 0x3333, 0x4444, 0x5555] };
        
        // Create a simple input
        let name = NName {
            p: vec![Hash { values: [1, 2, 3, 4, 5] }],
        };
        
        let pubkey = SchnorrPubkey {
            x: F6LT { values: [1, 2, 3, 4, 5, 0] },
            y: F6LT { values: [6, 7, 8, 9, 10, 0] },
            inf: false,
        };
        
        let mut pubkeys = ZSet::new();
        pubkeys.put(pubkey);
        let lock = Lock { m: 1, pubkeys };
        
        let source = Source {
            p: Hash { values: [10, 20, 30, 40, 50] },
            is_coinbase: false,
        };
        
        let note = NNote {
            meta: NNoteHead {
                version: 1,
                origin_page: PageNumber { value: 100 },
                timelock: Timelock {
                    intent: Some((
                        TimelockRange {
                            min: Some(PageNumber { value: 100 }),
                            max: Some(PageNumber { value: 200 }),
                        },
                        TimelockRange {
                            min: None,
                            max: None,
                        },
                    )),
                },
            },
            name: name.clone(),
            lock: lock.clone(),
            source: source.clone(),
            assets: Coins { value: 1000 },
        };
        
        let seed = Seed {
            output_source: Some(source),
            recipient: lock,
            timelock_intent: None,
            gift: Coins { value: 100 },
            parent_hash: Hash { values: [5, 4, 3, 2, 1] },
        };
        
        let mut seed_set = ZSet::new();
        seed_set.put(seed);
        
        let spend = Spend {
            signature: None,
            seeds: Seeds { set: seed_set },
            fee: Coins { value: 10 },
        };
        
        let input = Input { note, spend };
        
        let mut input_map = ZMap::new();
        input_map.put(name, input);
        let inputs = Inputs { p: input_map };
        
        // Create RawTransaction
        let raw_tx = RawTransaction {
            id: tx_id,
            inputs,
            timelock_range: TimelockRange {
                min: Some(PageNumber { value: 100 }),
                max: Some(PageNumber { value: 200 }),
            },
            total_fees: Coins { value: 10 },
        };
        
        // Encode to noun
        let encoded = raw_tx.to_noun(&mut stack);
        println!("RawTransaction encoded successfully");
        
        // Decode back
        let decoded: RawTransaction = RawTransaction::from_noun(&mut stack, &encoded)
            .expect("Should decode RawTransaction");
        
        // Verify fields
        assert_eq!(decoded.id.values, [0x1111, 0x2222, 0x3333, 0x4444, 0x5555]);
        assert_eq!(decoded.total_fees.value, 10);
        assert_eq!(decoded.timelock_range.min.unwrap().value, 100);
        assert_eq!(decoded.timelock_range.max.unwrap().value, 200);
        assert_eq!(decoded.inputs.p.wyt(), 1);
        
        println!("RawTransaction test passed!");
    }

    #[test]
    fn test_transaction_encoding() {
        let mut stack = NockStack::new(8 << 10 << 10, 0);

        // Create some test data
        let hash = Hash { values: [0x1234, 0x5678, 0x9abc, 0xdef0, 0x1111] };
        let page_number = PageNumber { value: 42 };
        let coins = Coins { value: 1000 };

        // Create a SchnorrPubkey
        let pubkey = SchnorrPubkey {
            x: F6LT { values: [1, 2, 3, 4, 5, 0] },
            y: F6LT { values: [6, 7, 8, 9, 10, 0] },
            inf: false,
        };

        // Create a Lock with the pubkey
        let mut pubkeys = ZSet::new();
        pubkeys.put(pubkey.clone());
        let lock = Lock {
            m: 1,
            pubkeys,
        };

        // Create Source
        let source = Source {
            p: hash.clone(),
            is_coinbase: false,
        };

        // Create Timelock structures
        let timelock_range = TimelockRange {
            min: Some(page_number.clone()),
            max: Some(PageNumber { value: 100 }),
        };
        let timelock_intent: TimelockIntent = Some((
            timelock_range,
            TimelockRange { min: None, max: None },
        ));
        let timelock = Timelock {
            intent: timelock_intent.clone(),
        };

        // Create NName
        let name = NName {
            p: vec![hash.clone(), Hash { values: [0x2222, 0x3333, 0x4444, 0x5555, 0x6666] }],
        };

        // Create NNoteHead
        let note_head = NNoteHead {
            version: 1,
            origin_page: page_number,
            timelock,
        };

        // Create NNote
        let note = NNote {
            meta: note_head,
            name: name.clone(),
            lock: lock.clone(),
            source: source.clone(),
            assets: coins.clone(),
        };

        // Create Seed
        let seed = Seed {
            output_source: Some(source),
            recipient: lock,
            timelock_intent,
            gift: coins.clone(),
            parent_hash: hash,
        };

        // Create Seeds
        let mut seed_set = ZSet::new();
        seed_set.put(seed);
        let seeds = Seeds {
            set: seed_set,
        };

        // Create Spend
        let spend = Spend {
            signature: None, // Simplified - no signature for this test
            seeds,
            fee: Coins { value: 10 },
        };

        // Create Input
        let input = Input {
            note,
            spend,
        };

        // Create Inputs
        let mut input_map = ZMap::new();
        input_map.put(name, input);
        let inputs = Inputs {
            p: input_map,
        };

        // Create Transaction
        let transaction = Transaction {
            name: "test_transaction".to_string(),
            p: inputs,
        };

        // Encode to noun
        let encoded = transaction.to_noun(&mut stack);
        println!("Transaction encoded: {:?}", FullDebugCell(&encoded.as_cell().unwrap()));

        // Test that we can decode it back
        let decoded: Transaction = Transaction::from_noun(&mut stack, &encoded).unwrap();
        println!("Transaction name: {}", decoded.name);
        println!("Number of inputs: {}", decoded.p.p.wyt());
    }
}

