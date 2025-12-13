use crate::draft_sign::noun_codec::{Arena, Noun};
use crate::draft_sign::tip5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZMapError {
    Tip5(tip5::Tip5Error),
    Malformed,
}

impl From<tip5::Tip5Error> for ZMapError {
    fn from(e: tip5::Tip5Error) -> Self {
        Self::Tip5(e)
    }
}

fn noun_is_null(noun: Noun, arena: &Arena) -> bool {
    matches!(noun, Noun::Atom(id) if arena.atom_u64(id) == Some(0))
}

fn nouns_equal(a: Noun, b: Noun, arena: &Arena) -> bool {
    match (a, b) {
        (Noun::Atom(x), Noun::Atom(y)) => arena.atom_bytes(x) == arena.atom_bytes(y),
        (Noun::Cell(ac), Noun::Cell(bc)) => {
            let a_cell = arena.cell(ac);
            let b_cell = arena.cell(bc);
            nouns_equal(a_cell.head, b_cell.head, arena) && nouns_equal(a_cell.tail, b_cell.tail, arena)
        }
        _ => false,
    }
}

fn atom_less_than(a: Noun, b: Noun, arena: &Arena) -> bool {
    let (Noun::Atom(a), Noun::Atom(b)) = (a, b) else {
        return false;
    };
    let ab = arena.atom_bytes(a);
    let bb = arena.atom_bytes(b);
    if ab.len() != bb.len() {
        return ab.len() < bb.len();
    }
    for i in (0..ab.len()).rev() {
        if ab[i] != bb[i] {
            return ab[i] < bb[i];
        }
    }
    false
}

fn dor_tip_compare(a: Noun, b: Noun, arena: &Arena) -> bool {
    if nouns_equal(a, b, arena) {
        return true;
    }

    match (a, b) {
        (Noun::Cell(ac), Noun::Cell(bc)) => {
            let ac = arena.cell(ac);
            let bc = arena.cell(bc);
            if nouns_equal(ac.head, bc.head, arena) {
                dor_tip_compare(ac.tail, bc.tail, arena)
            } else {
                dor_tip_compare(ac.head, bc.head, arena)
            }
        }
        (Noun::Atom(_), Noun::Cell(_)) => false,
        (Noun::Cell(_), Noun::Atom(_)) => false,
        (Noun::Atom(_), Noun::Atom(_)) => atom_less_than(a, b, arena),
    }
}

fn less_than_hash(a: &[u64; 5], b: &[u64; 5]) -> bool {
    // Compare digits in base P from most significant to least significant.
    for i in (0..5).rev() {
        if a[i] < b[i] {
            return true;
        }
        if a[i] > b[i] {
            return false;
        }
    }
    false
}

fn tip_hash(noun: Noun, arena: &Arena) -> Result<[u64; 5], ZMapError> {
    Ok(tip5::hash_noun_varlen(noun, arena)?)
}

fn double_tip_hash(noun: Noun, arena: &Arena) -> Result<[u64; 5], ZMapError> {
    let tip = tip_hash(noun, arena)?;
    Ok(tip5::hash_ten_cell(tip, tip)?)
}

fn gor_tip_compare(a: Noun, b: Noun, arena: &Arena) -> Result<bool, ZMapError> {
    let a_tip = tip_hash(a, arena)?;
    let b_tip = tip_hash(b, arena)?;
    if a_tip == b_tip {
        Ok(dor_tip_compare(a, b, arena))
    } else {
        Ok(less_than_hash(&a_tip, &b_tip))
    }
}

fn mor_tip_compare(a: Noun, b: Noun, arena: &Arena) -> Result<bool, ZMapError> {
    let a_tip = double_tip_hash(a, arena)?;
    let b_tip = double_tip_hash(b, arena)?;
    if a_tip == b_tip {
        Ok(dor_tip_compare(a, b, arena))
    } else {
        Ok(less_than_hash(&a_tip, &b_tip))
    }
}

fn tuple(arena: &mut Arena, elems: &[Noun]) -> Result<Noun, ZMapError> {
    if elems.is_empty() {
        return Ok(arena.atom0());
    }
    let mut res = *elems.last().unwrap();
    for &n in elems[..elems.len() - 1].iter().rev() {
        res = arena.alloc_cell(n, res);
    }
    Ok(res)
}

fn decompose_map(map: Noun, arena: &Arena) -> Result<(Noun, Noun, Noun), ZMapError> {
    let Noun::Cell(id) = map else {
        return Err(ZMapError::Malformed);
    };
    let cell = arena.cell(id);
    let node = cell.head;
    let tail = cell.tail;

    if let Noun::Cell(children_id) = tail {
        let children = arena.cell(children_id);
        Ok((node, children.head, children.tail))
    } else {
        Ok((node, tail, arena.atom0()))
    }
}

fn decompose_pair(pair: Noun, arena: &Arena) -> Result<(Noun, Noun), ZMapError> {
    let Noun::Cell(id) = pair else {
        return Err(ZMapError::Malformed);
    };
    let cell = arena.cell(id);
    Ok((cell.head, cell.tail))
}

/// Insert/replace a key/value pair into a canonical z-map noun.
///
/// This mirrors `canonical_zmap_put` in `tx-types`, but operates on our no_std noun arena.
pub fn canonical_zmap_put(
    arena: &mut Arena,
    map: Noun,
    key: Noun,
    value: Noun,
) -> Result<Noun, ZMapError> {
    if noun_is_null(map, arena) {
        let kv = tuple(arena, &[key, value])?;
        let children = tuple(arena, &[arena.atom0(), arena.atom0()])?;
        return tuple(arena, &[kv, children]);
    }

    let (node, left, right) = decompose_map(map, arena)?;
    let (node_key, node_value) = decompose_pair(node, arena)?;

    if nouns_equal(key, node_key, arena) {
        if nouns_equal(value, node_value, arena) {
            Ok(map)
        } else {
            let new_node = tuple(arena, &[key, value])?;
            tuple(arena, &[new_node, left, right])
        }
    } else if gor_tip_compare(key, node_key, arena)? {
        let new_left = canonical_zmap_put(arena, left, key, value)?;
        let (new_left_node, new_left_left, new_left_right) = decompose_map(new_left, arena)?;
        let (new_left_key, _) = decompose_pair(new_left_node, arena)?;

        if mor_tip_compare(node_key, new_left_key, arena)? {
            let children = tuple(arena, &[new_left, right])?;
            tuple(arena, &[node, children])
        } else {
            let right_children = tuple(arena, &[new_left_right, right])?;
            let new_right_branch = tuple(arena, &[node, right_children])?;
            let left_children = tuple(arena, &[new_left_left, new_right_branch])?;
            tuple(arena, &[new_left_node, left_children])
        }
    } else {
        let new_right = canonical_zmap_put(arena, right, key, value)?;
        let (new_right_node, new_right_left, new_right_right) = decompose_map(new_right, arena)?;
        let (new_right_key, _) = decompose_pair(new_right_node, arena)?;

        if mor_tip_compare(node_key, new_right_key, arena)? {
            let children = tuple(arena, &[left, new_right])?;
            tuple(arena, &[node, children])
        } else {
            let left_children = tuple(arena, &[left, new_right_left])?;
            let new_left_branch = tuple(arena, &[node, left_children])?;
            let right_children = tuple(arena, &[new_left_branch, new_right_right])?;
            tuple(arena, &[new_right_node, right_children])
        }
    }
}
