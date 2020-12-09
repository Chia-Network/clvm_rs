use crate::allocator::{Allocator, SExp};
use crate::arc_allocator::ArcAllocator;
use crate::node::Node;

#[derive(Debug, Clone)]
pub struct EvalErr<T>(pub T, pub String);

#[derive(Debug)]
pub struct Reduction<T>(pub u32, pub T);

pub type OpFn<T> = fn(&dyn Allocator<T>, &T) -> Result<Reduction<T>, EvalErr<T>>;

pub type OperatorHandler<T> =
    Box<dyn Fn(&dyn Allocator<T>, &[u8], &T) -> Result<Reduction<T>, EvalErr<T>>>;

pub type PostEval<T> = dyn Fn(Option<&T>);

pub type PreEval<T> = Box<dyn Fn(&T, &T) -> Result<Option<Box<PostEval<T>>>, EvalErr<T>>>;

impl<'a, T> dyn Allocator<T> + 'a {
    pub fn err<V>(&self, node: &T, msg: &str) -> Result<V, EvalErr<T>> {
        let s: String = msg.into();
        Err(EvalErr(self.make_clone(node), s))
    }
}

impl ArcAllocator {
    pub fn err<T>(&self, node: &Node, msg: &str) -> Result<T, EvalErr<Node>> {
        Err(EvalErr(self.make_clone(node), msg.into()))
    }
}

impl Node {
    pub fn err<T>(&self, msg: &str) -> Result<T, EvalErr<Node>> {
        Err(EvalErr(self.clone(), msg.into()))
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
    pub fn null(&self) -> T {
        self.blob_u8(&[])
    }

    pub fn one(&self) -> T {
        self.blob_u8(&[1])
    }

    pub fn nullp(&self, v: &T) -> bool {
        match self.sexp(v) {
            SExp::Atom(a) => a.len() == 0,
            _ => false,
        }
    }
}
