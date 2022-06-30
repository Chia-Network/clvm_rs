use crate::allocator::{Allocator, NodePtr, SExp};
type CachedFunction<T> = fn(&mut ObjectCache<T>, &Allocator, NodePtr) -> Option<T>;
use crate::bytes32::{hash_blobs, Bytes32};

pub struct ObjectCache<'a, T> {
    cache: Vec<Option<T>>,
    allocator: &'a Allocator,
    f: CachedFunction<T>,
}

fn node_to_index(node: &NodePtr) -> usize {
    let node = *node;
    if node < 0 {
        (-node - node - 1) as usize
    } else {
        (node + node) as usize
    }
}

impl<'a, T: Clone> ObjectCache<'a, T> {
    pub fn new(allocator: &'a Allocator, f: CachedFunction<T>) -> Self {
        let cache = vec![];
        Self {
            cache,
            allocator,
            f,
        }
    }
    pub fn get(&mut self, node: &NodePtr) -> Option<&T> {
        self.update(node);
        self.get_raw(node)
    }
    pub fn get_raw(&self, node: &NodePtr) -> Option<&T> {
        let index = node_to_index(node);
        if index < self.cache.len() {
            self.cache[index].as_ref()
        } else {
            None
        }
    }
    pub fn set(&mut self, node: &NodePtr, v: T) {
        let index = node_to_index(node);
        if index >= self.cache.len() {
            self.cache.resize(index + 1, None);
        }
        self.cache[index] = Some(v)
    }
}

pub fn generate_cache<T>(
    allocator: &Allocator,
    root_node: NodePtr,
    f: CachedFunction<T>,
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
                    let v = self.get_raw(&node);
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
                                self.set(&node, v);
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
        SExp::Pair(left, right) => match cache.get_raw(&left) {
            None => None,
            Some(left_value) => cache
                .get_raw(&right)
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
        SExp::Pair(left, right) => match cache.get_raw(&left) {
            None => None,
            Some(left_value) => cache
                .get_raw(&right)
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
