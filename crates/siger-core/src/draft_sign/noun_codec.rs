extern crate alloc;

use alloc::vec::Vec;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Noun {
    Atom(AtomId),
    Cell(CellId),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct AtomId(u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CellId(u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub head: Noun,
    pub tail: Noun,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct AtomSlice {
    off: u32,
    len: u32,
}

#[derive(Debug, Default)]
pub struct Arena {
    atoms: Vec<AtomSlice>,
    atom_data: Vec<u8>,
    cells: Vec<Cell>,
}

impl Arena {
    pub fn new() -> Self {
        // AtomId(0) is always the canonical `0` atom with an empty byte slice.
        let mut atoms = Vec::new();
        atoms.push(AtomSlice { off: 0, len: 0 });
        Self {
            atoms,
            atom_data: Vec::new(),
            cells: Vec::new(),
        }
    }

    #[inline]
    pub fn atom0(&self) -> Noun {
        Noun::Atom(AtomId(0))
    }

    #[inline]
    pub fn atom_bytes(&self, id: AtomId) -> &[u8] {
        let s = self.atoms[id.0 as usize];
        let start = s.off as usize;
        let end = start + (s.len as usize);
        &self.atom_data[start..end]
    }

    #[inline]
    pub fn atom_u64(&self, id: AtomId) -> Option<u64> {
        let bytes = self.atom_bytes(id);
        if bytes.len() > 8 {
            return None;
        }
        let mut out = 0u64;
        for (i, &b) in bytes.iter().enumerate() {
            out |= (b as u64) << (8 * i);
        }
        Some(out)
    }

    #[inline]
    pub fn atom_eq_bytes(&self, id: AtomId, bytes: &[u8]) -> bool {
        self.atom_bytes(id) == bytes
    }

    pub fn alloc_atom_bytes(&mut self, bytes: &[u8]) -> Noun {
        let mut len = bytes.len();
        while len > 0 && bytes[len - 1] == 0 {
            len -= 1;
        }
        if len == 0 {
            return self.atom0();
        }

        let off = self.atom_data.len();
        self.atom_data.extend_from_slice(&bytes[..len]);
        let id = self.atoms.len();
        self.atoms.push(AtomSlice {
            off: off as u32,
            len: len as u32,
        });
        Noun::Atom(AtomId(id as u32))
    }

    pub fn alloc_atom_u64(&mut self, v: u64) -> Noun {
        if v == 0 {
            return self.atom0();
        }
        let bytes = v.to_le_bytes();
        let mut len = 8usize;
        while len > 0 && bytes[len - 1] == 0 {
            len -= 1;
        }
        self.alloc_atom_bytes(&bytes[..len])
    }

    pub fn alloc_cell(&mut self, head: Noun, tail: Noun) -> Noun {
        let id = self.cells.len();
        self.cells.push(Cell { head, tail });
        Noun::Cell(CellId(id as u32))
    }

    pub fn cell(&self, id: CellId) -> &Cell {
        &self.cells[id.0 as usize]
    }

    pub fn cell_mut(&mut self, id: CellId) -> &mut Cell {
        &mut self.cells[id.0 as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    UnexpectedEof,
    AtomTooLarge,
    InvalidBackref,
    InvalidEncoding,
}

struct BitReader<'a> {
    bytes: &'a [u8],
    bit: usize,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, bit: 0 }
    }

    fn pos(&self) -> u64 {
        self.bit as u64
    }

    fn read_bit(&mut self) -> Result<bool, CodecError> {
        let byte_i = self.bit / 8;
        if byte_i >= self.bytes.len() {
            return Err(CodecError::UnexpectedEof);
        }
        let bit_i = self.bit % 8;
        let b = (self.bytes[byte_i] >> bit_i) & 1;
        self.bit += 1;
        Ok(b == 1)
    }

    fn read_bits_u64(&mut self, n: usize) -> Result<u64, CodecError> {
        if n > 64 {
            return Err(CodecError::AtomTooLarge);
        }
        let mut out = 0u64;
        for i in 0..n {
            if self.read_bit()? {
                out |= 1u64 << i;
            }
        }
        Ok(out)
    }
}

fn get_size(reader: &mut BitReader<'_>) -> Result<usize, CodecError> {
    let mut zeros = 0usize;
    loop {
        let b = reader.read_bit()?;
        if b {
            break;
        }
        zeros += 1;
        if zeros > 64 {
            return Err(CodecError::InvalidEncoding);
        }
    }
    if zeros == 0 {
        return Ok(0);
    }
    let bitsize = zeros;
    let low = reader.read_bits_u64(bitsize.saturating_sub(1))? as usize;
    Ok(low + (1usize << (bitsize - 1)))
}

fn rub_backref(reader: &mut BitReader<'_>) -> Result<u64, CodecError> {
    let size = get_size(reader)?;
    if size == 0 {
        return Ok(0);
    }
    if size > 64 {
        return Err(CodecError::InvalidBackref);
    }
    reader.read_bits_u64(size)
}

fn met0_u64(x: u64) -> usize {
    if x == 0 {
        0
    } else {
        64 - (x.leading_zeros() as usize)
    }
}

fn lookup_backref(backrefs: &[(u64, Noun)], key: u64) -> Option<Noun> {
    backrefs
        .binary_search_by(|(k, _)| k.cmp(&key))
        .ok()
        .map(|idx| backrefs[idx].1)
}

enum Slot {
    Root,
    Head(CellId),
    Tail(CellId),
}

/// Decode a jammed noun (little-endian bytes, LSB-first bits) into an in-memory noun tree.
pub fn cue(bytes: &[u8], arena: &mut Arena) -> Result<Noun, CodecError> {
    let mut reader = BitReader::new(bytes);
    let mut backrefs: Vec<(u64, Noun)> = Vec::new();
    let mut root: Option<Noun> = None;

    let mut stack: Vec<Slot> = Vec::new();
    stack.push(Slot::Root);

    while let Some(slot) = stack.pop() {
        let start = reader.pos();
        let first = reader.read_bit()?;
        if !first {
            let size = get_size(&mut reader)?;
            let noun = if size == 0 {
                arena.atom0()
            } else {
                let nbytes = (size + 7) / 8;
                let off = arena.atom_data.len();
                arena.atom_data.resize(off + nbytes, 0u8);
                for i in 0..size {
                    if reader.read_bit()? {
                        arena.atom_data[off + (i / 8)] |= 1u8 << (i % 8);
                    }
                }
                let id = arena.atoms.len();
                arena.atoms.push(AtomSlice {
                    off: off as u32,
                    len: nbytes as u32,
                });
                Noun::Atom(AtomId(id as u32))
            };
            backrefs.push((start, noun));
            match slot {
                Slot::Root => root = Some(noun),
                Slot::Head(id) => arena.cell_mut(id).head = noun,
                Slot::Tail(id) => arena.cell_mut(id).tail = noun,
            }
            continue;
        }

        let second = reader.read_bit()?;
        if second {
            let backref = rub_backref(&mut reader)?;
            let noun = lookup_backref(&backrefs, backref).ok_or(CodecError::InvalidBackref)?;
            match slot {
                Slot::Root => root = Some(noun),
                Slot::Head(id) => arena.cell_mut(id).head = noun,
                Slot::Tail(id) => arena.cell_mut(id).tail = noun,
            }
            continue;
        }

        // cell (10)
        let cell_noun = arena.alloc_cell(arena.atom0(), arena.atom0());
        backrefs.push((start, cell_noun));
        match slot {
            Slot::Root => root = Some(cell_noun),
            Slot::Head(id) => arena.cell_mut(id).head = cell_noun,
            Slot::Tail(id) => arena.cell_mut(id).tail = cell_noun,
        }
        let Noun::Cell(cell_id) = cell_noun else {
            unreachable!();
        };

        // Decode head then tail; stack is LIFO so push tail first.
        stack.push(Slot::Tail(cell_id));
        stack.push(Slot::Head(cell_id));
    }

    root.ok_or(CodecError::InvalidEncoding)
}

struct BitWriter {
    bytes: Vec<u8>,
    bit: usize,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            bit: 0,
        }
    }

    fn write_bit(&mut self, bit: bool) {
        let byte_i = self.bit / 8;
        if byte_i == self.bytes.len() {
            self.bytes.push(0);
        }
        let bit_i = self.bit % 8;
        if bit {
            self.bytes[byte_i] |= 1u8 << bit_i;
        }
        self.bit += 1;
    }

    fn write_bits_u64(&mut self, mut value: u64, n: usize) {
        for _ in 0..n {
            self.write_bit((value & 1) == 1);
            value >>= 1;
        }
    }
}

