use crate::allocator::{Allocator, NodePtr};
use crate::node::Node;
use crate::reduction::EvalErr;
use crate::serialize::node_from_bytes;
use crate::serialize::node_to_bytes_backrefs;

pub fn compress_with_backrefs<'a>(
    allocator: &mut Allocator,
    node_ptr: NodePtr,
) -> Result<NodePtr, EvalErr> {
    let node = Node {
        allocator,
        node: node_ptr,
    };
    let compressed_block = node_to_bytes_backrefs(&node).expect("can't compress");
    let apply_node = allocator.new_atom(&[2])?;
    let quote_node = allocator.new_atom(&[1])?;
    let serialized_backrefs_program = include_bytes!("deserialize_w_backrefs.bin");
    // "(a (q . deserialize_w_backrefs_program) (q . serialized_with_backrefs))"
    let program = node_from_bytes(allocator, serialized_backrefs_program)
        .expect("can't deserialize backref prog");
    let compressed_block = allocator.new_atom(&compressed_block)?;
    let compressed_block = allocator.new_pair(quote_node, compressed_block)?;
    let program = allocator.new_pair(quote_node, program)?;
    let list = allocator.null();
    let list = allocator.new_pair(compressed_block, list)?;
    let list = allocator.new_pair(program, list)?;
    let list = allocator.new_pair(apply_node, list)?;
    Ok(list)
}
