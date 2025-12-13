use crate::allocator::{Allocator, NodePtr};
use crate::cost::Cost;
use crate::op_utils::get_args;
use crate::reduction::Response;
use crate::treehash::*;

pub fn op_sha256_tree(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let [n] = get_args::<1>(a, input, "sha256tree")?;
    // let mut cache = TreeCache::default();
    tree_hash_costed(a, n, max_cost)
}
