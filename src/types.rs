use crate::allocator::{Allocator, SExp};
use crate::node::Node;
use crate::reduction::{EvalErr, Response};

pub type OpFn<T> = fn(&Node<T>) -> Response<T>;

pub type OperatorHandler<T> = Box<dyn Fn(&dyn Allocator<T>, &[u8], &T) -> Response<T>>;

impl<'a, T> dyn Allocator<T> + 'a {
    pub fn err<V>(&self, node: &T, msg: &str) -> Result<V, EvalErr<T>> {
        let s: String = msg.into();
        Err(EvalErr(self.make_clone(node), s))
    }
}

impl<'a, T> dyn Allocator<T> + 'a {
    pub fn first(&self, v: &T) -> Result<T, EvalErr<T>> {
        match self.sexp(v) {
            SExp::Pair(a, _b) => Ok(a),
            _ => self.err(v, "first of non-cons"),
        }
    }
    pub fn rest(&self, v: &T) -> Result<T, EvalErr<T>> {
        match self.sexp(v) {
            SExp::Pair(_a, b) => Ok(b),
            _ => self.err(v, "rest of non-cons"),
        }
    }
}

impl<'a, T> dyn Allocator<T> + 'a {
    pub fn nullp(&self, v: &T) -> bool {
        match self.sexp(v) {
            SExp::Atom(a) => a.len() == 0,
            _ => false,
        }
    }
}
