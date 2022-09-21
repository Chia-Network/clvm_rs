from typing import Tuple

from clvm_rs import CLVMObject, run_serialized_program, NO_NEG_DIV


from .EvalError import EvalError
from .serialize import sexp_to_bytes


DEFAULT_MAX_COST = (1 << 64) - 1
DEFAULT_FLAGS = NO_NEG_DIV


def run_program(
    program: CLVMObject,
    args: CLVMObject,
    max_cost=DEFAULT_MAX_COST,
    flags=DEFAULT_FLAGS,
) -> Tuple[int, CLVMObject]:
    program_blob = sexp_to_bytes(program)
    args_blob = sexp_to_bytes(args)
    cost_or_err_str, result = run_serialized_program(
        program_blob, args_blob, max_cost, flags
    )
    if isinstance(cost_or_err_str, str):
        raise EvalError(cost_or_err_str, result)
    return cost_or_err_str, result
