from .ser import sexp_to_bytes

class EvalError(ValueError):
    def __init__(self, message: str, sexp):
        super().__init__(message)
        self._sexp = sexp

    def __str__(self) -> str:
        return f"({self.args[0]}, {sexp_to_bytes(self._sexp).hex()})"
