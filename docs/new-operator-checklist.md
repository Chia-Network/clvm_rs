# Adding new operators
----------------------

There are two approaches to soft-forking in support for new operators:

1. Adding a new extension to the `softfork` operator (e.g. the BLS operators)
2. Assigning meaning to a, previously unknown, operator. Pick an opcode who's
   cost matches the cost you want your operator to have. The cost of unknown
   operators are defined by a formula, defined
   [here](https://github.com/Chia-Network/clvm_rs/blob/main/src/more_ops.rs#L156-L182).

Using approach (2) only works for operators that unconditionally return nil, and
raise in case of an error. i.e. it can be use for "assert-style" operators that validate
something.

Follow this checklist when adding operators:

* Add test cases in a new file under `op-tests/`. Make sure to include all
  possible ways the operator(s) can fail.
* If relevant, write a script that generates test vectors, printing them into a
  file under `op-tests/` (see `tools/generate-bls-tests.py`). This is to ensure
  the new operator's behavior match at least one other implementation.
* Include the new operators in the fuzzer `fuzz/fuzz_targets/operators.rs`
* Include the new operators and their signatures in `tools/src/bin/generate-fuzz-corpus.rs`.
  Make sure to run this and fuzz for some time before landing the PR.
* extend the benchmark-clvm-cost.rs to include benchmarks for the new operator,
  to establish its cost.
* The opcode decoding and dispatching happens in `src/ChiaDialect.rs`
* Add a new flag (in `src/chia_dialect.rs`) that controls whether the
  operators are activated or not. This is required in order for the chain to exist
  in a state *before* your soft-fork has activated, and behave consistently with
  versions of the node that doesn't know about your new operators.
  Make sure the value of the flag does not collide with any of the flags in
  [chia_rs](https://github.com/Chia-Network/chia_rs/blob/main/src/gen/flags.rs).
  This is a quirk where both of these repos share the same flags space.
* Once a soft-fork has activated, if everything on chain before the softfork is
  compatible with the new rules (which is likely and ought to be the ambition
  with all soft-forks), all logic surrounding activating or deactivating the
  soft-fork should be removed.
