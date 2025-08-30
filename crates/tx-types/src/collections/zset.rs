/// Z-Set: A deterministic self-balancing binary search tree (treap) for sets
/// Based on the Hoon z-set implementation from nockchain
///
/// This is a treap that uses hash-based priorities for balancing:
/// - Ordering: gor-tip (single hash comparison)
/// - Balancing: mor-tip (double hash comparison)
/// - Fallback: dor-tip (structural comparison)

use std::cmp::Ordering;
use std::fmt::Debug;
use nockvm::noun::D;
use noun_serde::NounEncode;
use ibig::UBig;
use crate::hashing::tip5::Tip5Hasher;

/// A z-set is a self-balancing binary search tree for sets
#[derive(Clone)]
pub struct ZSet<T> {
    root: Option<Box<Node<T>>>,
}

/// Internal node structure
#[derive(Clone)]
struct Node<T> {
    value: T,
    left: Option<Box<Node<T>>>,
    right: Option<Box<Node<T>>>,
}

impl<T> Node<T> {
    fn new(value: T) -> Self {
        Node {
            value,
            left: None,
            right: None,
        }
    }
}

/// Trait for types that can be compared using dor-tip (structural ordering)
pub trait DorTip {
    fn dor_tip(&self, other: &Self) -> Ordering;
}

/// Default implementation for types that implement Ord
impl<T: Ord> DorTip for T {
    fn dor_tip(&self, other: &Self) -> Ordering {
        self.cmp(other)
    }
}

