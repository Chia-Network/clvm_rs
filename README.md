Rust implementation of clvm.

![GitHub](https://img.shields.io/github/license/Chia-Network/clvm_rs?logo=Github)
[![Coverage Status](https://coveralls.io/repos/github/Chia-Network/clvm_rs/badge.svg?branch=main)](https://coveralls.io/github/Chia-Network/clvm_rs?branch=main)
![Build Crate](https://github.com/Chia-Network/clvm_rs/actions/workflows/build-crate.yml/badge.svg)
![Build Wheels](https://github.com/Chia-Network/clvm_rs/actions/workflows/build-test.yml/badge.svg)

![PyPI](https://img.shields.io/pypi/v/clvm_rs?logo=pypi)
[![Crates.io](https://img.shields.io/crates/v/clvmr.svg)](https://crates.io/crates/clvmr)

The cargo workspace includes an rlib crate, for use with rust or other applications, and a python wheel.

The python wheel is in `wheel`. The npm package is in `wasm`.

## Tests

In order to run the unit tests, run:

```
cargo test
```

## Fuzzing

The fuzzing infrastructure for `clvm_rs` uses [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz).

Documentation for setting up fuzzing in rust can be found [here](https://rust-fuzz.github.io/book/cargo-fuzz.html).

To generate an initial corpus (for the `run_program` fuzzer), run:

```
cd tools
cargo run generate-fuzz-corpus
```

To get started, run:

```
cargo fuzz run fuzz_run_program --jobs=32 -- -rss_limit_mb=4096
```

But with whatever number of jobs works best for you.

If you find issues in `clvm_rs` please use our [bug bounty program](https://hackerone.com/chia_network).

## Build Wheel

The `clvm_rs` wheel has python bindings for the rust implementation of clvm.

Use `maturin` to build the python interface. First, install into current virtualenv with

```
$ pip install maturin
```

While in the `wheel` directory, build `clvm_rs` into the current virtualenv with

```
$ maturin develop --release
```

On UNIX-based platforms, you may get a speed boost on `sha256` operations by building
with OpenSSL.

```
$ maturin develop --release --features=openssl
```

To build the wheel, do

```
$ maturin build --release
```

or

```
$ maturin build --release --features=openssl
```
