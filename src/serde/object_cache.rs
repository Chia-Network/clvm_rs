use std::convert::TryInto;

use crate::allocator::{Allocator, NodePtr, SExp};

use super::bytes32::{hash_blobs, Bytes32};

/// `ObjectCache` provides a way to calculate and cache values for each node
/// in a clvm object tree. It can be used to calculate the sha256 tree hash
/// for an object and save the hash for all the child objects for building
/// usage tables, for example.
///
/// It also allows a function that's defined recursively on a clvm tree to
/// have a non-recursive implementation (as it keeps a stack of uncached
/// objects locally).
pub struct ObjectCache<'a, T> {
    cache: Vec<Option<T>>,
    allocator: &'a Allocator,

    /// The `cached_function` is expected to calculate its T value recursively based
    /// on the T values for the left and right child for a pair. For an atom, `cached_function`
    /// must calculate the T value directly.
    ///
    /// If a pair is passed and one of the children does not have its T value cached
    /// in `ObjectCache` yet, return `None` and `cached_function` will be called with each child in turn.
    /// Don't recurse in `cached_function`. That's the point of this structure.
    cached_function: CachedFunction<T>,
}

type CachedFunction<T> = fn(&mut ObjectCache<T>, &Allocator, NodePtr) -> Option<T>;

/// Turn a `NodePtr` into a `usize`. Positive values become even indices
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
    pub fn new(allocator: &'a Allocator, cached_function: CachedFunction<T>) -> Self {
        let cache = vec![];
        Self {
            cache,
            allocator,
            cached_function,
        }
    }

    /// Return the function value for this node, either from cache
    /// or by calculating it.
    pub fn get_or_calculate(&mut self, node: &NodePtr) -> Option<&T> {
        self.calculate(node);
        self.get_from_cache(node)
    }

    /// Return the cached value for this node, or `None`.
    fn get_from_cache(&self, node: &NodePtr) -> Option<&T> {
        let index = node_to_index(node);
        if index < self.cache.len() {
            self.cache[index].as_ref()
        } else {
            None
        }
    }

    /// Set the cached value for a node.
    fn set(&mut self, node: &NodePtr, value: T) {
        let index = node_to_index(node);
        if index >= self.cache.len() {
            self.cache.resize(index + 1, None);
        }
        self.cache[index] = Some(value)
    }

    /// Calculate the function's value for the given node, traversing uncached children
    /// as necessary.
    fn calculate(&mut self, root_node: &NodePtr) {
        let mut list = vec![*root_node];
        loop {
            match list.pop() {
                None => {
                    return;
                }
                Some(node) => match self.get_from_cache(&node) {
                    Some(_) => {}
                    None => match (self.cached_function)(self, self.allocator, node) {
                        None => match self.allocator.sexp(node) {
                            SExp::Pair(left, right) => {
                                list.push(node);
                                list.push(left);
                                list.push(right);
                            }
                            _ => panic!("f returned `None` for atom"),
                        },
                        Some(value) => {
                            self.set(&node, value);
                        }
                    },
                },
            }
        }
    }
}

/// Calculate the standard `sha256tree` has for a node.
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
        SExp::Atom() => Some(hash_blobs(&[&[1], allocator.atom(node)])),
    }
}

/// Calculate the serialized length (without backrefs) of a node. This is used
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
        SExp::Atom() => {
            let bytes = allocator.atom(node);
            let length = bytes.len().try_into().unwrap_or(u64::MAX);
            let result = if length == 0 || (length == 1 && bytes[0] < 128) {
                1
            } else if length < 0x40 {
                1 + length
            } else if length < 0x2000 {
                2 + length
            } else if length < 0x100000 {
                3 + length
            } else if length < 0x8000000 {
                4 + length
            } else {
                5 + length
            };
            Some(result)
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

    /// Calculate the depth of a node.
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
            SExp::Atom() => Some(0),
        }
    }

    fn check_cached_function<T>(hex: &str, expected_value: T, cached_function: CachedFunction<T>)
    where
        T: Clone + Eq + Debug,
    {
        let mut allocator = Allocator::new();
        let blob: Vec<u8> = Vec::from_hex(hex).unwrap();
        let object = node_from_stream(&mut allocator, &mut Cursor::new(&blob)).unwrap();
        let mut object_cache = ObjectCache::new(&allocator, cached_function);

        assert_eq!(object_cache.get_from_cache(&object), None);

        object_cache.calculate(&object);

        assert_eq!(object_cache.get_from_cache(&object), Some(&expected_value));
        assert_eq!(
            object_cache.get_or_calculate(&object).unwrap().clone(),
            expected_value
        );
        assert_eq!(object_cache.get_from_cache(&object), Some(&expected_value));

        // Do it again, but the simple way.
        let mut object_cache = ObjectCache::new(&allocator, cached_function);
        assert_eq!(
            object_cache.get_or_calculate(&object).unwrap().clone(),
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

    #[test]
    fn test_node_to_index() {
        assert_eq!(node_to_index(&0), 0);
        assert_eq!(node_to_index(&1), 2);
        assert_eq!(node_to_index(&2), 4);
        assert_eq!(node_to_index(&-1), 1);
        assert_eq!(node_to_index(&-2), 3);
    }

    // This test takes a very long time (>60s) in debug mode, so it only runs in release mode.
    #[cfg(not(debug_assertions))]
    #[test]
    fn test_very_long_list() {
        // In this test, we check that `treehash` and `serialized_length` can handle very deep trees that
        // would normally blow out the stack. It's expensive to create such a long list, so we do both
        // tests here so we only have to to create the list once.

        const LIST_SIZE: u64 = 20_000_000;
        let mut allocator = Allocator::new();
        let mut top = allocator.null();
        for _ in 0..LIST_SIZE {
            let atom = allocator.one();
            top = allocator.new_pair(atom, top).unwrap();
        }

        let expected_value = LIST_SIZE * 2 + 1;
        let mut object_cache = ObjectCache::new(&allocator, serialized_length);
        assert_eq!(
            object_cache.get_or_calculate(&top).unwrap().clone(),
            expected_value
        );

        let expected_value = <[u8; 32]>::from_hex(
            "a168fce695099a30c0745075e6db3722ed7f059e0d7cc4d7e7504e215db5017b",
        )
        .unwrap();
        let mut object_cache = ObjectCache::new(&allocator, treehash);
        assert_eq!(
            object_cache.get_or_calculate(&top).unwrap().clone(),
            expected_value
        );
    }
}