impl<T> ZSet<T> 
where 
    T: NounEncode + DorTip + Clone + Debug + PartialEq,
{
    /// Create a new empty z-set
    pub fn new() -> Self {
        ZSet { root: None }
    }

    /// Insert a value, maintaining treap invariants
    pub fn put(&mut self, value: T) {
        self.root = Self::put_recursive(self.root.take(), value);
    }
    
    /// Insert a value with debug output
    pub fn put_with_debug(&mut self, value: T) 
    where 
        T: Debug,
    {
        println!("Inserting value: {:?}", value);
        self.root = Self::put_recursive_debug(self.root.take(), value, 0);
    }
    
    /// Public debug structure method
    pub fn debug_structure(&self) -> String 
    where
        T: Debug,
    {
        match &self.root {
            None => "  (empty)".to_string(),
            Some(node) => Self::debug_node_structure_internal(node, 1),
        }
    }
    
    /// Internal debug structure method (always available, not just in tests)
    fn debug_structure_internal(&self) -> String {
        match &self.root {
            None => "  (empty)".to_string(),
            Some(node) => Self::debug_node_structure_internal(node, 1),
        }
    }
    
    fn debug_node_structure_internal(node: &Box<Node<T>>, indent: usize) -> String {
        let spaces = "  ".repeat(indent);
        let value_str = format!("{:?}", node.value);
        let mut result = format!("{}Node({})\n", spaces, value_str);
        
        match &node.left {
            None => result.push_str(&format!("{}  L: null\n", spaces)),
            Some(left) => {
                result.push_str(&format!("{}  L:\n", spaces));
                result.push_str(&Self::debug_node_structure_internal(left, indent + 2));
            }
        }
        
        match &node.right {
            None => result.push_str(&format!("{}  R: null", spaces)),
            Some(right) => {
                result.push_str(&format!("{}  R:\n", spaces));
                result.push_str(&Self::debug_node_structure_internal(right, indent + 2));
            }
        }
        
        result
    }

    /// Recursive insertion with debug output
    fn put_recursive_debug(node: Option<Box<Node<T>>>, value: T, depth: usize) -> Option<Box<Node<T>>> 
    where
        T: Debug,
    {
        let indent = "  ".repeat(depth);
        
        match node {
            None => {
                println!("{}Creating new node for {:?}", indent, value);
                Some(Box::new(Node::new(value)))
            }
            Some(mut n) => {
                let cmp = Self::gor_tip(&value, &n.value);
                println!("{}Comparing {:?} with node {:?}: {:?}", indent, value, n.value, cmp);
                
                match cmp {
                    Ordering::Equal => {
                        println!("{}Value already exists, no-op", indent);
                        Some(n)
                    }
                    Ordering::Less => {
                        println!("{}Going left", indent);
                        n.left = Self::put_recursive_debug(n.left, value, depth + 1);
                        
                        if let Some(ref left_child) = n.left {
                            let mor_cmp = Self::mor_tip(&n.value, &left_child.value);
                            println!("{}mor_tip({:?}, {:?}) = {:?}", indent, n.value, left_child.value, mor_cmp);
                            if mor_cmp != Ordering::Less {
                                println!("{}Rotating right", indent);
                                Some(Self::rotate_right(n))
                            } else {
                                println!("{}No rotation needed", indent);
                                Some(n)
                            }
                        } else {
                            Some(n)
                        }
                    }
                    Ordering::Greater => {
                        println!("{}Going right", indent);
                        n.right = Self::put_recursive_debug(n.right, value, depth + 1);
                        
                        if let Some(ref right_child) = n.right {
                            let mor_cmp = Self::mor_tip(&n.value, &right_child.value);
                            println!("{}mor_tip({:?}, {:?}) = {:?}", indent, n.value, right_child.value, mor_cmp);
                            if mor_cmp != Ordering::Less {
                                println!("{}Rotating left", indent);
                                Some(Self::rotate_left(n))
                            } else {
                                println!("{}No rotation needed", indent);
                                Some(n)
                            }
                        } else {
                            Some(n)
                        }
                    }
                }
            }
        }
    }
    
    /// Recursive insertion with rotations
    fn put_recursive(node: Option<Box<Node<T>>>, value: T) -> Option<Box<Node<T>>> {
        match node {
            None => {
                // Empty node, create new
                Some(Box::new(Node::new(value)))
            }
            Some(mut n) => {
                let cmp = Self::gor_tip(&value, &n.value);
                
                match cmp {
                    Ordering::Equal => {
                        // Value exists, no-op for set
                        Some(n)
                    }
                    Ordering::Less => {
                        // Insert left
                        n.left = Self::put_recursive(n.left, value);
                        
                        // Check if rotation needed (mor-tip comparison)
                        // In Hoon: if (mor-tip n.a n.c) is true, NO rotation
                        // mor-tip returns Less when first has higher priority (smaller mor value)
                        if let Some(ref left_child) = n.left {
                            if Self::mor_tip(&n.value, &left_child.value) != Ordering::Less {
                                // Parent does NOT have higher priority, rotate right
                                Some(Self::rotate_right(n))
                            } else {
                                Some(n)
                            }
                        } else {
                            Some(n)
                        }
                    }
                    Ordering::Greater => {
                        // Insert right
                        n.right = Self::put_recursive(n.right, value);
                        
                        // Check if rotation needed (mor-tip comparison)
                        // In Hoon: if (mor-tip n.a n.c) is true, NO rotation
                        // mor-tip returns Less when first has higher priority (smaller mor value)
                        if let Some(ref right_child) = n.right {
                            if Self::mor_tip(&n.value, &right_child.value) != Ordering::Less {
                                // Parent does NOT have higher priority, rotate left
                                Some(Self::rotate_left(n))
                            } else {
                                Some(n)
                            }
                        } else {
                            Some(n)
                        }
                    }
                }
            }
        }
    }

    /// Right rotation
    fn rotate_right(mut n: Box<Node<T>>) -> Box<Node<T>> {
        let mut left = n.left.take().expect("rotate_right called with no left child");
        n.left = left.right.take();
        left.right = Some(n);
        left
    }

    /// Left rotation
    fn rotate_left(mut n: Box<Node<T>>) -> Box<Node<T>> {
        let mut right = n.right.take().expect("rotate_left called with no right child");
        n.right = right.left.take();
        right.left = Some(n);
        right
    }

    /// Check if a value exists
    pub fn has(&self, value: &T) -> bool {
        Self::has_recursive(&self.root, value)
    }

    fn has_recursive(node: &Option<Box<Node<T>>>, value: &T) -> bool {
        match node {
            None => false,
            Some(n) => {
                match Self::gor_tip(value, &n.value) {
                    Ordering::Equal => true,
                    Ordering::Less => Self::has_recursive(&n.left, value),
                    Ordering::Greater => Self::has_recursive(&n.right, value),
                }
            }
        }
    }

    /// Delete a value from the tree
    pub fn del(&mut self, value: &T) {
        self.root = Self::del_recursive(self.root.take(), value);
    }

    fn del_recursive(node: Option<Box<Node<T>>>, value: &T) -> Option<Box<Node<T>>> {
        match node {
            None => None,
            Some(mut n) => {
                match Self::gor_tip(value, &n.value) {
                    Ordering::Equal => {
                        // Found node to delete, merge children
                        Self::merge_children(n.left, n.right)
                    }
                    Ordering::Less => {
                        n.left = Self::del_recursive(n.left, value);
                        Some(n)
                    }
                    Ordering::Greater => {
                        n.right = Self::del_recursive(n.right, value);
                        Some(n)
                    }
                }
            }
        }
    }

    /// Merge two subtrees maintaining heap property
    fn merge_children(left: Option<Box<Node<T>>>, right: Option<Box<Node<T>>>) -> Option<Box<Node<T>>> {
        match (left, right) {
            (None, None) => None,
            (Some(l), None) => Some(l),
            (None, Some(r)) => Some(r),
            (Some(mut l), Some(mut r)) => {
                // Choose root based on mor-tip priority
                if Self::mor_tip(&l.value, &r.value) == Ordering::Less {
                    // l has higher priority (smaller mor-tip), make it root
                    l.right = Self::merge_children(l.right, Some(r));
                    Some(l)
                } else {
                    // r has higher priority, make it root
                    r.left = Self::merge_children(Some(l), r.left);
                    Some(r)
                }
            }
        }
    }

    /// Collect all values in order
    pub fn tap(&self) -> Vec<T> {
        let mut result = Vec::new();
        Self::tap_recursive(&self.root, &mut result);
        result
    }

    fn tap_recursive(node: &Option<Box<Node<T>>>, result: &mut Vec<T>) {
        if let Some(n) = node {
            Self::tap_recursive(&n.left, result);
            result.push(n.value.clone());
            Self::tap_recursive(&n.right, result);
        }
    }

    /// Get the number of elements
    pub fn wyt(&self) -> usize {
        Self::wyt_recursive(&self.root)
    }

    fn wyt_recursive(node: &Option<Box<Node<T>>>) -> usize {
        match node {
            None => 0,
            Some(n) => 1 + Self::wyt_recursive(&n.left) + Self::wyt_recursive(&n.right),
        }
    }

    /// Check if the set is empty
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    /// Get the number of elements (alias for wyt)
    pub fn len(&self) -> usize {
        self.wyt()
    }

    /// Create an iterator over the values
    pub fn iter(&self) -> ZSetIter<T> {
        let mut stack = Vec::new();
        if let Some(ref root) = self.root {
            Self::push_left(&mut stack, root);
        }
        ZSetIter { stack }
    }
    
    
    #[cfg(test)]
    fn debug_node_structure(node: &Box<Node<T>>, indent: usize) -> String {
        let spaces = "  ".repeat(indent);
        let value_str = format!("{:?}", node.value);
        let left_str = match &node.left {
            None => format!("{}  L: null", spaces),
            Some(left) => format!("{}  L:\n{}", spaces, Self::debug_node_structure(left, indent + 2)),
        };
        let right_str = match &node.right {
            None => format!("{}  R: null", spaces),
            Some(right) => format!("{}  R:\n{}", spaces, Self::debug_node_structure(right, indent + 2)),
        };
        format!("{}Node({})\n{}\n{}", spaces, value_str, left_str, right_str)
    }

    /// Helper for iterator: push all left nodes onto stack
    fn push_left<'a>(stack: &mut Vec<&'a Box<Node<T>>>, mut node: &'a Box<Node<T>>) {
        loop {
            stack.push(node);
            match node.left {
                Some(ref left) => node = left,
                None => break,
            }
        }
    }

    /// Compute TIP5 hash of a value and convert to UBig integer
    fn compute_tip5_hash(value: &T) -> UBig {
        use nockapp::noun::slab::NounSlab;
        use crate::transaction_types::Hash;
        
        // Create a noun slab and convert value to noun
        let mut slab: NounSlab = NounSlab::new();
        let value_noun = value.to_noun(&mut slab);
        
        // Compute the TIP5 hash
        let hash = Tip5Hasher::hash_noun(value_noun)
            .unwrap_or_else(|_| Hash { values: [0; 5] });
        
        // Convert Hash to UBig using our new method
        hash.to_ubig()
    }

    /// gor-tip: Compare by single hash (UBig integer), fallback to dor-tip
    fn gor_tip(a: &T, b: &T) -> Ordering {
        let hash_a = Self::compute_tip5_hash(a);
        let hash_b = Self::compute_tip5_hash(b);
        
        // Compare the UBig integers - ORIGINAL (not reversed)
        match hash_a.cmp(&hash_b) {  // Note: a compared to b (original)
            Ordering::Equal => a.dor_tip(b),
            other => other,
        }
    }

    /// mor-tip: Compare by double hash (hash of hash concatenated with itself), fallback to dor-tip
    fn mor_tip(a: &T, b: &T) -> Ordering {
        // Compute double-tip for each value
        let double_a = Self::compute_double_tip5_hash(a);
        let double_b = Self::compute_double_tip5_hash(b);
        
        // ORIGINAL (not reversed)
        match double_a.cmp(&double_b) {  // Note: a compared to b (original)
            Ordering::Equal => a.dor_tip(b),
            other => other,
        }
    }
    
    /// Compute double-tip: hash(hash(value) ++ hash(value))
    fn compute_double_tip5_hash(value: &T) -> UBig {
        use nockapp::noun::slab::NounSlab;
        use crate::transaction_types::Hash;
        use crate::hashing::hasher::hash_ten_cell;
        
        // First compute the regular hash
        let mut slab: NounSlab = NounSlab::new();
        let value_noun = value.to_noun(&mut slab);
        
        let hash = Tip5Hasher::hash_noun(value_noun)
            .unwrap_or_else(|_| Hash { values: [0; 5] });
        
        // Use hash_ten_cell with two copies of the hash
        let double_hash = hash_ten_cell(hash.clone(), hash);
        
        // Convert to UBig
        double_hash.to_ubig()
    }

    /// Build from an iterator (gas equivalent)
    pub fn gas<I>(iter: I) -> Self 
    where 
        I: IntoIterator<Item = T>
    {
        let mut set = Self::new();
        for v in iter {
            set.put(v);
        }
        set
    }
    
    /// Convert the z-set to a Hashable structure
    /// This preserves the exact tree structure for hashing
    pub fn to_hashable<F>(&self, val_fn: F) -> crate::hashing::hashable::Hashable 
    where
        F: Fn(&T) -> crate::hashing::hashable::Hashable + Copy,
    {
        use crate::hashing::hashable::Hashable;
        
        match &self.root {
            None => Hashable::null(),  // Empty tree
            Some(node) => Self::node_to_hashable(node, val_fn),
        }
    }
    
    /// Convert a node to hashable recursively
    fn node_to_hashable<F>(
        node: &Box<Node<T>>, 
        val_fn: F
    ) -> crate::hashing::hashable::Hashable
    where
        F: Fn(&T) -> crate::hashing::hashable::Hashable + Copy,
    {
        use crate::hashing::hashable::Hashable;
        
        // Get hashable for the current node's value
        let node_hashable = val_fn(&node.value);
        
        // Process left subtree
        let left = match &node.left {
            None => Hashable::null(),
            Some(left_node) => Self::node_to_hashable(left_node, val_fn),
        };
        
        // Process right subtree
        let right = match &node.right {
            None => Hashable::null(),
            Some(right_node) => Self::node_to_hashable(right_node, val_fn),
        };
        
        // Return as triple: [node_value, left, right]
        // This matches the Hoon z-set hashable structure
        Hashable::triple(node_hashable, left, right)
    }
    
    /// Convert a node to noun recursively for NounEncode
    fn node_to_noun<A: nockvm::noun::NounAllocator>(
        node: &Box<Node<T>>, 
        alloc: &mut A
    ) -> nockvm::noun::Noun {
        use nockvm::noun::T;
        
        // Get noun for the current node's value
        let value_noun = node.value.to_noun(alloc);
        
        // Process left subtree
        let left_noun = match &node.left {
            None => D(0),
            Some(left_node) => Self::node_to_noun(left_node, alloc),
        };
        
        // Process right subtree
        let right_noun = match &node.right {
            None => D(0),
            Some(right_node) => Self::node_to_noun(right_node, alloc),
        };
        
        // Return as triple: [value left-tree right-tree]
        // This matches the Hoon z-set structure
        T(alloc, &[value_noun, left_noun, right_noun])
    }
    
}

