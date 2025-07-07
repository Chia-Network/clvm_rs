use std::rc::Rc;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::flags::ALLOW_BACKREFS;
use crate::lazy_node::LazyNode;
use clvmr::serde::{
    node_from_bytes as _node_from_bytes, node_from_bytes_backrefs, serialized_length_from_bytes,
};
use clvmr::Allocator;

#[wasm_bindgen]
pub fn serialized_length(program: &[u8]) -> Result<u64, String> {
    serialized_length_from_bytes(program).map_err(|x| x.combined_str())
}

#[wasm_bindgen]
pub fn node_from_bytes(b: &[u8], flag: u32) -> Result<LazyNode, String> {
    let mut allocator = Allocator::new();
    let deserializer = if (flag & ALLOW_BACKREFS) != 0 {
        node_from_bytes_backrefs
    } else {
        _node_from_bytes
    };
    let node = deserializer(&mut allocator, b).map_err(|e| e.combined_str())?;
    Ok(LazyNode::new(Rc::new(allocator), node))
}
