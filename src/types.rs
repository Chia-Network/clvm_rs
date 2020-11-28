use super::node::{Allocator, Node};

#[derive(Debug, Clone)]
pub struct EvalErr(pub Node, pub String);

#[derive(Debug)]
pub struct Reduction(pub u32, pub Node);

pub type OpFn = fn(&Allocator, &Node) -> Result<Reduction, EvalErr>;

pub type OperatorHandler = Box<dyn Fn(&Allocator, &[u8], &Node) -> Result<Reduction, EvalErr>>;

pub type PostEval = dyn Fn(Option<&Node>);

pub type PreEval = Box<dyn Fn(&Node, &Node) -> Result<Option<Box<PostEval>>, EvalErr>>;

fn eval_err_for_allocator(allocator: &Allocator, err: std::io::Error) -> EvalErr {
    EvalErr(allocator.blob("std::io::Error"), err.to_string())
}

impl Node {
    pub fn err<T>(&self, msg: &str) -> Result<T, EvalErr> {
        Err(EvalErr(self.clone(), msg.into()))
    }
}