/// Iterator over ZSet values
pub struct ZSetIter<'a, T> {
    stack: Vec<&'a Box<Node<T>>>,
}

impl<'a, T> Iterator for ZSetIter<'a, T> 
where
    T: NounEncode + DorTip + Clone + Debug + PartialEq,
{
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(node) = self.stack.pop() {
            // If this node has a right child, push all its left descendants
            if let Some(ref right) = node.right {
                ZSet::push_left(&mut self.stack, right);
            }
            Some(&node.value)
        } else {
            None
        }
    }
}

// Implement Default
impl<T> Default for ZSet<T> 
where 
    T: NounEncode + DorTip + Clone + Debug + PartialEq,
{
    fn default() -> Self {
        Self::new()
    }
}

// Debug implementation for visualization
impl<T: Debug> Debug for ZSet<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ZSet {{ root: ")?;
        fmt_node(&self.root, f, 0)?;
        write!(f, " }}")
    }
}

// Implement PartialEq for ZSet
impl<T> PartialEq for ZSet<T>
where
    T: NounEncode + DorTip + Clone + Debug + PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        // Two sets are equal if they contain the same elements
        self.tap() == other.tap()
    }
}

// Implement Eq for ZSet
impl<T> Eq for ZSet<T>
where
    T: NounEncode + DorTip + Clone + Debug + PartialEq + Eq,
{
}

