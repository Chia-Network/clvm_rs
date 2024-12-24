use std::cell::RefCell;

use crate::{
    cost::Cost,
    dialect::{Dialect, OperatorSet},
    reduction::{Reduction, Response},
    secp_ops::{SECP256K1_VERIFY_COST, SECP256R1_VERIFY_COST},
    Allocator, NodePtr,
};

#[derive(Debug, Clone, Copy)]
pub struct CollectedOp {
    pub op: NodePtr,
    pub args: NodePtr,
}

#[derive(Debug, Default, Clone)]
pub struct CollectDialect<T> {
    dialect: T,
    collected_ops: RefCell<Vec<CollectedOp>>,
}

impl<T> CollectDialect<T> {
    pub fn new(dialect: T) -> Self {
        Self {
            dialect,
            collected_ops: RefCell::new(Vec::new()),
        }
    }

    pub fn collect(self) -> Vec<CollectedOp> {
        self.collected_ops.into_inner()
    }
}

impl<T> Dialect for CollectDialect<T>
where
    T: Dialect,
{
    fn apply_kw(&self) -> u32 {
        self.dialect.apply_kw()
    }

    fn quote_kw(&self) -> u32 {
        self.dialect.quote_kw()
    }

    fn softfork_kw(&self) -> u32 {
        self.dialect.softfork_kw()
    }

    fn allow_unknown_ops(&self) -> bool {
        self.dialect.allow_unknown_ops()
    }

    fn softfork_extension(&self, ext: u32) -> OperatorSet {
        self.dialect.softfork_extension(ext)
    }

    fn op(
        &self,
        allocator: &mut Allocator,
        op: NodePtr,
        args: NodePtr,
        max_cost: Cost,
        extensions: OperatorSet,
    ) -> Response {
        let response = self.dialect.op(allocator, op, args, max_cost, extensions);

        let op_len = allocator.atom_len(op);
        if op_len != 4 {
            return response;
        }

        let atom = allocator.atom(op);
        let opcode = u32::from_be_bytes(atom.as_ref().try_into().unwrap());

        let cost = match opcode {
            // We special case these opcodes and allow the response to pass through otherwise.
            // If new operators are added to the main dialect, they likely shouldn't be included here.
            // We're using the same cost to ensure that softfork conditions behave the same.
            0x13d61f00 => SECP256K1_VERIFY_COST,
            0x1c3a8f00 => SECP256R1_VERIFY_COST,
            _ => return response,
        };

        self.collected_ops
            .borrow_mut()
            .push(CollectedOp { op, args });

        Ok(Reduction(cost, NodePtr::NIL))
    }
}

#[cfg(test)]
mod tests {
    use crate::{run_program, ChiaDialect, SExp};

    use super::*;

    #[test]
    fn test_signature_collection() -> anyhow::Result<()> {
        let mut a = Allocator::new();

        let op = a.new_atom(&[0x13, 0xd6, 0x1f, 0x00])?;
        let fake_arg = a.new_atom(&[1, 2, 3])?;
        let op_q = a.one();
        let quoted_fake_arg = a.new_pair(op_q, fake_arg)?;
        let args = a.new_pair(quoted_fake_arg, NodePtr::NIL)?;
        let program = a.new_pair(op, args)?;

        let dialect = CollectDialect::new(ChiaDialect::new(0));

        let reduction = run_program(&mut a, &dialect, program, NodePtr::NIL, u64::MAX).unwrap();
        let collected = dialect.collect();

        assert!(a.atom(reduction.1).is_empty());
        assert_eq!(collected.len(), 1);

        let collected = collected[0];
        assert_eq!(collected.op, op);

        let SExp::Pair(f, r) = a.sexp(collected.args) else {
            unreachable!();
        };
        assert!(a.atom(r).is_empty());
        assert_eq!(f, fake_arg);

        Ok(())
    }
}
