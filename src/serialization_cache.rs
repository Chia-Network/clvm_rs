use crate::allocator::{Allocator, NodePtr, SExp};
use crate::sha2::{Digest, Sha256};
use std::collections::HashMap;

type HashFunction<T> = fn(&mut ObjectCache<T>, &Allocator, NodePtr) -> Option<T>;

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
    cache: &mut ObjectCache<[u8; 32]>,
    allocator: &Allocator,
    node: NodePtr,
) -> Option<[u8; 32]> {
    match allocator.sexp(node) {
        SExp::Pair(left, right) => match cache.hash.get(&left) {
            None => None,
            Some(left_value) => match cache.hash.get(&right) {
                None => None,
                Some(right_value) => {
                    let mut sha256 = Sha256::new();
                    sha256.update(&[2]);
                    sha256.update(left_value);
                    sha256.update(right_value);
                    Some(sha256.finalize().into())
                }
            },
        },
        SExp::Atom(atom_buf) => {
            let mut sha256 = Sha256::new();
            sha256.update(&[1]);
            sha256.update(allocator.buf(&atom_buf));
            Some(sha256.finalize().into())
        }
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
