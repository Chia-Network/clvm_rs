use crate::allocator::{Allocator, NodePtr};
use crate::cost::Cost;
use crate::operator_handler::OperatorHandler;
use crate::reduction::Response;

use crate::run_program::{run_program, PreEval};

pub struct Dialect<Handler: OperatorHandler> {
    quote_kw: Vec<u8>,
    apply_kw: Vec<u8>,
    op_handler: Handler,
}

impl<Handler: OperatorHandler> OperatorHandler for Dialect<Handler> {
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

impl<Handler: OperatorHandler> Dialect<Handler> {
    pub fn new(quote_kw: &[u8], apply_kw: &[u8], op_handler: Handler) -> Self {
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
            &self.quote_kw,
            &self.apply_kw,
            self,
            program,
            args,
            max_cost,
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
            &self.quote_kw,
            &self.apply_kw,
            self,
            program,
            args,
            max_cost,
            None,
        )
    }
}
