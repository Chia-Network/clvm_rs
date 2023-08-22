/// `ObjectCache` provides a way to calculate and cache values for each node
/// in a clvm object tree. It can be used to calculate the sha256 tree hash
/// for an object and save the hash for all the child objects for building
/// usage tables, for example.
///
/// It also allows a function that's defined recursively on a clvm tree to
/// have a non-recursive implementation (as it keeps a stack of uncached
/// objects locally).
use std::convert::TryInto;

use crate::allocator::{Allocator, NodePtr, SExp};
type CachedFunction<T> = fn(&mut ObjectCache<T>, &Allocator, NodePtr) -> Option<T>;
use super::bytes32::{hash_blobs, Bytes32};

pub struct ObjectCache<'a, T> {
    cache: Vec<Option<T>>,
    allocator: &'a Allocator,

    /// The function `f` is expected to calculate its T value recursively based
    /// on the T values for the left and right child for a pair. For an atom, the
    /// function f must calculate the T value directly.
    ///
    /// If a pair is passed and one of the children does not have its T value cached
    /// in `ObjectCache` yet, return `None` and f will be called with each child in turn.
    /// Don't recurse in f; that's the point of this structure.
    f: CachedFunction<T>,
}

/// turn a `NodePtr` into a `usize`. Positive values become even indices
/// and negative values become odd indices.

