use crate::allocator::Allocator;
use crate::node::Node;
use crate::op_utils::{atom, check_arg_count};
use crate::reduction::{Reduction, Response};

const FIRST_COST: u32 = 8;
const IF_COST: u32 = 31;
const CONS_COST: u32 = 18;
const REST_COST: u32 = 20;
const LISTP_COST: u32 = 5;
const CMP_BASE_COST: u32 = 16;
const CMP_COST_PER_LIMB_DIVIDER: u32 = 64;

pub fn op_if<T: Allocator>(a: &mut T, input: T::Ptr) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    check_arg_count(&args, 3, "i")?;
    let cond = args.first()?;
    let mut chosen_node = args.rest()?;
    if cond.nullp() {
        chosen_node = chosen_node.rest()?;
    }
    Ok(Reduction(IF_COST, chosen_node.first()?.node))
}

pub fn op_cons<T: Allocator>(a: &mut T, input: T::Ptr) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    check_arg_count(&args, 2, "c")?;
    let a1 = args.first()?;
    let a2 = args.rest()?.first()?;
    let n1 = a1.node;
    let n2 = a2.node;
    let r = a.new_pair(n1, n2);
    Ok(Reduction(CONS_COST, r))
}

pub fn op_first<T: Allocator>(a: &mut T, input: T::Ptr) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    check_arg_count(&args, 1, "f")?;
    Ok(Reduction(FIRST_COST, args.first()?.first()?.node))
}

pub fn op_rest<T: Allocator>(a: &mut T, input: T::Ptr) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    check_arg_count(&args, 1, "r")?;
    Ok(Reduction(REST_COST, args.first()?.rest()?.node))
}

pub fn op_listp<T: Allocator>(a: &mut T, input: T::Ptr) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    check_arg_count(&args, 1, "l")?;
    match args.first()?.pair() {
        Some((_first, _rest)) => Ok(Reduction(LISTP_COST, a.one())),
        _ => Ok(Reduction(LISTP_COST, a.null())),
    }
}

pub fn op_raise<T: Allocator>(a: &mut T, input: T::Ptr) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    args.err("clvm raise")
}

pub fn op_eq<T: Allocator>(a: &mut T, input: T::Ptr) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    check_arg_count(&args, 2, "=")?;
    let a0 = args.first()?;
    let a1 = args.rest()?.first()?;
    let s0 = atom(&a0, "=")?;
    let s1 = atom(&a1, "=")?;
    let cost: u32 = CMP_BASE_COST + (s0.len() as u32 + s1.len() as u32) / CMP_COST_PER_LIMB_DIVIDER;
    Ok(Reduction(cost, if s0 == s1 { a.one() } else { a.null() }))
}
