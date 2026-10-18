//! Hostile-jam and non-minimal-atom coverage for the pokenoun boundary that
//! the device/wasm signers decode untrusted drafts with (audit findings 7/8).
//!
//! These craft jam bytes directly instead of using `pokenoun::jam`, because
//! the encoder emits no backreferences and always minimal atom sizes.

use pokenoun::{canonical_zmap_put, cue, hash_noun_varlen, Arena, CodecError, Noun, Tip5Error};

struct Bits {
    bytes: Vec<u8>,
    bit: usize,
}

impl Bits {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            bit: 0,
        }
    }

    fn pos(&self) -> u64 {
        self.bit as u64
    }

    fn push(&mut self, bit: bool) {
        let byte = self.bit / 8;
        if byte == self.bytes.len() {
            self.bytes.push(0);
        }
        if bit {
            self.bytes[byte] |= 1 << (self.bit % 8);
        }
        self.bit += 1;
    }

    fn push_bits(&mut self, value: u64, count: usize) {
        for i in 0..count {
            self.push((value >> i) & 1 == 1);
        }
    }

    /// Jam `get_size` header for an atom/backref size.
    fn push_size(&mut self, size: u64) {
        if size == 0 {
            self.push(true);
            return;
        }
        let width = 64 - size.leading_zeros() as usize; // met0
        for _ in 0..width {
            self.push(false);
        }
        self.push(true);
        if width > 1 {
            self.push_bits(size, width - 1);
        }
    }

    fn push_atom_bits(&mut self, atom_bits: usize, bytes: &[u8]) {
        self.push(false); // atom tag
        self.push_size(atom_bits as u64);
        for i in 0..atom_bits {
            self.push((bytes[i / 8] >> (i % 8)) & 1 == 1);
        }
    }

    fn push_backref(&mut self, target: u64) {
        // `11`, then a `get_size` header for the *bit length* of the target
        // offset, then that many offset bits.
        self.push(true);
        self.push(true);
        let width = if target == 0 {
            0
        } else {
            64 - target.leading_zeros() as usize
        };
        self.push_size(width as u64);
        self.push_bits(target, width);
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

fn atom_bits(bytes: &[u8]) -> usize {
    let mut len = bytes.len();
    while len > 0 && bytes[len - 1] == 0 {
        len -= 1;
    }
    if len == 0 {
        0
    } else {
        (len - 1) * 8 + (8 - bytes[len - 1].leading_zeros() as usize)
    }
}

#[test]
fn truncated_jams_error() {
    let mut bits = Bits::new();
    bits.push_atom_bits(atom_bits(b"hello"), b"hello");
    let bytes = bits.into_bytes();
    assert!(cue(&bytes[..bytes.len() - 1], &mut Arena::new()).is_err());

    // Truncated cell: `10` tag with no children.
    let mut bits = Bits::new();
    bits.push(true);
    bits.push(false);
    let bytes = bits.into_bytes();
    assert_eq!(
        cue(&bytes, &mut Arena::new()),
        Err(CodecError::UnexpectedEof)
    );
}

#[test]
fn oversized_atom_headers_error_without_allocating() {
    // Declare a 2^62-bit atom while supplying no payload.
    let mut bits = Bits::new();
    bits.push(false); // atom
    for _ in 0..63 {
        bits.push(false);
    }
    bits.push(true);
    let bytes = bits.into_bytes();
    assert_eq!(
        cue(&bytes, &mut Arena::new()),
        Err(CodecError::UnexpectedEof)
    );

    // More than 64 leading zero bits is rejected as invalid, not shifted.
    let mut bits = Bits::new();
    bits.push(false);
    for _ in 0..65 {
        bits.push(false);
    }
    let bytes = bits.into_bytes();
    assert_eq!(
        cue(&bytes, &mut Arena::new()),
        Err(CodecError::InvalidEncoding)
    );
}

#[test]
fn shared_dag_jam_hits_walk_budget_instead_of_exploding() {
    // 20 nested cells, each encoding a child once and then referencing it
    // again via a backref. The jam stays tiny while the logical tree has
    // 2^21 - 1 nodes.
    fn encode_level(bits: &mut Bits, level: usize) -> u64 {
        let start = bits.pos();
        bits.push(true);
        bits.push(false); // cell
        let child_start = if level == 1 {
            let atom_start = bits.pos();
            bits.push_atom_bits(1, &[1]);
            atom_start
        } else {
            encode_level(bits, level - 1)
        };
        bits.push_backref(child_start);
        start
    }

    let mut bits = Bits::new();
    assert_eq!(encode_level(&mut bits, 20), 0);
    let bytes = bits.into_bytes();

    let mut arena = Arena::new();
    let root = cue(&bytes, &mut arena).expect("shared-DAG jam is structurally valid");
    fn count(n: Noun, arena: &Arena, budget: usize) -> usize {
        let mut stack = vec![n];
        let mut seen = 0;
        while let Some(n) = stack.pop() {
            seen += 1;
            if seen > budget { return seen; }
            if let Noun::Cell(id) = n {
                let c = arena.cell(id);
                stack.push(c.head);
                stack.push(c.tail);
            }
        }
        seen
    }
    assert_eq!(count(root, &arena, 4 << 20), (1usize << 21) - 1);
    assert_eq!(
        hash_noun_varlen(root, &arena),
        Err(Tip5Error::WalkTooLarge)
    );
}

#[test]
fn non_minimal_atoms_compare_by_value() {
    // Atom `%pkh` encoded with a trailing zero byte (size 32, non-minimal
    // spelling). `cue` preserves the spelling, comparisons must not.
    let mut bits = Bits::new();
    bits.push_atom_bits(32, b"pkh\0");
    let bytes = bits.into_bytes();

    let mut arena = Arena::new();
    let root = cue(&bytes, &mut arena).expect("non-minimal atom cue");
    let Noun::Atom(id) = root else {
        panic!("expected atom");
    };
    assert_eq!(arena.atom_bytes(id).len(), 4, "spelling is preserved");
    assert!(arena.atom_eq_bytes(id, b"pkh"));
    assert_eq!(arena.atom_u64(id), Some(0x0068_6b70));

    let minimal = arena.alloc_atom_bytes(b"pkh");
    assert_eq!(
        hash_noun_varlen(root, &arena).expect("non-minimal hash"),
        hash_noun_varlen(minimal, &arena).expect("minimal hash"),
    );

    // The same key in a z-map must replace, not duplicate.
    let null = arena.atom0();
    let v1 = arena.alloc_atom_u64(1);
    let v2 = arena.alloc_atom_u64(2);
    let map = canonical_zmap_put(&mut arena, null, root, v1).expect("put non-minimal key");
    let map = canonical_zmap_put(&mut arena, map, minimal, v2).expect("replace minimal key");

    let mut expected_arena = Arena::new();
    let expected_null = expected_arena.atom0();
    let expected_key = expected_arena.alloc_atom_bytes(b"pkh");
    let expected_value = expected_arena.alloc_atom_u64(2);
    let expected =
        canonical_zmap_put(&mut expected_arena, expected_null, expected_key, expected_value)
            .expect("expected map");

    assert_eq!(
        hash_noun_varlen(map, &arena).expect("map hash"),
        hash_noun_varlen(expected, &expected_arena).expect("expected map hash"),
        "non-minimal key must canonicalize to one z-map entry"
    );
}
