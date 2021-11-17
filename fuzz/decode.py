# tool to decompile a fuzz_run_program test case to human readable form

import sys
import io
from ir import reader
from clvm_tools import binutils
from clvm.serialize import sexp_from_stream
from clvm import to_sexp_f

with open(sys.argv[1], 'rb') as f:
    blob = f.read()
    sexp = sexp_from_stream(io.BytesIO(blob), to_sexp_f)
    print(binutils.disassemble(sexp))

