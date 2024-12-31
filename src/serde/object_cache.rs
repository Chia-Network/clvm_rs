/// `ObjectCache` provides a way to calculate and cache values for each node
/// in a clvm object tree. It can be used to calculate the sha256 tree hash
/// for an object and save the hash for all the child objects for building
/// usage tables, for example.
///
/// It also allows a function that's defined recursively on a clvm tree to
/// have a non-recursive implementation (as it keeps a stack of uncached
/// objects locally).
use crate::allocator::{Allocator, NodePtr, SExp};
use std::collections::HashMap;
type CachedFunction<T> = fn(&mut ObjectCache<T>, &Allocator, NodePtr) -> Option<T>;
use super::bytes32::{hash_blobs, Bytes32};
use crate::serde::serialized_length_atom;

pub struct ObjectCache<T> {
    cache: HashMap<NodePtr, T>,

    /// The function `f` is expected to calculate its T value recursively based
    /// on the T values for the left and right child for a pair. For an atom, the
    /// function f must calculate the T value directly.
    ///
    /// If a pair is passed and one of the children does not have its T value cached
    /// in `ObjectCache` yet, return `None` and f will be called with each child in turn.
    /// Don't recurse in f; that's the point of this structure.
    f: CachedFunction<T>,
}

impl<T: Clone> ObjectCache<T> {
    pub fn new(f: CachedFunction<T>) -> Self {
        Self {
            cache: HashMap::new(),
            f,
        }
    }

    /// return the function value for this node, either from cache
    /// or by calculating it. If the stop_token is specified and is found in the
    /// CLVM tree below node, traversal will stop and `None` is returned.
    pub fn get_or_calculate(
        &mut self,
        allocator: &Allocator,
        node: &NodePtr,
        stop_token: Option<NodePtr>,
    ) -> Option<&T> {
        self.calculate(allocator, node, stop_token);
        self.get_from_cache(node)
    }

    /// return the cached value for this node, or `None`
    fn get_from_cache(&self, node: &NodePtr) -> Option<&T> {
        self.cache.get(node)
    }

    /// set the cached value for a node
    fn set(&mut self, node: &NodePtr, v: T) {
        self.cache.insert(*node, v);
    }

    /// calculate the function's value for the given node, traversing uncached children
    /// as necessary. If, the optional, stop_token NodePtr is encountered in the
    /// sub tree of root_node, we stop calculations and don't add the the value
    /// for root_node to the cache. This is / used for accessing incrementally
    /// built trees, where the stop_token indicates an unfinished part of the
    /// structure.
    fn calculate(
        &mut self,
        allocator: &Allocator,
        root_node: &NodePtr,
        stop_token: Option<NodePtr>,
    ) {
        let mut obj_list = vec![*root_node];
        while let Some(node) = obj_list.pop() {
            if stop_token == Some(node) {
                // we must terminate the search if we hit the stop_token. We can't
                // traverse past it (since we're serializing incrementally).
                return;
            }
            let v = self.get_from_cache(&node);
            match v {
                Some(_) => {}
                None => match (self.f)(self, allocator, node) {
                    None => match allocator.sexp(node) {
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
        SExp::Atom => Some(hash_blobs(&[&[1], allocator.atom(node).as_ref()])),
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
            Some(serialized_length_atom(buf.as_ref()) as u64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use hex::FromHex;
    use std::cmp::max;
    use std::fmt::Debug;
    use std::io::Cursor;

    use crate::serde::de::node_from_stream;

    /// calculate the depth of a node. Used for tests
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

    fn check_cached_function<T>(obj_as_hex: &str, expected_value: T, f: CachedFunction<T>)
    where
        T: Clone + Eq + Debug,
    {
        let mut allocator = Allocator::new();
        let blob: Vec<u8> = Vec::from_hex(obj_as_hex).unwrap();
        let mut cursor: Cursor<&[u8]> = Cursor::new(&blob);
        let obj = node_from_stream(&mut allocator, &mut cursor).unwrap();
        let mut oc = ObjectCache::new(f);

        assert_eq!(oc.get_from_cache(&obj), None);

        oc.calculate(&allocator, &obj, None);

        assert_eq!(oc.get_from_cache(&obj), Some(&expected_value));

        assert_eq!(
            oc.get_or_calculate(&allocator, &obj, None).unwrap().clone(),
            expected_value
        );

        assert_eq!(oc.get_from_cache(&obj), Some(&expected_value));

        // do it again, but the simple way
        let mut oc = ObjectCache::new(f);
        assert_eq!(
            oc.get_or_calculate(&allocator, &obj, None).unwrap().clone(),
            expected_value
        );
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

    // this test takes a very long time (>60s) in debug mode, so it only runs in release mode

    #[cfg(not(debug_assertions))]
    #[test]
    fn test_very_long_list() {
        // in this test, we check that `treehash` and `serialized_length` can handle very deep trees that
        // would normally blow out the stack. It's expensive to create such a long list, so we do both
        // tests here so we only have to to create the list once

        const LIST_SIZE: u64 = 20_000_000;
        let mut allocator = Allocator::new();
        let mut top = allocator.nil();
        for _ in 0..LIST_SIZE {
            let atom = allocator.one();
            top = allocator.new_pair(atom, top).unwrap();
        }

        let expected_value = LIST_SIZE * 2 + 1;
        let mut oc = ObjectCache::new(serialized_length);
        assert_eq!(
            oc.get_or_calculate(&allocator, &top, None).unwrap().clone(),
            expected_value
        );

        let expected_value = <[u8; 32]>::from_hex(
            "a168fce695099a30c0745075e6db3722ed7f059e0d7cc4d7e7504e215db5017b",
        )
        .unwrap();
        let mut oc = ObjectCache::new(treehash);
        assert_eq!(
            oc.get_or_calculate(&allocator, &top, None).unwrap().clone(),
            expected_value
        );
    }

    fn do_check_token(
        allocator: &Allocator,
        stop_token: NodePtr,
        poisoned_nodes: &[NodePtr],
        good_nodes: &[NodePtr],
    ) {
        let mut cache = ObjectCache::new(treehash);

        for n in poisoned_nodes {
            assert!(cache
                .get_or_calculate(allocator, n, Some(stop_token))
                .is_none());
        }

        for n in good_nodes {
            assert!(cache
                .get_or_calculate(allocator, n, Some(stop_token))
                .is_some());
        }
    }

    #[test]
    fn test_stop_token() {
        // we build a tree and insert a stop_token and ensure we get `None` in
        // the appropriate places in the tree
        //            A
        //          /   \
        //         B     C
        //        / \   / \
        //       D   E F   G
        // if F is made the stop-token F, C and A should return None.
        let mut allocator = Allocator::new();

        let d = allocator.new_atom(b"d").unwrap();
        let e = allocator.new_atom(b"e").unwrap();
        let f = allocator.new_atom(b"f").unwrap();
        let g = allocator.new_atom(b"g").unwrap();
        let b = allocator.new_pair(d, e).unwrap();
        let c = allocator.new_pair(f, g).unwrap();
        let a = allocator.new_pair(b, c).unwrap();

        // if d is the stop token; d,b and a should return None
        do_check_token(&allocator, d, &[d, b, a], &[e, c, f, g]);
        do_check_token(&allocator, e, &[e, b, a], &[d, c, f, g]);
        do_check_token(&allocator, f, &[f, c, a], &[d, e, g, b]);
        do_check_token(&allocator, g, &[g, c, a], &[d, e, b, f]);
    }
}
