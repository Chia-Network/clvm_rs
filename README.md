Rust implementation of clvm.

The cargo workspace includes an rlib crate, for use with rust or other applications, and a python wheel.

The python wheel is in `wheel`. The npm package is in `wasm`.


TESTS
-----
In order to run the unit tests, run:

```
cargo test
```

Fuzzing
-------

The fuzzing infrastructure for `clvm_rs` uses [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz).

Documentation for setting up fuzzing in rust can be found [here](https://rust-fuzz.github.io/book/cargo-fuzz.html).

To generate an initial corpus (for the `run_program` fuzzer), run:

```
cd fuzz
mkdir -p corpus/fuzz_run_program/
python gen_corpus.py
```

To get started, run:

```
cargo fuzz run fuzz_run_program --jobs=32 -- -rss_limit_mb=4096
```

But with whatever number of jobs works best for you.

If you find issues in `clvm_rs` please see the [Bug Bounty program](https://www.chia.net/2021/10/21/bugcrowd-bounty-launch.en.html).
