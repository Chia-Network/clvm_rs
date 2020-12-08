use super::node::{Allocator, ArcAllocator, Node, U8};

#[derive(Debug, Clone)]
pub struct EvalErr<T>(pub T, pub String);

#[derive(Debug)]
pub struct Reduction<T>(pub u32, pub T);

pub type OpFn<T> = fn(&dyn Allocator<Node, U8>, &T) -> Result<Reduction<T>, EvalErr<T>>;

pub type OperatorHandler<T> =
    Box<dyn Fn(&dyn Allocator<Node, U8>, &[u8], &T) -> Result<Reduction<T>, EvalErr<T>>>;

pub type PostEval<T> = dyn Fn(Option<&T>);

pub type PreEval<T> = Box<dyn Fn(&T, &T) -> Result<Option<Box<PostEval<T>>>, EvalErr<T>>>;

impl<'a, T, U> dyn Allocator<T, U> + 'a {
    pub fn err<V>(&self, node: &Node, msg: &str) -> Result<V, EvalErr<Node>> {
        Err(EvalErr(node.clone(), msg.into()))
    }
}

impl ArcAllocator {
    pub fn err<T>(&self, node: &Node, msg: &str) -> Result<T, EvalErr<Node>> {
        Err(EvalErr(node.clone(), msg.into()))
    }
}

impl Node {
    pub fn err<T>(&self, msg: &str) -> Result<T, EvalErr<Node>> {
        Err(EvalErr(self.clone(), msg.into()))
    }
}