fn met0_bytes(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        return 0;
    }
    let last = bytes[bytes.len() - 1];
    let bits = 8 - (last.leading_zeros() as usize);
    (bytes.len() - 1) * 8 + bits
}

fn mat(writer: &mut BitWriter, atom_bits: usize, atom_bytes: &[u8]) -> Result<(), CodecError> {
    let b_atom_size = atom_bits;
    if b_atom_size == 0 {
        writer.write_bit(true);
        return Ok(());
    }

    let b_atom_size_atom: u64 = b_atom_size
        .try_into()
        .map_err(|_| CodecError::AtomTooLarge)?;
    let c_b_size = met0_u64(b_atom_size_atom);

    // c_b_size zeros, then a 1
    for _ in 0..c_b_size {
        writer.write_bit(false);
    }
    writer.write_bit(true);

    // size bits (excluding MSB)
    if c_b_size > 1 {
        writer.write_bits_u64(b_atom_size_atom, c_b_size - 1);
    }

    // atom bits
    for i in 0..b_atom_size {
        let bit = (atom_bytes[i / 8] >> (i % 8)) & 1;
        writer.write_bit(bit == 1);
    }
    Ok(())
}

fn jam_noun(writer: &mut BitWriter, arena: &Arena, noun: Noun) -> Result<(), CodecError> {
    let mut stack: Vec<Noun> = Vec::new();
    stack.push(noun);
    while let Some(n) = stack.pop() {
        match n {
            Noun::Atom(id) => {
                writer.write_bit(false); // 0 tag
                let bytes = arena.atom_bytes(id);
                let bits = met0_bytes(bytes);
                mat(writer, bits, bytes)?;
            }
            Noun::Cell(id) => {
                writer.write_bit(true);
                writer.write_bit(false); // 10 tag
                let cell = arena.cell(id);
                // DFS: head then tail => push tail first.
                stack.push(cell.tail);
                stack.push(cell.head);
            }
        }
    }
    Ok(())
}

/// Encode a noun tree into jam bytes (little-endian bytes, LSB-first bits).
///
/// This encoder currently emits no backreferences; output is still valid jam.
pub fn jam(noun: Noun, arena: &Arena) -> Vec<u8> {
    let mut writer = BitWriter::new();
    jam_noun(&mut writer, arena, noun).expect("jam");
    writer.bytes
}
