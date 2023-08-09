use crate::allocator::{Allocator, NodePtr, SExp};
use crate::cost::Cost;
use crate::err_utils::err;
use crate::op_utils::{first, get_args, nullp, rest};
use crate::reduction::{EvalErr, Reduction, Response};

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
    let [cond, affirmative, negative] = get_args::<3>(a, input, "i")?;
    let chosen_node = if nullp(a, cond) {
        negative
    } else {
        affirmative
    };
    Ok(Reduction(IF_COST, chosen_node))
}

pub fn op_cons(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n1, n2] = get_args::<2>(a, input, "c")?;
    let r = a.new_pair(n1, n2)?;
    Ok(Reduction(CONS_COST, r))
}

pub fn op_first(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n] = get_args::<1>(a, input, "f")?;
    Ok(Reduction(FIRST_COST, first(a, n)?))
}

pub fn op_rest(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n] = get_args::<1>(a, input, "r")?;
    Ok(Reduction(REST_COST, rest(a, n)?))
}

pub fn op_listp(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n] = get_args::<1>(a, input, "l")?;
    match a.sexp(n) {
        SExp::Pair(_, _) => Ok(Reduction(LISTP_COST, a.one())),
        _ => Ok(Reduction(LISTP_COST, a.null())),
    }
}

pub fn op_raise(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    // if given a single argument we should raise the single argument rather
    // than the full list of arguments. brun also used to behave this way.
    // if the single argument here is a pair then don't throw it unwrapped
    // as it'd potentially look the same as a throw of multiple arguments.
    let throw_value = if let Ok([value]) = get_args::<1>(a, input, "") {
        match a.sexp(value) {
            SExp::Atom => value,
            _ => input,
        }
    } else {
        input
    };

    err(throw_value, "clvm raise")
}

fn ensure_atom(a: &Allocator, n: NodePtr, op: &str) -> Result<(), EvalErr> {
    if let SExp::Atom = a.sexp(n) {
        Ok(())
    } else {
        Err(EvalErr(n, format!("{op} on list")))
    }
}

pub fn op_eq(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [s0, s1] = get_args::<2>(a, input, "=")?;
    ensure_atom(a, s0, "=")?;
    ensure_atom(a, s1, "=")?;
    let eq = a.atom_eq(s0, s1);
    let cost = EQ_BASE_COST + (a.atom_len(s0) as Cost + a.atom_len(s1) as Cost) * EQ_COST_PER_BYTE;
    Ok(Reduction(cost, if eq { a.one() } else { a.null() }))
}