// Implement Hash for ZSet
use std::hash::{Hash, Hasher};

impl<T> Hash for ZSet<T>
where
    T: NounEncode + DorTip + Clone + Debug + PartialEq + Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash the elements in order
        let items = self.tap();
        items.hash(state);
    }
}

// Implement PartialOrd for ZSet
impl<T> PartialOrd for ZSet<T>
where
    T: NounEncode + DorTip + Clone + Debug + PartialEq + PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.tap().partial_cmp(&other.tap())
    }
}

// Implement Ord for ZSet
impl<T> Ord for ZSet<T>
where
    T: NounEncode + DorTip + Clone + Debug + PartialEq + Eq + Ord,
{
    fn cmp(&self, other: &Self) -> Ordering {
        self.tap().cmp(&other.tap())
    }
}

// Implement NounEncode for ZSet
impl<T> NounEncode for ZSet<T>
where
    T: NounEncode + DorTip + Clone + Debug + PartialEq,
{
    fn to_noun<A: nockvm::noun::NounAllocator>(&self, alloc: &mut A) -> nockvm::noun::Noun {
        // Convert to a tree structure matching Hoon's z-set
        // Empty set is ~
        // Node is [value left-tree right-tree]
        match &self.root {
            None => D(0), // Empty set is ~
            Some(node) => Self::node_to_noun(node, alloc),
        }
    }
}

