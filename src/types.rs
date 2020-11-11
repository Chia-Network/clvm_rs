use super::node::Node;

#[derive(Debug, Clone)]
pub struct EvalErr(pub Node, pub String);

#[derive(Debug)]
pub struct Reduction(pub Node, pub u32);

pub type OpFn = fn(&Node) -> Result<Reduction, EvalErr>;

pub type OperatorHandler = Box<dyn Fn(&[u8], &Node) -> Result<Reduction, EvalErr>>;

pub type PostEval = dyn Fn(Option<&Node>);

pub type PreEval = Box<dyn Fn(&Node, &Node) -> Result<Option<Box<PostEval>>, EvalErr>>;

impl From<std::io::Error> for EvalErr {
    fn from(err: std::io::Error) -> Self {
        EvalErr(Node::blob("std::io::Error"), err.to_string())
    }
}

impl Node {
    pub fn err<T>(&self, msg: &str) -> Result<T, EvalErr> {
        Err(EvalErr(self.clone(), msg.into()))
    }
}

impl From<Node> for Reduction {
    fn from(node: Node) -> Self {
        Reduction(node, 1)
    }
}
