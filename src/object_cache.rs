/// `ObjectCache` provides a way to calculate and cache values for each node
/// in a clvm object tree. It can be used to calculate the sha256 tree hash
/// for an object and save the hash for all the child objects for building
/// usage tables, for example.
///
/// It also allows a function that's defined recursively on a clvm tree to
/// have a non-recursive implementation (as it keeps a stack of uncached
/// objects locally).
use crate::allocator::{Allocator, NodePtr, SExp};
type CachedFunction<T> = fn(&mut ObjectCache<T>, &Allocator, NodePtr) -> Option<T>;
use crate::bytes32::{hash_blobs, Bytes32};

pub struct ObjectCache<'a, T> {
    cache: Vec<Option<T>>,
    allocator: &'a Allocator,

    /// The function `f` is expected to calculate its T value recursively based
    /// on the T values for the left and right child for a pair. For an atom, the
    /// function f must calculate the T value directly.
    ///
    /// If a pair is passed and one of the children does not have its T value cached
    /// in `ObjectCache` yet, return `None` and f will be called with each child in turn.
    /// Don't recurse in f; that's part of the point of this function.
    f: CachedFunction<T>,
}

/// turn a `NodePtr` into a `usize`. Positive values become even indices
/// and negative values become odd indices.

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
    fn get_raw(&self, node: &NodePtr) -> Option<&T> {
        let index = node_to_index(node);
        if index < self.cache.len() {
            self.cache[index].as_ref()
        } else {
            None
        }
    }
    fn set(&mut self, node: &NodePtr, v: T) {
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
    /// calculate the function's value for the given node, traversing uncached children
    /// as necessary
    fn update(&mut self, root_node: &NodePtr) {
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

/// calculate the standard `sha256tree` has for a node

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

/// calculate the serialized length (without backrefs) of a node. This is used
/// to check if using backrefs is actually smaller.

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
