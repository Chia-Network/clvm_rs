use js_sys::Array;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

use clvmr::allocator::{Allocator, NodePtr, SExp};
use clvmr::serde::{
    node_from_bytes, node_from_bytes_backrefs, node_to_bytes_backrefs, node_to_bytes_limit,
};

#[wasm_bindgen]
#[derive(Clone)]
pub struct LazyNode {
    allocator: Rc<Allocator>,
    node: NodePtr,
}

#[wasm_bindgen]
impl LazyNode {
    #[wasm_bindgen(getter)]
    pub fn pair(&self) -> Option<Array> {
        match &self.allocator.sexp(self.node) {
            SExp::Pair(p1, p2) => {
                let r1 = Self::new(self.allocator.clone(), *p1);
                let r2 = Self::new(self.allocator.clone(), *p2);
                let tuple = Array::new_with_length(2);
                tuple.set(0, JsValue::from(r1));
                tuple.set(1, JsValue::from(r2));
                Some(tuple)
            }
            _ => None,
        }
    }

    #[wasm_bindgen(getter)]
    pub fn atom(&self) -> Option<Vec<u8>> {
        match &self.allocator.sexp(self.node) {
            SExp::Atom => Some(self.allocator.atom(self.node).as_ref().into()),
            _ => None,
        }
    }

    #[wasm_bindgen]
    pub fn to_bytes_with_backref(&self) -> Result<Vec<u8>, String> {
        node_to_bytes_backrefs(&self.allocator, self.node).map_err(|e| e.combined_str())
    }

    #[wasm_bindgen]
    pub fn to_bytes(&self, limit: usize) -> Result<Vec<u8>, String> {
        node_to_bytes_limit(&self.allocator, self.node, limit).map_err(|e| e.combined_str())
    }

    #[wasm_bindgen]
    pub fn from_bytes_with_backref(b: &[u8]) -> Result<LazyNode, String> {
        let mut allocator = Allocator::new();
        let node = node_from_bytes_backrefs(&mut allocator, b).map_err(|e| e.combined_str())?;
        Ok(LazyNode::new(Rc::new(allocator), node))
    }

    #[wasm_bindgen]
    pub fn from_bytes(b: &[u8]) -> Result<LazyNode, String> {
        let mut allocator = Allocator::new();
        let node = node_from_bytes(&mut allocator, b).map_err(|e| e.combined_str())?;
        Ok(LazyNode::new(Rc::new(allocator), node))
    }
}

impl LazyNode {
    pub const fn new(a: Rc<Allocator>, n: NodePtr) -> Self {
        Self {
            allocator: a,
            node: n,
        }
    }
}
