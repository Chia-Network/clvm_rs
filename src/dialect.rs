use crate::allocator::{Allocator, NodePtr};
use crate::cost::Cost;
use crate::reduction::Response;
use crate::run_program::{run_program, OperatorHandler, PreEval};

pub struct Dialect {
    quote_kw: Vec<u8>,
    apply_kw: Vec<u8>,
    op_handler: Box<dyn OperatorHandler>,
}

impl OperatorHandler for Dialect {
    fn op(
        &self,
        allocator: &mut Allocator,
        op: NodePtr,
        args: NodePtr,
        max_cost: Cost,
    ) -> Response {
        self.op_handler.op(allocator, op, args, max_cost)
    }
}

impl Dialect {
    pub fn new(quote_kw: &[u8], apply_kw: &[u8], op_handler: Box<dyn OperatorHandler>) -> Self {
        Dialect {
            quote_kw: quote_kw.to_owned(),
            apply_kw: apply_kw.to_owned(),
            op_handler,
        }
    }

    pub fn run_program_with_pre_eval(
        &self,
        allocator: &mut Allocator,
        program: NodePtr,
        args: NodePtr,
        max_cost: Cost,
        pre_eval: PreEval,
    ) -> Response {
        run_program(
            allocator,
            program,
            args,
            &self.quote_kw,
            &self.apply_kw,
            max_cost,
            self,
            Some(pre_eval),
        )
    }

    pub fn run_program(
        &self,
        allocator: &mut Allocator,
        program: NodePtr,
        args: NodePtr,
        max_cost: Cost,
    ) -> Response {
        run_program(
            allocator,
            program,
            args,
            &self.quote_kw,
            &self.apply_kw,
            max_cost,
            self,
            None,
        )
    }
}
