use crate::serde::RandomState;
use bitvec::prelude::*;
use bitvec::vec::BitVec;
/// When deserializing a clvm object, a stack of deserialized child objects
/// is created, which can be used with back-references. A `ReadCacheLookup` keeps
/// track of the state of this stack and all child objects under each root
/// node in the stack so that we can quickly determine if a relevant
/// back-reference is available.
///
/// In other words, if we've already serialized an object with tree hash T,
/// and we encounter another object with that tree hash, we don't re-serialize
/// it, but rather include a back-reference to it. This data structure lets
/// us quickly determine which back-reference has the shortest path.
///
/// Note that there is a counter. This is because the stack contains some
/// child objects that are transient, and no longer appear in the stack
/// at later times in the parsing. We don't want to waste time looking for
/// these objects that no longer exist, so we reference-count them.
///
/// All hashes correspond to sha256 tree hashes.
use std::collections::{HashMap, HashSet};

use super::bytes32::{hash_blob, hash_blobs, Bytes32};
use super::serialized_length::atom_length_bits;

#[derive(Debug, Clone)]
pub struct ReadCacheLookup {
    root_hash: Bytes32,

    /// the stack is a cons-based list of objects. The
    /// `read_stack` corresponds to cons cells and contains
    /// the tree hashes of the contents on the left and right
    read_stack: Vec<(Bytes32, Bytes32)>,

    count: HashMap<Bytes32, u32, RandomState>,

    /// a mapping of tree hashes to `(parent, is_right)` tuples
    parent_lookup: HashMap<Bytes32, Vec<(Bytes32, bool)>, RandomState>,
}

impl Default for ReadCacheLookup {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadCacheLookup {
    pub fn new() -> Self {
        let root_hash = hash_blob(&[1]);
        let read_stack = Vec::with_capacity(1000);
        // all keys in count and parent_lookup are tree-hashes. There's no need
        // to hash them again for the hash map
        let mut count = HashMap::with_hasher(RandomState::default());
        count.insert(root_hash, 1);
        let parent_lookup = HashMap::with_hasher(RandomState::default());
        Self {
            root_hash,
            read_stack,
            count,
            parent_lookup,
        }
    }

    /// update the cache based on pushing an object with the given tree hash
    pub fn push(&mut self, id: Bytes32) {
        // we add two new entries: the new root of the tree, and this object (by id)
        // new_root: (id, old_root)

        let new_root_hash = hash_blobs(&[&[2], &id, &self.root_hash]);

        self.read_stack.push((id, self.root_hash));

        *self.count.entry(id).or_insert(0) += 1;
        *self.count.entry(new_root_hash).or_insert(0) += 1;

        let new_parent_to_old_root = (new_root_hash, false);
        self.parent_lookup
            .entry(id)
            .or_default()
            .push(new_parent_to_old_root);

        let new_parent_to_id = (new_root_hash, true);
        self.parent_lookup
            .entry(self.root_hash)
            .or_default()
            .push(new_parent_to_id);

        self.root_hash = new_root_hash;
    }

    /// update the cache based on popping the top-most object
    /// returns the hash of the object in this position and
    /// the new root hash
    fn pop(&mut self) -> (Bytes32, Bytes32) {
        let item = self.read_stack.pop().expect("stack empty");
        *self.count.entry(item.0).or_insert(0) -= 1;
        *self.count.entry(self.root_hash).or_insert(0) -= 1;
        self.root_hash = item.1;
        item
    }

    /// update the cache based on the "pop/pop/cons" operation used
    /// during deserialization
    pub fn pop2_and_cons(&mut self) {
        // we remove two items: each side of each left/right pair
        let right = self.pop();
        let left = self.pop();

        *self.count.entry(left.0).or_insert(0) += 1;
        *self.count.entry(right.0).or_insert(0) += 1;

        let new_root_hash = hash_blobs(&[&[2], &left.0, &right.0]);

        self.parent_lookup
            .entry(left.0)
            .or_default()
            .push((new_root_hash, false));

        self.parent_lookup
            .entry(right.0)
            .or_default()
            .push((new_root_hash, true));

        self.push(new_root_hash);
    }

