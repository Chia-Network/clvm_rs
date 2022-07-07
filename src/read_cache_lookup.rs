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

use crate::bytes32::{hash_blob, hash_blobs, Bytes32};

#[derive(Debug)]
pub struct ReadCacheLookup {
    root_hash: Bytes32,

    /// the stack is a cons-based list of objects. The
    /// `read_stack` corresponds to cons cells and contains
    /// the tree hashes of the contents on the left and right
    read_stack: Vec<(Bytes32, Bytes32)>,

    count: HashMap<Bytes32, usize>,

    /// a mapping of tree hashes to `(parent, is_right)` tuples
    parent_lookup: HashMap<Bytes32, Vec<(Bytes32, u8)>>,
}

impl Default for ReadCacheLookup {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadCacheLookup {
    pub fn new() -> Self {
        let root_hash = hash_blob(&[1]);
        let read_stack = vec![];
        let mut count = HashMap::default();
        count.insert(root_hash.clone(), 1);
        let parent_lookup = HashMap::default();
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

        let new_root_hash = hash_blobs(&[&[2], &id.0, &self.root_hash.0]);

        self.read_stack.push((id.clone(), self.root_hash.clone()));

        *self.count.entry(id.clone()).or_insert(0) += 1;
        *self.count.entry(new_root_hash.clone()).or_insert(0) += 1;

        let new_parent_to_old_root = (new_root_hash.clone(), 0);
        self.parent_lookup
            .entry(id)
            .or_insert(vec![])
            .push(new_parent_to_old_root);

        let new_parent_to_id = (new_root_hash.clone(), 1);
        self.parent_lookup
            .entry(self.root_hash.clone())
            .or_insert(vec![])
            .push(new_parent_to_id);

        self.root_hash = new_root_hash;
    }

    /// update the cache based on popping the top-most object
    /// returns the hash of the object in this position and
    /// the new root hash
    fn pop(&mut self) -> (Bytes32, Bytes32) {
        let item = self.read_stack.pop().expect("stack empty");
        *self.count.entry(item.0.clone()).or_insert(0) -= 1;
        *self.count.entry(self.root_hash.clone()).or_insert(0) -= 1;
        self.root_hash = item.1.clone();
        item
    }

    /// update the cache based on the "pop/pop/cons" operation used
    /// during deserialization
    pub fn pop2_and_cons(&mut self) {
        // we remove two items: each side of each left/right pair
        let right = self.pop();
        let left = self.pop();

        *self.count.entry(left.0.clone()).or_insert(0) += 1;
        *self.count.entry(right.0.clone()).or_insert(0) += 1;

        let new_root_hash = hash_blobs(&[&[2], &left.0 .0, &right.0 .0]);

        self.parent_lookup
            .entry(left.0)
            .or_insert(vec![])
            .push((new_root_hash.clone(), 0));

        self.parent_lookup
            .entry(right.0)
            .or_insert(vec![])
            .push((new_root_hash.clone(), 1));

        self.push(new_root_hash);
    }

    /// return the list of minimal-length paths to the given hash which will serialize to no larger
    /// than the given size (or an empty list if no such path exists)
    pub fn find_paths(&self, id: &Bytes32, serialized_length: usize) -> Vec<Vec<u8>> {
        let mut seen_ids = HashSet::<&Bytes32>::default();
        let mut possible_responses = vec![];
        if serialized_length < 3 {
            return possible_responses;
        }
        let max_bytes_for_path_encoding = serialized_length - 2; // 1 byte for 0xfe, 1 min byte for savings
        let max_path_length = max_bytes_for_path_encoding * 8 - 1;
        seen_ids.insert(id);
        let mut partial_paths = vec![(id.clone(), vec![])];

        loop {
            if partial_paths.is_empty() {
                break;
            }
            let mut new_partial_paths = vec![];
            for (node, path) in partial_paths.iter_mut() {
                if *node == self.root_hash {
                    possible_responses.push(reversed_path_to_vec_u8(path));
                    continue;
                }

                let parents = self.parent_lookup.get(node);
                if let Some(items) = parents {
                    for (parent, direction) in items.iter() {
                        if *(self.count.get(parent).unwrap_or(&0)) > 0 && !seen_ids.contains(parent)
                        {
                            let mut new_path = path.clone();
                            new_path.push(*direction);
                            if new_path.len() > max_path_length {
                                return possible_responses;
                            }
                            new_partial_paths.push((parent.clone(), new_path));
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

    /// If multiple paths exist, the lexigraphically smallest one will be returned.
    pub fn find_path(&self, id: &Bytes32, serialized_length: usize) -> Option<Vec<u8>> {
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

fn reversed_path_to_vec_u8(path: &[u8]) -> Vec<u8> {
    let byte_count = (path.len() + 1 + 7) >> 3;
    let mut v = vec![0; byte_count];
    let mut index = byte_count - 1;
    let mut mask: u8 = 1;
    for p in path.iter().rev() {
        if *p != 0 {
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

#[test]
fn test_path_to_vec_u8() {
    assert_eq!(reversed_path_to_vec_u8(&[]), vec!(0b1));
    assert_eq!(reversed_path_to_vec_u8(&[0]), vec!(0b10));
    assert_eq!(reversed_path_to_vec_u8(&[1]), vec!(0b11));
    assert_eq!(reversed_path_to_vec_u8(&[0, 0]), vec!(0b100));
    assert_eq!(reversed_path_to_vec_u8(&[0, 1]), vec!(0b101));
    assert_eq!(reversed_path_to_vec_u8(&[1, 0]), vec!(0b110));
    assert_eq!(reversed_path_to_vec_u8(&[1, 1]), vec!(0b111));
    assert_eq!(reversed_path_to_vec_u8(&[1, 1, 1]), vec!(0b1111));
    assert_eq!(reversed_path_to_vec_u8(&[0, 1, 1, 1]), vec!(0b10111));
    assert_eq!(reversed_path_to_vec_u8(&[1, 0, 1, 1, 1]), vec!(0b110111));
    assert_eq!(
        reversed_path_to_vec_u8(&[1, 1, 0, 1, 1, 1]),
        vec!(0b1110111)
    );
    assert_eq!(
        reversed_path_to_vec_u8(&[0, 1, 1, 0, 1, 1, 1]),
        vec!(0b10110111)
    );
    assert_eq!(
        reversed_path_to_vec_u8(&[0, 0, 1, 1, 0, 1, 1, 1]),
        vec!(0b1, 0b00110111)
    );
    assert_eq!(
        reversed_path_to_vec_u8(&[1, 0, 0, 1, 1, 0, 1, 1, 1]),
        vec!(0b11, 0b00110111)
    );
}
