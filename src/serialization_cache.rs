use crate::allocator::{Allocator, NodePtr, SExp};
use crate::sha2::{Digest, Sha256};
use std::collections::hash_set::HashSet;
use std::collections::HashMap;
use std::hash::Hash;
type HashFunction<T> = fn(&mut ObjectCache<T>, &Allocator, NodePtr) -> Option<T>;
use crate::bytes32::{hash_blobs, Bytes32};

pub struct ObjectCache<'a, T> {
    hash: HashMap<NodePtr, T>,
    allocator: &'a Allocator,
    f: HashFunction<T>,
}

impl<'a, T: Clone> ObjectCache<'a, T> {
    pub fn new(allocator: &'a Allocator, f: HashFunction<T>) -> Self {
        let hash = HashMap::new();
        Self { hash, allocator, f }
    }
    pub fn get(&mut self, node: &NodePtr) -> Option<&T> {
        self.update(node);
        self.hash.get(node)
    }
}

impl<'a, T: Hash + Eq + Clone> ObjectCache<'a, T> {
    pub fn invert(&self) -> HashMap<&T, HashSet<&NodePtr>> {
        invert_hashmap(&self.hash)
    }
}

pub fn generate_cache<T>(
    allocator: &Allocator,
    root_node: NodePtr,
    f: HashFunction<T>,
) -> ObjectCache<T>
where
    T: Clone,
{
    let mut cache: ObjectCache<T> = ObjectCache::new(allocator, f);
    cache.update(&root_node);
    cache
}

impl<'a, T: Clone> ObjectCache<'a, T> {
    pub fn update(&mut self, root_node: &NodePtr) {
        let mut obj_list = vec![*root_node];
        loop {
            match obj_list.pop() {
                None => {
                    return;
                }
                Some(node) => {
                    let v = self.hash.get(&node);
                    match v {
                        Some(_) => {}
                        None => match (self.f)(self, self.allocator, node) {
                            None => match self.allocator.sexp(node) {
                                SExp::Pair(left, right) => {
                                    obj_list.push(node);
                                    obj_list.push(left);
                                    obj_list.push(right);
                                }
                                _ => panic!("f returned `None` for atom"),
                            },
                            Some(v) => {
                                self.hash.insert(node, v);
                            }
                        },
                    }
                }
            }
        }
    }
}

pub fn treehash(
    cache: &mut ObjectCache<Bytes32>,
    allocator: &Allocator,
    node: NodePtr,
) -> Option<Bytes32> {
    match allocator.sexp(node) {
        SExp::Pair(left, right) => match cache.hash.get(&left) {
            None => None,
            Some(left_value) => cache
                .hash
                .get(&right)
                .map(|right_value| hash_blobs(&[&[2], &left_value.0, &right_value.0])),
        },
        SExp::Atom(atom_buf) => Some(hash_blobs(&[&[1], allocator.buf(&atom_buf)])),
    }
}

pub fn serialized_length(
    cache: &mut ObjectCache<usize>,
    allocator: &Allocator,
    node: NodePtr,
) -> Option<usize> {
    match allocator.sexp(node) {
        SExp::Pair(left, right) => match cache.hash.get(&left) {
            None => None,
            Some(left_value) => cache
                .hash
                .get(&right)
                .map(|right_value| 1 + left_value + right_value),
        },
        SExp::Atom(atom_buf) => {
            let buf = allocator.buf(&atom_buf);
            let lb = buf.len();
            Some(if lb == 0 || (lb == 1 && buf[0] < 128) {
                1
            } else if lb < 0x40 {
                1 + lb
            } else if lb < 0x2000 {
                2 + lb
            } else if lb < 0x100000 {
                3 + lb
            } else if lb < 0x8000000 {
                4 + lb
            } else {
                5 + lb
            })
        }
    }
}

pub fn parent_path(
    cache: &mut ObjectCache<HashSet<(NodePtr, i8)>>,
    allocator: &Allocator,
    node: NodePtr,
) -> Option<HashSet<(NodePtr, i8)>> {
    match allocator.sexp(node) {
        SExp::Pair(left, right) => {
            if cache.hash.contains_key(&left) && cache.hash.contains_key(&right) {
                let left_value = cache.hash.get_mut(&left).unwrap();
                left_value.insert((node, 0));
                let right_value = cache.hash.get_mut(&right).unwrap();
                right_value.insert((node, 0));
                Some(HashSet::new())
            } else {
                None
            }
        }
        SExp::Atom(_atom_buf) => Some(HashSet::new()),
    }
}

pub fn invert_hashmap<K: Hash + Eq, V: Hash + Eq>(map: &HashMap<K, V>) -> HashMap<&V, HashSet<&K>> {
    let mut hm = HashMap::new();
    for (k, v) in map.iter() {
        if !hm.contains_key(v) {
            hm.insert(v, HashSet::new());
        }
        let hs = hm.get_mut(v).unwrap();
        hs.insert(k);
    }
    hm
}
