use crate::allocator::Allocator;
use crate::node::Node;
use crate::op_utils::{atom, check_arg_count};
use crate::reduction::{EvalErr, Reduction, Response};

const FIRST_COST: u32 = 8;
const IF_COST: u32 = 31;
const CONS_COST: u32 = 18;
const REST_COST: u32 = 20;
const LISTP_COST: u32 = 5;
const CMP_BASE_COST: u32 = 16;
const CMP_COST_PER_LIMB_DIVIDER: u32 = 64;

impl<'a, T: Allocator> Node<'a, T> {
    pub fn first(&self) -> Result<Node<'a, T>, EvalErr<T::Ptr>> {
        match self.pair() {
            Some((p1, _)) => Ok(self.with_node(p1.node)),
            _ => self.err("first of non-cons"),
        }
    }

    pub fn rest(&self) -> Result<Node<'a, T>, EvalErr<T::Ptr>> {
        match self.pair() {
            Some((_, p2)) => Ok(self.with_node(p2.node)),
            _ => self.err("rest of non-cons"),
        }
    }

    pub fn err<U>(&self, msg: &str) -> Result<U, EvalErr<T::Ptr>> {
        Err(EvalErr(self.node.clone(), msg.into()))
    }
}

pub fn op_if<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    check_arg_count(args, 3, "i")?;
    let cond = args.first()?;
    let mut chosen_node = args.rest()?;
    if cond.nullp() {
        chosen_node = chosen_node.rest()?;
    }
    Ok(Reduction(IF_COST, chosen_node.first()?.node))
}

pub fn op_cons<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    check_arg_count(args, 2, "c")?;
    let a1 = args.first()?;
    let a2 = args.rest()?.first()?;
    let r = args.allocator.new_pair(a1.node, a2.node);
    Ok(Reduction(CONS_COST, r))
}

pub fn op_first<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    check_arg_count(args, 1, "f")?;
    Ok(Reduction(FIRST_COST, args.first()?.first()?.node))
}

pub fn op_rest<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    check_arg_count(args, 1, "r")?;
    Ok(Reduction(REST_COST, args.first()?.rest()?.node))
}

pub fn op_listp<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    check_arg_count(args, 1, "l")?;
    match args.first()?.pair() {
        Some((_first, _rest)) => Ok(Reduction(LISTP_COST, args.one().node)),
        _ => Ok(Reduction(LISTP_COST, args.null().node)),
    }
}

pub fn op_raise<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    args.err("clvm raise")
}

pub fn op_eq<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    check_arg_count(args, 2, "=")?;
    let a0 = args.first()?;
    let a1 = args.rest()?.first()?;
    let s0 = atom(&a0, "=")?;
    let s1 = atom(&a1, "=")?;
    let cost: u32 = CMP_BASE_COST + (s0.len() as u32 + s1.len() as u32) / CMP_COST_PER_LIMB_DIVIDER;
    Ok(Reduction(
        cost,
        if s0 == s1 {
            args.one().node
        } else {
            args.null().node
        },
    ))
}