// Implement NounDecode for ZSet
use noun_serde::{NounDecode, NounDecodeError};
use nockvm::noun::{Noun, NounAllocator};

impl<T> ZSet<T>
where
    T: NounDecode + NounEncode + DorTip + Clone + Debug + PartialEq,
{
    /// Decode a node from noun recursively for NounDecode
    fn node_from_noun(
        noun: &Noun
    ) -> Result<Option<Box<Node<T>>>, NounDecodeError> {
        // Check if it's empty (~ = 0)
        if noun.as_atom().is_ok() && noun.as_atom().unwrap().as_u64().unwrap_or(1) == 0 {
            return Ok(None);
        }
        
        // Otherwise it should be a cell [value left right]
        let cell = noun.as_cell()
            .map_err(|_| NounDecodeError::ExpectedCell)?;
        
        // First element is the value
        let value = T::from_noun(&cell.head())?;
        
        // Second element should be a cell [left right]
        let tail_cell = cell.tail().as_cell()
            .map_err(|_| NounDecodeError::ExpectedCell)?;
        
        // Recursively decode left and right subtrees
        let left = Self::node_from_noun(&tail_cell.head())?;
        let right = Self::node_from_noun(&tail_cell.tail())?;
        
        Ok(Some(Box::new(Node {
            value,
            left,
            right,
        })))
    }
}

impl<T> NounDecode for ZSet<T>
where
    T: NounDecode + NounEncode + DorTip + Clone + Debug + PartialEq,
{
    fn from_noun(noun: &Noun) -> Result<Self, NounDecodeError> {
        // Decode from tree structure matching Hoon's z-set
        let root = Self::node_from_noun(noun)?;
        Ok(ZSet { root })
    }
}

