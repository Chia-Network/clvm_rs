use super::node::Node;
use super::types::{EvalErr, Reduction};

const FIRST_COST: u32 = 10;
const IF_COST: u32 = 10;
const CONS_COST: u32 = 10;
const REST_COST: u32 = 10;
const LISTP_COST: u32 = 10;

impl Node {
    pub fn first(&self) -> Result<Node, EvalErr> {
        match self.as_pair() {
            Some((a, _b)) => Ok(a),
            _ => self.err("first of non-cons"),
        }
    }

    pub fn rest(&self) -> Result<Node, EvalErr> {
        match self.as_pair() {
            Some((_a, b)) => Ok(b),
            _ => self.err("rest of non-cons"),
        }
    }
}

pub fn op_if(args: &Node) -> Result<Reduction, EvalErr> {
    let cond = args.first()?;
    let mut chosen_node = args.rest()?;
    if cond.nullp() {
        chosen_node = chosen_node.rest()?;
    }
    Ok(Reduction(chosen_node.first()?, IF_COST))
}

pub fn op_cons(args: &Node) -> Result<Reduction, EvalErr> {
    let a1 = args.first()?;
    let a2 = args.rest()?.first()?;
    Ok(Reduction(Node::pair(&a1, &a2), CONS_COST))
}

pub fn op_first(args: &Node) -> Result<Reduction, EvalErr> {
    Ok(Reduction(args.first()?.first()?, FIRST_COST))
}

pub fn op_rest(args: &Node) -> Result<Reduction, EvalErr> {
    Ok(Reduction(args.first()?.rest()?, REST_COST))
}

pub fn op_listp(args: &Node) -> Result<Reduction, EvalErr> {
    match args.first()?.as_pair() {
        Some((_first, _rest)) => Ok(Reduction(Node::from(1), LISTP_COST)),
        _ => Ok(Reduction(Node::null(), LISTP_COST)),
    }
}

pub fn op_raise(args: &Node) -> Result<Reduction, EvalErr> {
    args.err("clvm raise")
}

pub fn op_eq(args: &Node) -> Result<Reduction, EvalErr> {
    let a0 = args.first()?;
    let a1 = args.rest()?.first()?;
    if let Some(s0) = a0.as_atom() {
        if let Some(s1) = a1.as_atom() {
            let cost: u32 = s0.len() as u32 + s1.len() as u32;
            return Ok(Reduction(
                if s0 == s1 {
                    Node::blob_u8(&[1])
                } else {
                    Node::null()
                },
                cost,
            ));
        }
    }
    args.err("= on list")
}
