use std::collections::hash_set::HashSet;
use std::collections::HashMap;

use crate::bytes32::{hash_blob, hash_blobs, Bytes32};

#[derive(Debug)]
pub struct StackCache {
    root_hash: Bytes32,
    read_stack: Vec<(Bytes32, Bytes32)>,
    count: HashMap<Bytes32, usize>,
    parent_lookup: HashMap<Bytes32, Vec<(Bytes32, u8)>>,
}

impl Default for StackCache {
    fn default() -> Self {
        Self::new()
    }
}

impl StackCache {
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

    pub fn pop(&mut self) -> (Bytes32, Bytes32) {
        let item = self.read_stack.pop().expect("stack empty");
        *self.count.entry(item.0.clone()).or_insert(0) -= 1;
        *self.count.entry(self.root_hash.clone()).or_insert(0) -= 1;
        self.root_hash = item.1.clone();
        item
    }

    pub fn pop2_and_cons(&mut self) {
        // we remove two items: the right side of each left/right pair
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

    pub fn find_path(&self, id: &Bytes32, serialized_length: usize) -> Option<Vec<u8>> {
        let mut seen_ids = HashSet::<&Bytes32>::default();
        if serialized_length < 3 { return None; }
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
                    path.reverse();
                    return Some(path_to_vec_u8(path));
                }

                let parents = self.parent_lookup.get(node);
                if let Some(items) = parents {
                    for (parent, direction) in items.iter() {
                        if *(self.count.get(parent).unwrap_or(&0)) > 0 && !seen_ids.contains(parent)
                        {
                            let mut new_path = path.clone();
                            new_path.push(*direction);
                            if new_path.len() > max_path_length {
                                return None;
                            }
                            new_partial_paths.push((parent.clone(), new_path));
                        }
                        seen_ids.insert(parent);
                    }
                }
            }
            partial_paths = new_partial_paths;
        }
        None
    }
}

fn path_to_vec_u8(path: &[u8]) -> Vec<u8> {
    let byte_count = (path.len() + 1 + 7) >> 3;
    let mut v = vec![0; byte_count];
    let mut index = byte_count - 1;
    let mut mask: u8 = 1;
    for p in path.iter() {
        if *p == 1 {
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
