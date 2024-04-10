use js_sys::Array;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

use clvmr::allocator::{Allocator, NodePtr, SExp};
use clvmr::serde::{node_to_bytes_limit, node_to_bytes_backrefs_limit};

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
    pub fn to_bytes_with_backref(&self, limit: usize) -> Result<Vec<u8>, String> {
        node_to_bytes_backrefs_limit(&self.allocator, self.node, limit).map_err(|e| e.to_string())
    }

    #[wasm_bindgen]
    pub fn to_bytes(&self, limit: usize) -> Result<Vec<u8>, String> {
        node_to_bytes_limit(&self.allocator, self.node, limit).map_err(|e| e.to_string())
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