    /// return the list of minimal-length paths to the given hash which will serialize to no larger
    /// than the given size (or an empty list if no such path exists)
    pub fn find_paths(&self, id: &Bytes32, serialized_length: u64) -> Vec<Vec<u8>> {
        // this function is not cheap. only keep going if there's potential to
        // save enough bytes
        if serialized_length < 4 {
            return vec![];
        }

        let mut possible_responses = Vec::with_capacity(50);

        // all the values we put in this hash set are themselves sha256 hashes.
        // There's no point in hashing the hashes
        let mut seen_ids = HashSet::<&Bytes32, RandomState>::with_capacity_and_hasher(
            1000,
            RandomState::default(),
        );

        let max_bytes_for_path_encoding = serialized_length - 1; // 1 byte for 0xfe
        let max_path_length: usize = (max_bytes_for_path_encoding.saturating_mul(8) - 1)
            .try_into()
            .unwrap_or(usize::MAX);
        seen_ids.insert(id);
        let mut partial_paths = Vec::with_capacity(500);
        partial_paths.push((*id, BitVec::with_capacity(100)));

        while !partial_paths.is_empty() {
            let mut new_partial_paths = vec![];
            for (node, path) in partial_paths.iter_mut() {
                if *node == self.root_hash {
                    // make sure we never return a path that needs more (or the
                    // same) bytes to serialize than the node we're referencing.
                    // path.len() + 1 is because reversed_path_to_vec_u8() will
                    // also add the "terminator" bit, at the far left (MSB)
                    // if we have 8 steps to traverse, we need 9 bits to represent
                    // it as a path
                    if let Some(path_len) = atom_length_bits(path.len() as u64 + 1) {
                        if path_len <= max_bytes_for_path_encoding {
                            possible_responses.push(reversed_path_to_vec_u8(path));
                        }
                    }
                    continue;
                }

                let parents = self.parent_lookup.get(node);
                if let Some(items) = parents {
                    for (parent, direction) in items.iter() {
                        if *(self.count.get(parent).unwrap_or(&0)) > 0 && !seen_ids.contains(parent)
                        {
                            if path.len() > max_path_length {
                                return possible_responses;
                            }
                            if path.len() < max_path_length {
                                let mut new_path = path.clone();
                                new_path.push(*direction);
                                new_partial_paths.push((*parent, new_path));
                            }
                        }
                        seen_ids.insert(parent);
                    }
                }
            }
            if !possible_responses.is_empty() {
                break;
            }
            partial_paths = new_partial_paths;
        }
        possible_responses
    }

