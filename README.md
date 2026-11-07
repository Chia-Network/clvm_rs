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

### Miri

Run the library and integration tests under
[Miri](https://github.com/rust-lang/miri):

```sh
rustup toolchain install nightly --component miri
cargo +nightly miri setup
MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri test -p clvmr
MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri test -p clvmr --features allocator-debug
```

CI runs both configurations on pushes and pull requests. Tests that require
BLST native code or take too long under Miri use `#[cfg_attr(miri, ignore = "...")]`
with a reason. Tests that call OpenSSL are ignored only when both Miri and the
`openssl` feature are enabled; the default pure-Rust SHA-256 backend remains
covered. Mixed test tables are split into cases so compatible checks still run.
Ordinary `cargo test` retains its existing coverage and ignored stress tests.
CI runs the operator fixtures separately from the other tests to keep the jobs
shorter; together they cover the same suite as the commands above.
To list skipped tests, run either Miri command with
`-- --ignored --list` (without `--list`, this would execute the skipped tests).

Tree Borrows keeps aliasing checks enabled while accommodating `bitvec`'s pointer
handling, which fails under Stacked Borrows. Miri still warns about that
dependency's integer-to-pointer casts; this limits its ability to check pointer
provenance in those operations.

## Fuzzing

The fuzzing infrastructure for `clvm_rs` uses [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz).

Documentation for setting up fuzzing in rust can be found [here](https://rust-fuzz.github.io/book/cargo-fuzz.html).

To get started, run:

```
cargo fuzz run run-program --jobs=32 -- -rss_limit_mb=4096
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

## Bumping Version Number

Make sure you have `cargo-workspaces` installed:

```bash
cargo install cargo-workspaces
```

To bump the versions of all relevant crates:

```bash
cargo ws version --force "**" --all --no-git-commit
```

Select "minor update" if there has not been any incompatible API changes, otherwise "major update".
