use std::rc::Rc;
use wasm_bindgen::prelude::wasm_bindgen;
use clvmr::{Allocator, ALLOW_BACKREFS};
use clvmr::serde::{node_from_bytes, node_from_bytes_backrefs, serialized_length_from_bytes};
use crate::lazy_node::LazyNode;

#[wasm_bindgen]
pub fn serialized_length(program: &[u8]) -> Result<u64, String> {
    match serialized_length_from_bytes(program) {
        Ok(length) => Ok(length),
        Err(err) => Err(err.to_string()),
    }
}

#[wasm_bindgen]
pub fn sexp_from_bytes(b: &[u8], flag: u32) -> Result<LazyNode, String> {
    let mut allocator = Allocator::new();
    let deserializer = if (flag & ALLOW_BACKREFS) != 0 {
        node_from_bytes_backrefs
    } else {
        node_from_bytes
    };
    match deserializer(&mut allocator, b) {
        Ok(node) => {
            Ok(LazyNode::new(Rc::new(allocator), node))
        },
        Err(err) => Err(err.to_string())
    }
}
