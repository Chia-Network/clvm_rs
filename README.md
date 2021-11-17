Rust implementation of clvm.

Python Wheel
------------

Use `maturin` to build the python interface. First, install into current virtualenv with

```
$ pip install maturin
```

Build `clvm_rs` directly into the current virtualenv with

```
$ maturin develop --release
```

On UNIX-based platforms, you may get a speed boost on `sha256` operations by building
with OpenSSL.

```
$ maturin develop --release --cargo-extra-args="--features=openssl"
```


To build the wheel, do

```
$ maturin build --release --no-sdist
````

or

```
$ maturin build --release --no-sdist --cargo-extra-args="--features=openssl"
```


WASM
----

Use `wasm-pack` to build the wasm `pkg` file used with npm. Install it with:

```
$ cargo install wasm-pack
```

Then build with

```
$ wasm-pack build --release
```


TESTS
-----
In order to run the unit tests, run:

```
cargo test
```

Fuzzing
-------

The fuzzing infrastructure for `clvm_rs` uses [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz).

Documentation for setting up fuzzing in rust  can be found [here](https://rust-fuzz.github.io/book/cargo-fuzz.html).

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
