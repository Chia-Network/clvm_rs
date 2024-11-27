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