// Helper function for debug formatting
fn fmt_node<T: Debug>(node: &Option<Box<Node<T>>>, f: &mut std::fmt::Formatter<'_>, depth: usize) -> std::fmt::Result {
    match node {
        None => write!(f, "~"),
        Some(n) => {
            write!(f, "\n{:indent$}[{:?}]", "", n.value, indent = depth * 2)?;
            if n.left.is_some() || n.right.is_some() {
                write!(f, "\n{:indent$}  L: ", "", indent = depth * 2)?;
                fmt_node(&n.left, f, depth + 2)?;
                write!(f, "\n{:indent$}  R: ", "", indent = depth * 2)?;
                fmt_node(&n.right, f, depth + 2)?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Wrapper for i32 to implement NounEncode
    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct TestInt(i32);
    
    impl NounEncode for TestInt {
        fn to_noun<A: nockvm::noun::NounAllocator>(&self, alloc: &mut A) -> nockvm::noun::Noun {
            use nockvm::noun::Atom;
            // Convert to u64 for atom creation (handle negative values)
            let value = if self.0 < 0 {
                // For negative numbers, use two's complement representation
                (self.0 as i64) as u64
            } else {
                self.0 as u64
            };
            Atom::new(alloc, value).as_noun()
        }
    }

    #[test]
    fn test_basic_operations() {
        let mut set: ZSet<TestInt> = ZSet::new();
        
        // Test insertion
        set.put(TestInt(5));
        set.put(TestInt(3));
        set.put(TestInt(7));
        set.put(TestInt(1));
        set.put(TestInt(9));
        set.put(TestInt(5)); // Duplicate, should be ignored
        
        // Test has
        assert!(set.has(&TestInt(5)));
        assert!(set.has(&TestInt(3)));
        assert!(!set.has(&TestInt(10)));
        
        // Test wyt (count)
        assert_eq!(set.wyt(), 5);
        
        // Test deletion
        set.del(&TestInt(3));
        assert!(!set.has(&TestInt(3)));
        assert_eq!(set.wyt(), 4);
        
        // Test tap (in-order traversal)
        // Note: ZSet uses hash-based ordering (gor-tip), not natural ordering
        let items = set.tap();
        assert_eq!(items.len(), 4);
        // Verify all expected values are present
        let values: Vec<i32> = items.iter().map(|t| t.0).collect();
        assert!(values.contains(&1));
        assert!(values.contains(&5));
        assert!(values.contains(&7));
        assert!(values.contains(&9));
    }

    #[test]
    fn test_gas_construction() {
        let values = vec![
            TestInt(10),
            TestInt(20),
            TestInt(15),
            TestInt(5),
            TestInt(25),
            TestInt(20), // Duplicate
        ];
        
        let set = ZSet::gas(values);
        
        assert_eq!(set.wyt(), 5); // Only 5 unique values
        assert!(set.has(&TestInt(15)));
        
        // Check all items are present
        let items = set.tap();
        assert_eq!(items.len(), 5);
        let values: Vec<i32> = items.iter().map(|t| t.0).collect();
        assert!(values.contains(&5));
        assert!(values.contains(&10));
        assert!(values.contains(&15));
        assert!(values.contains(&20));
        assert!(values.contains(&25));
    }

    #[test]
    fn test_iterator() {
        let mut set = ZSet::new();
        for i in [5, 2, 8, 1, 9, 3] {
            set.put(TestInt(i));
        }
        
        let collected: Vec<_> = set.iter().map(|t| t.0).collect();
        assert_eq!(collected.len(), 6);
        
        // All values should be present
        for i in [1, 2, 3, 5, 8, 9] {
            assert!(collected.contains(&i));
        }
    }
    
    #[test]
    fn test_numbers_1_to_10_tree_structure() {
        use crate::hashing::tip5::Tip5Hasher;
        use nockapp::noun::slab::NounSlab;
        use noun_serde::NounEncode;
        
        println!("\n=== Testing ZSet with numbers 1 through 10 ===\n");
        
        let mut set = ZSet::new();
        
        // Insert numbers 1 through 10
        for i in 1..=10 {
            println!("Inserting {}", i);
            set.put(TestInt(i));
        }
        
        // Get the tree structure
        println!("\n--- Tree Structure ---");
        println!("{}", set.debug_structure());
        
        // Get the in-order traversal
        let items = set.tap();
        println!("\n--- In-order Traversal (based on gor-tip ordering) ---");
        for (idx, item) in items.iter().enumerate() {
            // Compute the hash for each value to understand ordering
            let mut slab: NounSlab = NounSlab::new();
            let noun = item.to_noun(&mut slab);
            let hash = Tip5Hasher::hash_noun(noun).unwrap();
            println!("{}: value={}, hash={:x?}", idx, item.0, &hash.values[0..2]); // Show first 2 values of hash
        }
        
        // Verify all numbers are present
        println!("\n--- Verification ---");
        assert_eq!(set.wyt(), 10, "Should have exactly 10 elements");
        for i in 1..=10 {
            assert!(set.has(&TestInt(i)), "Should contain {}", i);
        }
        println!("✓ All numbers 1-10 are present in the set");
        
        // Show the actual order
        let ordered_values: Vec<i32> = items.iter().map(|t| t.0).collect();
        println!("\n--- Final Order ---");
        println!("Order: {:?}", ordered_values);
    }
}