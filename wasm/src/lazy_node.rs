use clvmr::allocator::{ImmutableAllocator, NodePtr, SExp};
use std::rc::Rc;

use js_sys::Array;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
#[derive(Clone)]
pub struct LazyNode {
    allocator: Rc<ImmutableAllocator>,
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
            SExp::Atom => Some(self.allocator.atom(self.node).into()),
            _ => None,
        }
    }
}

impl LazyNode {
    pub const fn new(a: Rc<ImmutableAllocator>, n: NodePtr) -> Self {
        Self {
            allocator: a,
            node: n,
        }
    }
}
