use crate::allocator::{Allocator, SExp};
use crate::node::Node;
use crate::reduction::{EvalErr, Response};

pub type OpFn<T> = fn(&Node<T>) -> Response<T>;

pub type OperatorHandler<T> = Box<dyn Fn(&dyn Allocator<Ptr = T>, &[u8], &T) -> Response<T>>;

impl<'a, T> dyn Allocator<Ptr = T> + 'a {
    pub fn err<V>(&self, node: &T, msg: &str) -> Result<V, EvalErr<T>> {
        let s: String = msg.into();
        Err(EvalErr(self.make_clone(node), s))
    }
}

impl<'a, T> dyn Allocator<Ptr = T> + 'a {
    pub fn nullp(&self, v: &T) -> bool {
        match self.sexp(v) {
            SExp::Atom(a) => a.is_empty(),
            _ => false,
        }
    }
}