    /// If multiple paths exist, the lexicographically smallest one will be returned.
    pub fn find_path(&self, id: &Bytes32, serialized_length: u64) -> Option<Vec<u8>> {
        let mut paths = self.find_paths(id, serialized_length);
        if !paths.is_empty() {
            paths.sort();
            paths.truncate(1);
            paths.pop()
        } else {
            None
        }
    }
}

/// Turn a list of 0/1 values (for "left/right") into `Vec<u8>` representing
/// the corresponding clvm path in the standard way.
/// `[]` => `1`
/// If `A` => `v` then `[A] + [0]` => `v * 2` and `[A] + [1]` => `v * 2 + 1`
/// Then the integer is turned into the minimal-length array of `u8` representing
/// that value as an unsigned integer.
fn reversed_path_to_vec_u8(path: &BitSlice) -> Vec<u8> {
    let byte_count = (path.len() + 1 + 7) >> 3;
    let mut v = vec![0; byte_count];
    let mut index = byte_count - 1;
    let mut mask: u8 = 1;
    for p in path.iter().rev() {
        if p != false {
            v[index] |= mask;
        }
        mask = {
            if mask == 0x80 {
                index -= 1;
                1
            } else {
                mask + mask
            }
        };
    }
    v[index] |= mask;
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_to_vec_u8() {
        assert_eq!(reversed_path_to_vec_u8(bits![]), vec!(0b1));
        assert_eq!(reversed_path_to_vec_u8(bits![0]), vec!(0b10));
        assert_eq!(reversed_path_to_vec_u8(bits![1]), vec!(0b11));
        assert_eq!(reversed_path_to_vec_u8(bits![0, 0]), vec!(0b100));
        assert_eq!(reversed_path_to_vec_u8(bits![0, 1]), vec!(0b101));
        assert_eq!(reversed_path_to_vec_u8(bits![1, 0]), vec!(0b110));
        assert_eq!(reversed_path_to_vec_u8(bits![1, 1]), vec!(0b111));
        assert_eq!(reversed_path_to_vec_u8(bits![1, 1, 1]), vec!(0b1111));
        assert_eq!(reversed_path_to_vec_u8(bits![0, 1, 1, 1]), vec!(0b10111));
        assert_eq!(
            reversed_path_to_vec_u8(bits![1, 0, 1, 1, 1]),
            vec!(0b110111)
        );
        assert_eq!(
            reversed_path_to_vec_u8(bits![1, 1, 0, 1, 1, 1]),
            vec!(0b1110111)
        );
        assert_eq!(
            reversed_path_to_vec_u8(bits![0, 1, 1, 0, 1, 1, 1]),
            vec!(0b10110111)
        );
        assert_eq!(
            reversed_path_to_vec_u8(bits![0, 0, 1, 1, 0, 1, 1, 1]),
            vec!(0b1, 0b00110111)
        );
        assert_eq!(
            reversed_path_to_vec_u8(bits![1, 0, 0, 1, 1, 0, 1, 1, 1]),
            vec!(0b11, 0b00110111)
        );
    }

    #[test]
    fn test_read_cache_lookup() {
        let large_max = 30;
        let mut rcl = ReadCacheLookup::new();

        // the only thing cached right now is a nil, right at the top of the tree (ie. `1`)
        let hash_of_nil = hash_blob(&[1]);
        assert_eq!(rcl.find_paths(&hash_of_nil, large_max), [[1]]);

        assert_eq!(rcl.count.get(&hash_of_nil).unwrap(), &1);

        // the atom `1` is not in the tree anywhere
        let hash_of_1_atom = hash_blobs(&[&[1], &[1]]);
        assert!(rcl.find_paths(&hash_of_1_atom, large_max).is_empty());

        // now let's push a `5` atom to the top
        // tree: `(5 . 0)`
        let hash_of_5_atom = hash_blobs(&[&[1], &[5]]);
        rcl.push(hash_of_5_atom);
        let hash_of_cons_5_nil = hash_blobs(&[&[2], &hash_of_5_atom, &hash_of_nil]);
        assert_eq!(rcl.find_paths(&hash_of_cons_5_nil, large_max), [[1]]);
        assert_eq!(rcl.find_paths(&hash_of_5_atom, large_max), [[2]]);
        assert_eq!(rcl.find_paths(&hash_of_nil, large_max), [[3]]);

        assert_eq!(rcl.count.get(&hash_of_cons_5_nil).unwrap(), &1);
        assert_eq!(rcl.count.get(&hash_of_5_atom).unwrap(), &1);
        assert_eq!(rcl.count.get(&hash_of_nil).unwrap(), &1);

        // the atom `1` is still not in the tree anywhere
        assert!(rcl.find_paths(&hash_of_1_atom, large_max).is_empty());

        // now let's push a `9` atom to the top
        // tree: `(9 . (5 . 0))`
        let hash_of_9_atom = hash_blobs(&[&[1], &[9]]);
        rcl.push(hash_of_9_atom);
        let hash_of_cons_9_cons_5_nil = hash_blobs(&[&[2], &hash_of_9_atom, &hash_of_cons_5_nil]);

        assert_eq!(rcl.find_paths(&hash_of_cons_9_cons_5_nil, large_max), [[1]]);
        assert_eq!(rcl.find_paths(&hash_of_9_atom, large_max), [[2]]);
        assert_eq!(rcl.find_paths(&hash_of_cons_5_nil, large_max), [[3]]);
        assert_eq!(rcl.find_paths(&hash_of_5_atom, large_max), [[5]]);
        assert_eq!(rcl.find_paths(&hash_of_nil, large_max), [[7]]);

        assert_eq!(rcl.count.get(&hash_of_cons_9_cons_5_nil).unwrap(), &1);
        assert_eq!(rcl.count.get(&hash_of_9_atom).unwrap(), &1);
        assert_eq!(rcl.count.get(&hash_of_cons_5_nil).unwrap(), &1);
        assert_eq!(rcl.count.get(&hash_of_5_atom).unwrap(), &1);
        assert_eq!(rcl.count.get(&hash_of_nil).unwrap(), &1);

        // the atom `1` is still not in the tree anywhere
        assert!(rcl.find_paths(&hash_of_1_atom, large_max).is_empty());

        // now let's push a `10` atom to the top
        // tree: `(10 . (9 . (5 . 0)))`

        let hash_of_10_atom = hash_blobs(&[&[1], &[10]]);
        rcl.push(hash_of_10_atom);
        let hash_of_cons_10_cons_9_cons_5_nil =
            hash_blobs(&[&[2], &hash_of_10_atom, &hash_of_cons_9_cons_5_nil]);
        assert_eq!(
            rcl.find_paths(&hash_of_cons_10_cons_9_cons_5_nil, large_max),
            [[1]]
        );
        assert_eq!(rcl.find_paths(&hash_of_10_atom, large_max), [[2]]);
        assert_eq!(rcl.find_paths(&hash_of_cons_9_cons_5_nil, large_max), [[3]]);
        assert_eq!(rcl.find_paths(&hash_of_9_atom, large_max), [[5]]);
        assert_eq!(rcl.find_paths(&hash_of_cons_5_nil, large_max), [[7]]);
        assert_eq!(rcl.find_paths(&hash_of_5_atom, large_max), [[11]]);
        assert_eq!(rcl.find_paths(&hash_of_nil, large_max), [[15]]);

        assert_eq!(
            rcl.count.get(&hash_of_cons_10_cons_9_cons_5_nil).unwrap(),
            &1
        );
        assert_eq!(rcl.count.get(&hash_of_10_atom).unwrap(), &1);
        assert_eq!(rcl.count.get(&hash_of_cons_9_cons_5_nil).unwrap(), &1);
        assert_eq!(rcl.count.get(&hash_of_9_atom).unwrap(), &1);
        assert_eq!(rcl.count.get(&hash_of_cons_5_nil).unwrap(), &1);
        assert_eq!(rcl.count.get(&hash_of_5_atom).unwrap(), &1);
        assert_eq!(rcl.count.get(&hash_of_nil).unwrap(), &1);

        // the atom `1` is still not in the tree anywhere
        assert!(rcl.find_paths(&hash_of_1_atom, large_max).is_empty());

        // now let's do a `pop2_and_cons`
        // tree: `((9 . 10) . (5 . 0))`
        // 2 => (9 . 10)
        // 3 => (5 . 0)
        // 4 => 9
        // 5 => 5
        // 6 => 10
        // 7 => 0
        rcl.pop2_and_cons();
        let hash_of_cons_9_10 = hash_blobs(&[&[2], &hash_of_9_atom, &hash_of_10_atom]);
        let hash_of_cons_cons_9_10_cons_5_nil =
            hash_blobs(&[&[2], &hash_of_cons_9_10, &hash_of_cons_5_nil]);
        assert_eq!(
            rcl.find_paths(&hash_of_cons_cons_9_10_cons_5_nil, large_max),
            [[1]]
        );
        assert_eq!(rcl.find_paths(&hash_of_cons_9_10, large_max), [[2]]);
        assert_eq!(rcl.find_paths(&hash_of_cons_5_nil, large_max), [[3]]);
        assert_eq!(rcl.find_paths(&hash_of_9_atom, large_max), [[4]]);
        assert_eq!(rcl.find_paths(&hash_of_10_atom, large_max), [[6]]);
        assert_eq!(rcl.find_paths(&hash_of_5_atom, large_max), [[5]]);
        assert_eq!(rcl.find_paths(&hash_of_nil, large_max), [[7]]);

        // `(9 . (5 . 0))` is no longer in the tree
        assert!(rcl
            .find_paths(&hash_of_cons_9_cons_5_nil, large_max)
            .is_empty());

        assert_eq!(
            rcl.count.get(&hash_of_cons_cons_9_10_cons_5_nil).unwrap(),
            &1
        );
        assert_eq!(rcl.count.get(&hash_of_cons_9_10).unwrap(), &1);
        assert_eq!(rcl.count.get(&hash_of_cons_5_nil).unwrap(), &1);
        assert_eq!(rcl.count.get(&hash_of_9_atom).unwrap(), &1);
        assert_eq!(rcl.count.get(&hash_of_10_atom).unwrap(), &1);
        assert_eq!(rcl.count.get(&hash_of_5_atom).unwrap(), &1);
        assert_eq!(rcl.count.get(&hash_of_nil).unwrap(), &1);

        // `(9 . (5 . 0))` is no longer in the tree
        assert_eq!(rcl.count.get(&hash_of_cons_9_cons_5_nil).unwrap(), &0);

        // the atom `1` is still not in the tree anywhere
        assert!(rcl.find_paths(&hash_of_1_atom, large_max).is_empty());

        assert!(!rcl.count.contains_key(&hash_of_1_atom));
    }
}
