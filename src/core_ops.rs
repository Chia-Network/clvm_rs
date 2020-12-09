use crate::allocator::{NodeT, SExp};
use crate::reduction::Reduction;
use crate::types::EvalErr;

const FIRST_COST: u32 = 10;
const IF_COST: u32 = 10;
const CONS_COST: u32 = 10;
const REST_COST: u32 = 10;
const LISTP_COST: u32 = 10;

impl<'a, T> NodeT<'a, T> {
    pub fn first(&self) -> Result<NodeT<'a, T>, EvalErr<T>> {
        match self.sexp() {
            SExp::Pair(p1, _) => Ok(self.with_node(p1)),
            _ => self.err("first of non-cons"),
        }
    }

    pub fn rest(&self) -> Result<NodeT<'a, T>, EvalErr<T>> {
        match self.sexp() {
            SExp::Pair(_, p2) => Ok(self.with_node(p2)),
            _ => self.err("rest of non-cons"),
        }
    }

    pub fn err<U>(&self, msg: &str) -> Result<U, EvalErr<T>> {
        Err(EvalErr(self.allocator.make_clone(&self.node), msg.into()))
    }

    pub fn nullp(&self) -> bool {
        if let Some(a) = self.atom() {
            a.len() == 0
        } else {
            false
        }
    }

    pub fn from_pair(&self, p1: &Self, p2: &Self) -> T {
        self.allocator.from_pair(&p1.node, &p2.node)
    }

    pub fn null(&self) -> T {
        self.allocator.null()
    }

    pub fn one(&self) -> T {
        self.allocator.one()
    }
}

pub fn op_if<T>(args: &NodeT<T>) -> Result<Reduction<T>, EvalErr<T>> {
    let cond = args.first()?;
    let mut chosen_node = args.rest()?;
    if cond.nullp() {
        chosen_node = chosen_node.rest()?;
    }
    Ok(Reduction(IF_COST, chosen_node.first()?.node))
}

pub fn op_cons<T>(args: &NodeT<T>) -> Result<Reduction<T>, EvalErr<T>> {
    let a1 = args.first()?;
    let a2 = args.rest()?.first()?;
    Ok(Reduction(CONS_COST, args.from_pair(&a1, &a2)))
}

pub fn op_first<T>(args: &NodeT<T>) -> Result<Reduction<T>, EvalErr<T>> {
    Ok(Reduction(FIRST_COST, args.first()?.first()?.node))
}

pub fn op_rest<T>(args: &NodeT<T>) -> Result<Reduction<T>, EvalErr<T>> {
    Ok(Reduction(REST_COST, args.first()?.rest()?.node))
}

pub fn op_listp<T>(args: &NodeT<T>) -> Result<Reduction<T>, EvalErr<T>> {
    match args.first()?.pair() {
        Some((_first, _rest)) => Ok(Reduction(LISTP_COST, args.one())),
        _ => Ok(Reduction(LISTP_COST, args.null())),
    }
}

pub fn op_raise<T>(args: &NodeT<T>) -> Result<Reduction<T>, EvalErr<T>> {
    args.err("clvm raise")
}

pub fn op_eq<T>(args: &NodeT<T>) -> Result<Reduction<T>, EvalErr<T>> {
    let a0 = args.first()?;
    let a1 = args.rest()?.first()?;
    if let Some(s0) = a0.atom() {
        if let Some(s1) = a1.atom() {
            let cost: u32 = s0.len() as u32 + s1.len() as u32;
            return Ok(Reduction(
                cost,
                if s0 == s1 { args.one() } else { args.null() },
            ));
        }
    }
    args.err("= on list")
}
