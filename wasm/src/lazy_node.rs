use js_sys::Array;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

use crate::flags::ALLOW_BACKREFS;
use clvmr::allocator::{Allocator, NodePtr, SExp};
use clvmr::serde::{node_to_bytes, node_to_bytes_backrefs};

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
    pub fn to_bytes(&self, flag: u32) -> Option<Vec<u8>> {
        let serializer = if (flag & ALLOW_BACKREFS) != 0 {
            node_to_bytes_backrefs
        } else {
            node_to_bytes
        };
        serializer(&self.allocator, self.node).ok()
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
