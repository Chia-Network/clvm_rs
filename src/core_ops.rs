use crate::allocator::{Allocator, NodePtr};
use crate::cost::Cost;
use crate::node::Node;
use crate::op_utils::{atom, check_arg_count};
use crate::reduction::{Reduction, Response};

const FIRST_COST: Cost = 30;
const IF_COST: Cost = 33;
// Cons cost lowered from 245. It only allocates a pair, which is small
const CONS_COST: Cost = 50;
// Rest cost lowered from 77 since it doesn't allocate anything and it should be
// the same as first
const REST_COST: Cost = 30;
const LISTP_COST: Cost = 19;
const EQ_BASE_COST: Cost = 117;
const EQ_COST_PER_BYTE: Cost = 1;

pub fn op_if(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 3, "i")?;
    let cond = args.first()?;
    let mut chosen_node = args.rest()?;
    if cond.nullp() {
        chosen_node = chosen_node.rest()?;
    }
    Ok(Reduction(IF_COST, chosen_node.first()?.node))
}

pub fn op_cons(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 2, "c")?;
    let a1 = args.first()?;
    let a2 = args.rest()?.first()?;
    let n1 = a1.node;
    let n2 = a2.node;
    let r = a.new_pair(n1, n2)?;
    Ok(Reduction(CONS_COST, r))
}

pub fn op_first(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 1, "f")?;
    Ok(Reduction(FIRST_COST, args.first()?.first()?.node))
}

pub fn op_rest(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 1, "r")?;
    Ok(Reduction(REST_COST, args.first()?.rest()?.node))
}

pub fn op_listp(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 1, "l")?;
    match args.first()?.pair() {
        Some((_first, _rest)) => Ok(Reduction(LISTP_COST, a.one())),
        _ => Ok(Reduction(LISTP_COST, a.null())),
    }
}

pub fn op_raise(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    // if given a single argument we should raise the single argument rather
    // than the full list of arguments. brun also used to behave this way.
    // if the single argument here is a pair then don't throw it unwrapped
    // as it'd potentially look the same as a throw of multiple arguments.
    let throw_value = args
        .pair()
        .as_ref()
        .and_then(|(_first, _rest)| {
            _first.atom().and_then(|_| {
                if _rest.nullp() {
                    Some(args.first().unwrap())
                } else {
                    None
                }
            })
        })
        .unwrap_or(args);

    throw_value.err("clvm raise")
}

pub fn op_eq(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 2, "=")?;
    let a0 = args.first()?;
    let a1 = args.rest()?.first()?;
    let s0 = atom(&a0, "=")?;
    let s1 = atom(&a1, "=")?;
    let cost = EQ_BASE_COST + (s0.len() as Cost + s1.len() as Cost) * EQ_COST_PER_BYTE;
    Ok(Reduction(cost, if s0 == s1 { a.one() } else { a.null() }))
}