fn node_to_index(node: &NodePtr) -> usize {
    let value = node.0;
    if value < 0 {
        (-value - value - 1) as usize
    } else {
        (value + value) as usize
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

    /// return the function value for this node, either from cache
    /// or by calculating it
    pub fn get_or_calculate(&mut self, node: &NodePtr) -> Option<&T> {
        self.calculate(node);
        self.get_from_cache(node)
    }

    /// return the cached value for this node, or `None`
    fn get_from_cache(&self, node: &NodePtr) -> Option<&T> {
        let index = node_to_index(node);
        if index < self.cache.len() {
            self.cache[index].as_ref()
        } else {
            None
        }
    }

    /// set the cached value for a node
    fn set(&mut self, node: &NodePtr, v: T) {
        let index = node_to_index(node);
        if index >= self.cache.len() {
            self.cache.resize(index + 1, None);
        }
        self.cache[index] = Some(v)
    }

    /// calculate the function's value for the given node, traversing uncached children
    /// as necessary
    fn calculate(&mut self, root_node: &NodePtr) {
        let mut obj_list = vec![*root_node];
        while let Some(node) = obj_list.pop() {
            let v = self.get_from_cache(&node);
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

/// calculate the standard `sha256tree` has for a node

pub fn treehash(
    cache: &mut ObjectCache<Bytes32>,
    allocator: &Allocator,
    node: NodePtr,
) -> Option<Bytes32> {
    match allocator.sexp(node) {
        SExp::Pair(left, right) => match cache.get_from_cache(&left) {
            None => None,
            Some(left_value) => cache
                .get_from_cache(&right)
                .map(|right_value| hash_blobs(&[&[2], left_value, right_value])),
        },
        SExp::Atom => Some(hash_blobs(&[&[1], allocator.atom(node)])),
    }
}

/// calculate the serialized length (without backrefs) of a node. This is used
/// to check if using backrefs is actually smaller.

pub fn serialized_length(
    cache: &mut ObjectCache<u64>,
    allocator: &Allocator,
    node: NodePtr,
) -> Option<u64> {
    match allocator.sexp(node) {
        SExp::Pair(left, right) => match cache.get_from_cache(&left) {
            None => None,
            Some(left_value) => cache.get_from_cache(&right).map(|right_value| {
                1_u64
                    .saturating_add(*left_value)
                    .saturating_add(*right_value)
            }),
        },
        SExp::Atom => {
            let buf = allocator.atom(node);
            let lb: u64 = buf.len().try_into().unwrap_or(u64::MAX);
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

#[cfg(test)]
use std::cmp::max;

#[cfg(test)]
use std::fmt::Debug;

#[cfg(test)]
use std::io::Cursor;

#[cfg(test)]
use hex::FromHex;

#[cfg(test)]
use crate::serde::de::node_from_stream;

/// calculate the depth of a node. Used for tests

#[cfg(test)]
fn calculate_depth_simple(
    cache: &mut ObjectCache<usize>,
    allocator: &Allocator,
    node: NodePtr,
) -> Option<usize> {
    match allocator.sexp(node) {
        SExp::Pair(left, right) => match cache.get_from_cache(&left) {
            None => None,
            Some(left_value) => cache
                .get_from_cache(&right)
                .map(|right_value| 1 + max(*left_value, *right_value)),
        },
        SExp::Atom => Some(0),
    }
}

#[cfg(test)]
fn check_cached_function<T>(obj_as_hex: &str, expected_value: T, f: CachedFunction<T>)
where
    T: Clone + Eq + Debug,
{
    let mut allocator = Allocator::new();
    let blob: Vec<u8> = Vec::from_hex(obj_as_hex).unwrap();
    let mut cursor: Cursor<&[u8]> = Cursor::new(&blob);
    let obj = node_from_stream(&mut allocator, &mut cursor).unwrap();
    let mut oc = ObjectCache::new(&allocator, f);

    assert_eq!(oc.get_from_cache(&obj), None);

    oc.calculate(&obj);

    assert_eq!(oc.get_from_cache(&obj), Some(&expected_value));

    assert_eq!(oc.get_or_calculate(&obj).unwrap().clone(), expected_value);

    assert_eq!(oc.get_from_cache(&obj), Some(&expected_value));

    // do it again, but the simple way
    let mut oc = ObjectCache::new(&allocator, f);
    assert_eq!(oc.get_or_calculate(&obj).unwrap().clone(), expected_value);
}

#[test]
fn test_depths_cache() {
    let check = |a, b| check_cached_function(a, b, calculate_depth_simple);
    check("01", 0); // 1
    check("ff83666f6f83626172", 1); // (foo . bar)
    check("ff83666f6fff8362617280", 2); // (foo bar)
    check("ffff0102ff0304", 2); // ((1 . 2) . (3 . 4))
    check("ff01ff02ff03ff04ff05ff0680", 6); // (1 2 3 4 5 6)
}

#[test]
fn test_treehash() {
    let check = |a, b| check_cached_function(a, Bytes32::from_hex(b).unwrap(), treehash);
    check(
        "ff83666f6f83626172",
        "c518e45ae6a7b4146017b7a1d81639051b132f1f5572ce3088a3898a9ed1280b",
    ); // (foo . bar)
    check(
        "ff83666f6fff8362617280",
        "c97d97cc81100a4980080ba81ff1ba3985f7cff1db9d41d904b9d512bb875144",
    ); // (foo bar)
    check(
        "ffff0102ff0304",
        "2824018d148bc6aed0847e2c86aaa8a5407b916169f15b12cea31fa932fc4c8d",
    ); // ((1 . 2) . (3 . 4))
    check(
        "ff01ff02ff03ff04ff05ff0680",
        "65de5098d18bebd62aee37de32f0b62d1803d9c7c48f10dca25501243d7a0392",
    ); // (1 2 3 4 5 6)
}

#[test]
fn test_serialized_length() {
    let check = |a, b| check_cached_function(a, b, serialized_length);
    check("ff83666f6f83626172", 9); // (foo . bar)
    check("ff83666f6fff8362617280", 11); // (foo bar)
    check("ffff0102ff0304", 7); // ((1 . 2) . (3 . 4))
    check("ff01ff02ff03ff04ff05ff0680", 13); // (1 2 3 4 5 6)
}

#[test]
fn test_node_to_index() {
    assert_eq!(node_to_index(&NodePtr(0)), 0);
    assert_eq!(node_to_index(&NodePtr(1)), 2);
    assert_eq!(node_to_index(&NodePtr(2)), 4);
    assert_eq!(node_to_index(&NodePtr(-1)), 1);
    assert_eq!(node_to_index(&NodePtr(-2)), 3);
}

// this test takes a very long time (>60s) in debug mode, so it only runs in release mode

#[cfg(not(debug_assertions))]
#[test]
fn test_very_long_list() {
    // in this test, we check that `treehash` and `serialized_length` can handle very deep trees that
    // would normally blow out the stack. It's expensive to create such a long list, so we do both
    // tests here so we only have to to create the list once

    const LIST_SIZE: u64 = 20_000_000;
    let mut allocator = Allocator::new();
    let mut top = allocator.null();
    for _ in 0..LIST_SIZE {
        let atom = allocator.one();
        top = allocator.new_pair(atom, top).unwrap();
    }

    let expected_value = LIST_SIZE * 2 + 1;
    let mut oc = ObjectCache::new(&allocator, serialized_length);
    assert_eq!(oc.get_or_calculate(&top).unwrap().clone(), expected_value);

    let expected_value =
        <[u8; 32]>::from_hex("a168fce695099a30c0745075e6db3722ed7f059e0d7cc4d7e7504e215db5017b")
            .unwrap();
    let mut oc = ObjectCache::new(&allocator, treehash);
    assert_eq!(oc.get_or_calculate(&top).unwrap().clone(), expected_value);
}
