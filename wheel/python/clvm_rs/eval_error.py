class EvalError(ValueError):
    def __init__(self, message: str, sexp):
        super().__init__(message)
        self._sexp = sexp
