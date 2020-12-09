use crate::allocator::Allocator;
use crate::node::Node;
use crate::types::{EvalErr, Reduction};

const FIRST_COST: u32 = 10;
const IF_COST: u32 = 10;
const CONS_COST: u32 = 10;
const REST_COST: u32 = 10;
const LISTP_COST: u32 = 10;

pub fn op_if<T>(allocator: &dyn Allocator<T>, args: &T) -> Result<Reduction<T>, EvalErr<T>>
where
    T: Clone,
{
    let cond = allocator.first(args)?;
    let mut chosen_node = allocator.rest(args)?;
    if allocator.nullp(&cond) {
        chosen_node = allocator.rest(&chosen_node)?;
    }
    Ok(Reduction(IF_COST, allocator.first(&chosen_node)?))
}

pub fn op_cons<T>(allocator: &dyn Allocator<T>, args: &T) -> Result<Reduction<T>, EvalErr<T>>
where
    T: Clone,
{
    let a1 = allocator.first(args)?;
    let a2 = allocator.first(&allocator.rest(args)?)?;
    Ok(Reduction(CONS_COST, allocator.from_pair(&a1, &a2)))
}

pub fn op_first<T>(allocator: &dyn Allocator<T>, args: &T) -> Result<Reduction<T>, EvalErr<T>>
where
    T: Clone,
{
    Ok(Reduction(
        FIRST_COST,
        allocator.first(&allocator.first(args)?)?,
    ))
}

pub fn op_rest(
    allocator: &dyn Allocator<Node>,
    args: &Node,
) -> Result<Reduction<Node>, EvalErr<Node>> {
    Ok(Reduction(
        REST_COST,
        allocator.rest(&allocator.first(args)?)?,
    ))
}

pub fn op_listp(
    allocator: &dyn Allocator<Node>,
    args: &Node,
) -> Result<Reduction<Node>, EvalErr<Node>> {
    match allocator.first(args)?.pair() {
        Some((_first, _rest)) => Ok(Reduction(LISTP_COST, allocator.one())),
        _ => Ok(Reduction(LISTP_COST, allocator.null())),
    }
}

pub fn op_raise(
    _allocator: &dyn Allocator<Node>,
    args: &Node,
) -> Result<Reduction<Node>, EvalErr<Node>> {
    args.err("clvm raise")
}

pub fn op_eq(
    allocator: &dyn Allocator<Node>,
    args: &Node,
) -> Result<Reduction<Node>, EvalErr<Node>> {
    let a0 = allocator.first(args)?;
    let a1 = allocator.first(&allocator.rest(args)?)?;
    if let Some(s0) = a0.atom() {
        if let Some(s1) = a1.atom() {
            let cost: u32 = s0.len() as u32 + s1.len() as u32;
            return Ok(Reduction(
                cost,
                if s0 == s1 {
                    allocator.one()
                } else {
                    allocator.null()
                },
            ));
        }
    }
    args.err("= on list")
}
